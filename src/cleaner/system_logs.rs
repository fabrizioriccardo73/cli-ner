use crate::cleaner::traits::{CleanTargetItem, Cleaner};
use crate::safety::allowlist::CleanCategory;
use crate::utils::fs::{calculate_size, expand_tilde};
use anyhow::Result;
use std::fs;

pub struct UserLogsCleaner;

impl Cleaner for UserLogsCleaner {
    fn name(&self) -> &'static str {
        "User Log Files"
    }

    fn category(&self) -> CleanCategory {
        CleanCategory::UserLogs
    }

    fn description(&self) -> &'static str {
        "Removes user application and diagnostic logs in ~/Library/Logs"
    }

    fn scan(&self) -> Result<Vec<CleanTargetItem>> {
        let logs_dir = expand_tilde("~/Library/Logs");
        let mut items = Vec::new();

        if logs_dir.is_dir() {
            for entry in fs::read_dir(&logs_dir)?.flatten() {
                let path = entry.path();
                if let Ok((size, count)) = calculate_size(&path) {
                    if size > 0 {
                        let name = entry.file_name().to_string_lossy().to_string();
                        items.push(CleanTargetItem {
                            path,
                            size_bytes: size,
                            file_count: count,
                            description: format!("Logs for {}", name),
                        });
                    }
                }
            }
        }

        items.sort_by_key(|b| std::cmp::Reverse(b.size_bytes));
        Ok(items)
    }
}
