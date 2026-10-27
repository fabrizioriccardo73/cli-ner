use crate::cleaner::traits::{CleanTargetItem, Cleaner};
use crate::safety::allowlist::CleanCategory;
use crate::utils::fs::{calculate_size, is_older_than};
use anyhow::Result;
use std::fs;
use std::path::Path;
use std::time::Duration;

/// Temp entries younger than this are left alone.
///
/// `/private/tmp` and `/private/var/tmp` hold scratch files, sockets and lock files
/// belonging to processes that are running right now; removing those mid-flight breaks
/// live applications. Anything untouched for a day is safely abandoned work.
const MIN_AGE_BEFORE_CLEANING: Duration = Duration::from_secs(24 * 60 * 60);

pub struct TempFilesCleaner;

impl Cleaner for TempFilesCleaner {
    fn name(&self) -> &'static str {
        "Temporary System & App Files"
    }

    fn category(&self) -> CleanCategory {
        CleanCategory::TempFiles
    }

    fn description(&self) -> &'static str {
        "Cleans temporary files and folders in /private/tmp and /private/var/tmp that have not been touched for at least 24 hours (in-use temp data is left alone)"
    }

    fn scan(&self) -> Result<Vec<CleanTargetItem>> {
        let temp_dirs = ["/private/tmp", "/private/var/tmp"];
        let mut items = Vec::new();

        for dir_str in &temp_dirs {
            let dir = Path::new(dir_str);
            if dir.is_dir() {
                if let Ok(entries) = fs::read_dir(dir) {
                    for entry in entries.filter_map(|e| e.ok()) {
                        let path = entry.path();

                        // Leave anything a running process may still be using.
                        if !is_older_than(&path, MIN_AGE_BEFORE_CLEANING) {
                            continue;
                        }

                        if let Ok((size, count)) = calculate_size(&path) {
                            if size > 0 {
                                let name = entry.file_name().to_string_lossy().to_string();
                                items.push(CleanTargetItem {
                                    path,
                                    size_bytes: size,
                                    file_count: count,
                                    description: format!("Temp file/folder (idle >24h): {}", name),
                                });
                            }
                        }
                    }
                }
            }
        }

        items.sort_by_key(|b| std::cmp::Reverse(b.size_bytes));
        Ok(items)
    }
}
