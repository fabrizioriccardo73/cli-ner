use crate::cleaner::traits::{CleanTargetItem, Cleaner};
use crate::safety::allowlist::CleanCategory;
use crate::utils::fs::calculate_size;
use anyhow::Result;
use std::fs;
use std::path::Path;

pub struct TempFilesCleaner;

impl Cleaner for TempFilesCleaner {
    fn name(&self) -> &'static str {
        "Temporary System & App Files"
    }

    fn category(&self) -> CleanCategory {
        CleanCategory::TempFiles
    }

    fn description(&self) -> &'static str {
        "Cleans temporary files and folders in /private/tmp and /private/var/tmp"
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
                        if let Ok((size, count)) = calculate_size(&path) {
                            if size > 0 {
                                let name = entry.file_name().to_string_lossy().to_string();
                                items.push(CleanTargetItem {
                                    path,
                                    size_bytes: size,
                                    file_count: count,
                                    description: format!("Temp file/folder: {}", name),
                                });
                            }
                        }
                    }
                }
            }
        }

        items.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
        Ok(items)
    }
}
