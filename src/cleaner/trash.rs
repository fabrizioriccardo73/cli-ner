use crate::cleaner::traits::{CleanResult, CleanTargetItem, Cleaner, ExecutionItemResult};
use crate::report::operation_log::{ActionStatus, ActionType};
use crate::safety::allowlist::CleanCategory;
use crate::utils::fs::{calculate_size, expand_tilde};
use anyhow::Result;
use std::fs;

pub struct TrashCleaner;

impl Cleaner for TrashCleaner {
    fn name(&self) -> &'static str {
        "macOS Trash"
    }

    fn category(&self) -> CleanCategory {
        CleanCategory::Trash
    }

    fn description(&self) -> &'static str {
        "Empties files and folders currently in the user's macOS Trash (~/.Trash)"
    }

    fn scan(&self) -> Result<Vec<CleanTargetItem>> {
        let trash_dir = expand_tilde("~/.Trash");
        let mut items = Vec::new();

        if trash_dir.is_dir() {
            for entry in fs::read_dir(&trash_dir)?.flatten() {
                let path = entry.path();
                if let Ok((size, count)) = calculate_size(&path) {
                    let name = entry.file_name().to_string_lossy().to_string();
                    items.push(CleanTargetItem {
                        path,
                        size_bytes: size,
                        file_count: count,
                        description: format!("Trash item: {}", name),
                    });
                }
            }
        }

        items.sort_by_key(|b| std::cmp::Reverse(b.size_bytes));
        Ok(items)
    }

    fn clean(&self, dry_run: bool, _force_permanent: bool) -> Result<CleanResult> {
        let targets = self.scan()?;
        let mut result = CleanResult::default();

        for item in targets {
            if dry_run {
                result.total_bytes_freed += item.size_bytes;
                result.items_cleaned += 1;
                result.details.push(ExecutionItemResult {
                    path: item.path.display().to_string(),
                    size_bytes: item.size_bytes,
                    action: ActionType::DryRun,
                    status: ActionStatus::Success,
                });
                continue;
            }

            let remove_res = if item.path.is_dir() {
                fs::remove_dir_all(&item.path)
            } else {
                fs::remove_file(&item.path)
            };

            match remove_res {
                Ok(_) => {
                    result.total_bytes_freed += item.size_bytes;
                    result.items_cleaned += 1;
                    result.details.push(ExecutionItemResult {
                        path: item.path.display().to_string(),
                        size_bytes: item.size_bytes,
                        action: ActionType::PermanentDelete,
                        status: ActionStatus::Success,
                    });
                }
                Err(err) => {
                    result.items_failed += 1;
                    result.details.push(ExecutionItemResult {
                        path: item.path.display().to_string(),
                        size_bytes: item.size_bytes,
                        action: ActionType::PermanentDelete,
                        status: ActionStatus::Failed(err.to_string()),
                    });
                }
            }
        }

        Ok(result)
    }
}
