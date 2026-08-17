use crate::cleaner::traits::{CleanTargetItem, Cleaner};
use crate::safety::allowlist::CleanCategory;
use crate::safety::browser::{get_running_browsers, is_cache_entry_for_running_browser};
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
        "Removes application cache files stored in ~/Library/Caches (excluding protected system items & active running browsers)"
    }

    fn scan(&self) -> Result<Vec<CleanTargetItem>> {
        let cache_dir = expand_tilde("~/Library/Caches");
        let mut items = Vec::new();
        let running_browsers = get_running_browsers();

        if cache_dir.is_dir() {
            for entry in fs::read_dir(&cache_dir)? {
                if let Ok(entry) = entry {
                    let name = entry.file_name().to_string_lossy().to_string();

                    // If a browser is currently running, skip its cache directory to prevent session corruption
                    if is_cache_entry_for_running_browser(&name, &running_browsers).is_some() {
                        continue;
                    }

                    let path = entry.path();
                    if let Ok((size, count)) = calculate_size(&path) {
                        if size > 0 {
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

