use crate::cleaner::traits::{CleanTargetItem, Cleaner};
use crate::safety::allowlist::CleanCategory;
use crate::utils::fs::{calculate_size, expand_tilde};
use anyhow::Result;
use std::fs;

pub struct UserCacheCleaner;

impl Cleaner for UserCacheCleaner {
    fn name(&self) -> &'static str {
        "User Application Caches"
    }

    fn category(&self) -> CleanCategory {
        CleanCategory::UserCache
    }

    fn description(&self) -> &'static str {
        "Removes application cache files stored in ~/Library/Caches (excluding protected system items)"
    }

    fn scan(&self) -> Result<Vec<CleanTargetItem>> {
        let cache_dir = expand_tilde("~/Library/Caches");
        let mut items = Vec::new();

        if cache_dir.is_dir() {
            for entry in fs::read_dir(&cache_dir)? {
                if let Ok(entry) = entry {
                    let path = entry.path();
                    if let Ok((size, count)) = calculate_size(&path) {
                        if size > 0 {
                            let name = entry.file_name().to_string_lossy().to_string();
                            items.push(CleanTargetItem {
                                path,
                                size_bytes: size,
                                file_count: count,
                                description: format!("Cache for {}", name),
                            });
                        }
                    }
                }
            }
        }

        items.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
        Ok(items)
    }
}
