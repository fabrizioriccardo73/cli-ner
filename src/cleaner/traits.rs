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
    /// Where the item landed inside `~/.Trash`, so the user can put it back by hand.
    pub trashed_to: Option<String>,
}

impl ExecutionItemResult {
    pub fn new(path: String, size_bytes: u64, action: ActionType, status: ActionStatus) -> Self {
        Self {
            path,
            size_bytes,
            action,
            status,
            trashed_to: None,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub struct CleanResult {
    /// Bytes that are gone from the volume right now (permanent deletes, external
    /// tools that really removed data).
    pub total_bytes_freed: u64,
    /// Bytes that were only moved into `~/.Trash`. Still occupying the disk until
    /// the Trash is emptied — reported separately so the summary cannot overstate.
    pub total_bytes_trashed: u64,
    pub items_cleaned: usize,
    pub items_skipped: usize,
    pub items_failed: usize,
    pub details: Vec<ExecutionItemResult>,
}

impl CleanResult {
    /// Everything the operation acted on, whether it is already gone or sitting in the Trash.
    pub fn total_bytes_processed(&self) -> u64 {
        self.total_bytes_freed + self.total_bytes_trashed
    }

    pub fn merge(&mut self, other: CleanResult) {
        self.total_bytes_freed += other.total_bytes_freed;
        self.total_bytes_trashed += other.total_bytes_trashed;
        self.items_cleaned += other.items_cleaned;
        self.items_skipped += other.items_skipped;
        self.items_failed += other.items_failed;
        self.details.extend(other.details);
    }
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

    /// Whether `--category all` includes this cleaner. Cleaners that touch
    /// personal data rather than caches return `false` and only run when their
    /// category is requested by name.
    fn included_in_default_run(&self) -> bool {
        true
    }

    /// Whether this cleaner removes data for good even without `--force`: it runs
    /// an external tool (brew, npm, docker, …) or empties the Trash, so nothing
    /// lands in `~/.Trash` to restore. The confirmation prompt lists these apart.
    fn deletes_permanently(&self) -> bool {
        false
    }

    /// Scan target paths to estimate reclaimable space
    fn scan(&self) -> Result<Vec<CleanTargetItem>>;

    /// Execute the cleaning against an explicit, already-scanned list of targets.
    ///
    /// Callers must pass the very same items the user was shown and approved. Re-scanning
    /// here would let the target set drift between the preview and the deletion — and, for
    /// the Trash cleaner running after the others, would permanently destroy the files this
    /// same run had just moved into the Trash for safekeeping.
    fn clean_items(
        &self,
        targets: &[CleanTargetItem],
        dry_run: bool,
        force_permanent: bool,
    ) -> Result<CleanResult> {
        trash_or_delete_targets(targets, dry_run, force_permanent)
    }

    /// Convenience wrapper: scan and immediately clean. Prefer `clean_items` with a
    /// previously reviewed target list whenever the user has been shown a preview.
    fn clean(&self, dry_run: bool, force_permanent: bool) -> Result<CleanResult> {
        let targets = self.scan()?;
        self.clean_items(&targets, dry_run, force_permanent)
    }
}

/// The standard safety-checked removal loop: validate each path, then either simulate,
/// permanently delete, or move it to the Trash.
///
/// Lives outside the trait so a cleaner that overrides `clean_items` to bolt on an extra
/// step can still delegate the file handling here rather than duplicating it.
pub fn trash_or_delete_targets(
    targets: &[CleanTargetItem],
    dry_run: bool,
    force_permanent: bool,
) -> Result<CleanResult> {
    let mut result = CleanResult::default();

    for item in targets {
        // Safety validation
        let validation = match validate_path_for_cleaning(&item.path) {
            Ok(val) => val,
            Err(err) => {
                result.items_skipped += 1;
                result.details.push(ExecutionItemResult::new(
                    item.path.display().to_string(),
                    item.size_bytes,
                    ActionType::DryRun,
                    ActionStatus::Skipped(err.to_string()),
                ));
                continue;
            }
        };

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

        // Real execution
        if force_permanent {
            let remove_res = if validation.canonical_path.is_symlink() {
                fs::remove_file(&validation.canonical_path)
            } else if validation.canonical_path.is_dir() {
                fs::remove_dir_all(&validation.canonical_path)
            } else {
                fs::remove_file(&validation.canonical_path)
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
        } else {
            // Default: silently move to macOS Trash
            match move_to_trash_silent(&validation.canonical_path) {
                Ok(dest) => {
                    // Nothing is freed yet — the bytes simply moved to ~/.Trash.
                    result.total_bytes_trashed += item.size_bytes;
                    result.items_cleaned += 1;
                    let mut detail = ExecutionItemResult::new(
                        item.path.display().to_string(),
                        item.size_bytes,
                        ActionType::Trash,
                        ActionStatus::Success,
                    );
                    detail.trashed_to = Some(dest.display().to_string());
                    result.details.push(detail);
                }
                Err(err) => {
                    result.items_failed += 1;
                    result.details.push(ExecutionItemResult::new(
                        item.path.display().to_string(),
                        item.size_bytes,
                        ActionType::Trash,
                        ActionStatus::Failed(err.to_string()),
                    ));
                }
            }
        }
    }

    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::fs::expand_tilde;
    use std::io::Write;

    /// Build a throwaway directory under an allowlisted root so the real validation
    /// path runs, and return it plus the size the cleaner should attribute to it.
    fn scratch_cache_dir(tag: &str) -> (PathBuf, CleanTargetItem) {
        let dir = expand_tilde("~/Library/Caches").join(format!("cli-ner-selftest-{tag}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let mut f = fs::File::create(dir.join("payload.bin")).unwrap();
        f.write_all(&vec![b'x'; 8192]).unwrap();

        let (size, count) = crate::utils::fs::calculate_size(&dir).unwrap();
        let item = CleanTargetItem {
            path: dir.clone(),
            size_bytes: size,
            file_count: count,
            description: "self-test".into(),
        };
        (dir, item)
    }

    /// A dry run must leave the filesystem completely untouched.
    #[test]
    fn test_dry_run_does_not_touch_anything() {
        let (dir, item) = scratch_cache_dir("dryrun");

        let result = trash_or_delete_targets(&[item], true, false).unwrap();

        assert!(dir.exists(), "dry run must not remove the target");
        assert_eq!(result.items_cleaned, 1);
        assert_eq!(result.total_bytes_trashed, 0);

        let _ = fs::remove_dir_all(&dir);
    }

    /// The default path moves to the Trash: the bytes are *not* freed yet, they are
    /// only relocated, and the destination is recorded so the item can be restored.
    #[test]
    fn test_trash_move_is_reported_as_trashed_not_freed() {
        let (dir, item) = scratch_cache_dir("trash");
        let expected_size = item.size_bytes;

        let result = trash_or_delete_targets(&[item], false, false).unwrap();

        assert!(!dir.exists(), "the target must have moved");
        assert_eq!(result.items_cleaned, 1);
        assert_eq!(
            result.total_bytes_freed, 0,
            "moving to the Trash frees nothing until the Trash is emptied"
        );
        assert_eq!(result.total_bytes_trashed, expected_size);

        let dest = result.details[0]
            .trashed_to
            .as_ref()
            .expect("the Trash destination must be recorded for restoring");
        assert!(PathBuf::from(dest).exists());

        let _ = fs::remove_dir_all(dest);
    }

    /// `--force` really deletes, and only then are the bytes reported as freed.
    #[test]
    fn test_force_delete_is_reported_as_freed() {
        let (dir, item) = scratch_cache_dir("force");
        let expected_size = item.size_bytes;

        let result = trash_or_delete_targets(&[item], false, true).unwrap();

        assert!(!dir.exists());
        assert_eq!(result.total_bytes_freed, expected_size);
        assert_eq!(result.total_bytes_trashed, 0);
        assert!(result.details[0].trashed_to.is_none());
    }

    /// Paths outside the allowlist are refused even when handed in directly.
    #[test]
    fn test_target_outside_allowlist_is_skipped() {
        let item = CleanTargetItem {
            path: expand_tilde("~/Documents"),
            size_bytes: 1_000,
            file_count: 1,
            description: "should never be touched".into(),
        };

        let result = trash_or_delete_targets(&[item], false, true).unwrap();

        assert_eq!(result.items_skipped, 1);
        assert_eq!(result.items_cleaned, 0);
        assert_eq!(result.total_bytes_freed, 0);
        assert!(expand_tilde("~/Documents").exists());
    }
}
