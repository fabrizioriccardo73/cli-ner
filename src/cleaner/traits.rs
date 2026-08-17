use crate::report::operation_log::{ActionStatus, ActionType};
use crate::safety::allowlist::CleanCategory;
use crate::safety::validator::validate_path_for_cleaning;
use crate::utils::fs::move_to_trash_silent;
use anyhow::Result;
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CleanTargetItem {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub file_count: usize,
    pub description: String,
}

#[derive(Debug, Clone)]
pub struct ExecutionItemResult {
    pub path: String,
    pub size_bytes: u64,
    pub action: ActionType,
    pub status: ActionStatus,
}

#[derive(Debug, Clone, Default)]
pub struct CleanResult {
    pub total_bytes_freed: u64,
    pub items_cleaned: usize,
    pub items_skipped: usize,
    pub items_failed: usize,
    pub details: Vec<ExecutionItemResult>,
}

/// Core trait implemented by all clean modules.
#[allow(dead_code)]
pub trait Cleaner: Send + Sync {
    /// Friendly display name
    fn name(&self) -> &'static str;

    /// Category for filtering
    fn category(&self) -> CleanCategory;

    /// Detailed description of what this cleaner targets
    fn description(&self) -> &'static str;

    /// Whether this cleaner is available on current system
    fn is_available(&self) -> bool {
        true
    }

    /// Scan target paths to estimate reclaimable space
    fn scan(&self) -> Result<Vec<CleanTargetItem>>;

    /// Execute cleaning operation (or simulate in dry-run)
    fn clean(&self, dry_run: bool, force_permanent: bool) -> Result<CleanResult> {
        let targets = self.scan()?;
        let mut result = CleanResult::default();

        for item in targets {
            // Safety validation
            let validation = match validate_path_for_cleaning(&item.path) {
                Ok(val) => val,
                Err(err) => {
                    result.items_skipped += 1;
                    result.details.push(ExecutionItemResult {
                        path: item.path.display().to_string(),
                        size_bytes: item.size_bytes,
                        action: ActionType::DryRun,
                        status: ActionStatus::Skipped(err.to_string()),
                    });
                    continue;
                }
            };

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

            // Real execution
            if force_permanent {
                let remove_res = if validation.canonical_path.is_dir() {
                    fs::remove_dir_all(&validation.canonical_path)
                } else {
                    fs::remove_file(&validation.canonical_path)
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
            } else {
                // Default: silently move to macOS Trash
                match move_to_trash_silent(&validation.canonical_path) {
                    Ok(_) => {
                        result.total_bytes_freed += item.size_bytes;
                        result.items_cleaned += 1;
                        result.details.push(ExecutionItemResult {
                            path: item.path.display().to_string(),
                            size_bytes: item.size_bytes,
                            action: ActionType::Trash,
                            status: ActionStatus::Success,
                        });
                    }
                    Err(err) => {
                        result.items_failed += 1;
                        result.details.push(ExecutionItemResult {
                            path: item.path.display().to_string(),
                            size_bytes: item.size_bytes,
                            action: ActionType::Trash,
                            status: ActionStatus::Failed(err.to_string()),
                        });
                    }
                }
            }
        }

        Ok(result)
    }
}
