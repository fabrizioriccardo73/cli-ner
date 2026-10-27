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

    /// Emptying the Trash is always permanent, so it acts strictly on `targets` — the
    /// list captured before any other cleaner ran. Re-scanning here would sweep up
    /// whatever the same run had just moved into the Trash and delete it for good.
    fn clean_items(
        &self,
        targets: &[CleanTargetItem],
        dry_run: bool,
        _force_permanent: bool,
    ) -> Result<CleanResult> {
        let mut result = CleanResult::default();

        for item in targets {
            if dry_run {
                result.total_bytes_freed += item.size_bytes;
                result.items_cleaned += 1;
                result.details.push(ExecutionItemResult::new(
                    item.path.display().to_string(),
                    item.size_bytes,
                    ActionType::DryRun,
                    ActionStatus::Success,
                ));
                continue;
            }

            // Guard: only ever touch entries that are still directly inside ~/.Trash.
            let trash_dir = expand_tilde("~/.Trash");
            if item.path.parent() != Some(trash_dir.as_path()) {
                result.items_skipped += 1;
                result.details.push(ExecutionItemResult::new(
                    item.path.display().to_string(),
                    item.size_bytes,
                    ActionType::PermanentDelete,
                    ActionStatus::Skipped("Not a direct child of ~/.Trash".into()),
                ));
                continue;
            }

            let remove_res = if item.path.is_symlink() {
                fs::remove_file(&item.path)
            } else if item.path.is_dir() {
                fs::remove_dir_all(&item.path)
            } else {
                fs::remove_file(&item.path)
            };

            match remove_res {
                Ok(_) => {
                    result.total_bytes_freed += item.size_bytes;
                    result.items_cleaned += 1;
                    result.details.push(ExecutionItemResult::new(
                        item.path.display().to_string(),
                        item.size_bytes,
                        ActionType::PermanentDelete,
                        ActionStatus::Success,
                    ));
                }
                Err(err) => {
                    result.items_failed += 1;
                    result.details.push(ExecutionItemResult::new(
                        item.path.display().to_string(),
                        item.size_bytes,
                        ActionType::PermanentDelete,
                        ActionStatus::Failed(err.to_string()),
                    ));
                }
            }
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cleaner::traits::CleanTargetItem;

    fn item(path: &str) -> CleanTargetItem {
        CleanTargetItem {
            path: path.into(),
            size_bytes: 1_000,
            file_count: 1,
            description: String::new(),
        }
    }

    /// Emptying the Trash is irreversible, so it must act only on the list captured
    /// before the run started. Anything outside `~/.Trash` is refused outright, which
    /// is what keeps `--category all` from permanently destroying the caches it had
    /// just moved there for safekeeping.
    #[test]
    fn test_refuses_paths_outside_trash() {
        let outside = expand_tilde("~/Library/Caches/some-app");
        let result = TrashCleaner
            .clean_items(&[item(outside.to_str().unwrap())], false, false)
            .unwrap();

        assert_eq!(result.items_skipped, 1);
        assert_eq!(result.items_cleaned, 0);
        assert_eq!(result.total_bytes_freed, 0);
    }

    /// Nested paths are refused too: only direct children of ~/.Trash are ever removed.
    #[test]
    fn test_refuses_nested_paths_inside_trash() {
        let nested = expand_tilde("~/.Trash/folder/inner-file");
        let result = TrashCleaner
            .clean_items(&[item(nested.to_str().unwrap())], false, false)
            .unwrap();

        assert_eq!(result.items_skipped, 1);
        assert_eq!(result.items_cleaned, 0);
    }

    /// A dry run touches nothing and reports what would go.
    #[test]
    fn test_dry_run_removes_nothing() {
        let entry = expand_tilde("~/.Trash/whatever");
        let result = TrashCleaner
            .clean_items(&[item(entry.to_str().unwrap())], true, false)
            .unwrap();

        assert_eq!(result.items_cleaned, 1);
        assert_eq!(result.total_bytes_freed, 1_000);
        assert!(!entry.exists());
    }
}
