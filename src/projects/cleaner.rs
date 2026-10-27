use crate::projects::models::{DiscoveredProject, ProjectArtifact};
use crate::report::operation_log::{
    save_operation_log, ActionStatus, ActionType, CleanedItem, OperationRecord,
};
use crate::safety::validator::validate_project_artifact_for_cleaning;
use crate::utils::fs::move_to_trash_silent;
use anyhow::Result;
use std::fs;
use std::time::Instant;

#[derive(Debug, Default)]
pub struct ProjectCleanResult {
    /// Bytes removed from the volume immediately (`--force` permanent deletes).
    pub total_bytes_freed: u64,
    /// Bytes moved into `~/.Trash`: still on disk until the Trash is emptied.
    pub total_bytes_trashed: u64,
    pub items_cleaned: usize,
    pub items_failed: usize,
    pub details: Vec<CleanedItem>,
}

impl ProjectCleanResult {
    /// Everything acted on, whether already gone or waiting in the Trash.
    pub fn total_bytes_processed(&self) -> u64 {
        self.total_bytes_freed + self.total_bytes_trashed
    }
}

/// Outcome of acting on one artifact.
enum Outcome {
    /// Removed for good; carries the bytes reclaimed.
    Deleted(u64),
    /// Moved to the Trash; carries the bytes moved and the destination path.
    Trashed(u64, String),
    /// Simulated only; carries the bytes that would have been reclaimed.
    Simulated(u64),
    Failed(String),
}

/// Safely cleans selected project build artifacts (moving to Trash or permanent delete).
pub fn clean_project_artifacts(
    targets: &[(&DiscoveredProject, &ProjectArtifact)],
    dry_run: bool,
    force_permanent: bool,
) -> Result<ProjectCleanResult> {
    let start_time = Instant::now();
    let mut operation_record = OperationRecord::new("projects", "projects-sweep", dry_run);
    let mut result = ProjectCleanResult::default();

    for (project, artifact) in targets {
        let path_str = artifact.path.display().to_string();

        let action = if dry_run {
            ActionType::DryRun
        } else if force_permanent {
            ActionType::PermanentDelete
        } else {
            ActionType::Trash
        };

        // 1. Strict safety validation, then act.
        let outcome =
            match validate_project_artifact_for_cleaning(&artifact.path, &project.root_path) {
                Err(val_err) => Outcome::Failed(format!("Safety check failed: {}", val_err)),
                Ok(()) if dry_run => Outcome::Simulated(artifact.size_bytes),
                Ok(()) if force_permanent => match fs::remove_dir_all(&artifact.path) {
                    Ok(()) => Outcome::Deleted(artifact.size_bytes),
                    Err(e) => Outcome::Failed(e.to_string()),
                },
                Ok(()) => match move_to_trash_silent(&artifact.path) {
                    Ok(dest) => Outcome::Trashed(artifact.size_bytes, dest.display().to_string()),
                    Err(e) => Outcome::Failed(e.to_string()),
                },
            };

        // 2. Record it once, in both the audit log and the returned summary.
        let (size, status, trashed_to) = match outcome {
            Outcome::Deleted(size) => {
                result.total_bytes_freed += size;
                result.items_cleaned += 1;
                (size, ActionStatus::Success, None)
            }
            Outcome::Trashed(size, dest) => {
                result.total_bytes_trashed += size;
                result.items_cleaned += 1;
                (size, ActionStatus::Success, Some(dest))
            }
            Outcome::Simulated(size) => {
                result.total_bytes_freed += size;
                result.items_cleaned += 1;
                (size, ActionStatus::Success, None)
            }
            Outcome::Failed(err) => {
                result.items_failed += 1;
                (0, ActionStatus::Failed(err), None)
            }
        };

        operation_record.add_item(
            path_str.clone(),
            size,
            action.clone(),
            status.clone(),
            trashed_to.clone(),
        );
        result.details.push(CleanedItem {
            path: path_str,
            size_bytes: size,
            action,
            status,
            trashed_to,
        });
    }

    operation_record.set_duration(start_time.elapsed());
    let _ = save_operation_log(&operation_record);

    Ok(result)
}
