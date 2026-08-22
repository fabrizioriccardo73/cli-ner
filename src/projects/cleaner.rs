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
    pub total_bytes_freed: u64,
    pub items_cleaned: usize,
    pub items_failed: usize,
    pub details: Vec<CleanedItem>,
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

        // 1. Strict safety validation
        if let Err(val_err) =
            validate_project_artifact_for_cleaning(&artifact.path, &project.root_path)
        {
            let status = ActionStatus::Failed(format!("Safety check failed: {}", val_err));
            let action = if dry_run {
                ActionType::DryRun
            } else if force_permanent {
                ActionType::PermanentDelete
            } else {
                ActionType::Trash
            };

            operation_record.add_item(path_str.clone(), 0, action.clone(), status.clone());
            result.items_failed += 1;
            result.details.push(CleanedItem {
                path: path_str,
                size_bytes: 0,
                action,
                status,
            });
            continue;
        }

        // 2. Perform clean or simulate
        if dry_run {
            let status = ActionStatus::Success;
            let action = ActionType::DryRun;

            operation_record.add_item(
                path_str.clone(),
                artifact.size_bytes,
                action.clone(),
                status.clone(),
            );
            result.total_bytes_freed += artifact.size_bytes;
            result.items_cleaned += 1;
            result.details.push(CleanedItem {
                path: path_str,
                size_bytes: artifact.size_bytes,
                action,
                status,
            });
        } else if force_permanent {
            match fs::remove_dir_all(&artifact.path) {
                Ok(_) => {
                    let status = ActionStatus::Success;
                    let action = ActionType::PermanentDelete;

                    operation_record.add_item(
                        path_str.clone(),
                        artifact.size_bytes,
                        action.clone(),
                        status.clone(),
                    );
                    result.total_bytes_freed += artifact.size_bytes;
                    result.items_cleaned += 1;
                    result.details.push(CleanedItem {
                        path: path_str,
                        size_bytes: artifact.size_bytes,
                        action,
                        status,
                    });
                }
                Err(e) => {
                    let status = ActionStatus::Failed(e.to_string());
                    let action = ActionType::PermanentDelete;

                    operation_record.add_item(
                        path_str.clone(),
                        0,
                        action.clone(),
                        status.clone(),
                    );
                    result.items_failed += 1;
                    result.details.push(CleanedItem {
                        path: path_str,
                        size_bytes: 0,
                        action,
                        status,
                    });
                }
            }
        } else {
            match move_to_trash_silent(&artifact.path) {
                Ok(_) => {
                    let status = ActionStatus::Success;
                    let action = ActionType::Trash;

                    operation_record.add_item(
                        path_str.clone(),
                        artifact.size_bytes,
                        action.clone(),
                        status.clone(),
                    );
                    result.total_bytes_freed += artifact.size_bytes;
                    result.items_cleaned += 1;
                    result.details.push(CleanedItem {
                        path: path_str,
                        size_bytes: artifact.size_bytes,
                        action,
                        status,
                    });
                }
                Err(e) => {
                    let status = ActionStatus::Failed(e.to_string());
                    let action = ActionType::Trash;

                    operation_record.add_item(
                        path_str.clone(),
                        0,
                        action.clone(),
                        status.clone(),
                    );
                    result.items_failed += 1;
                    result.details.push(CleanedItem {
                        path: path_str,
                        size_bytes: 0,
                        action,
                        status,
                    });
                }
            }
        }
    }

    operation_record.set_duration(start_time.elapsed());
    let _ = save_operation_log(&operation_record);

    Ok(result)
}
