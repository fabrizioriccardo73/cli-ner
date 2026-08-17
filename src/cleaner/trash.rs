use crate::cleaner::traits::{CleanTargetItem, Cleaner};
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
            for entry in fs::read_dir(&trash_dir)? {
                if let Ok(entry) = entry {
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
        }

        items.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
        Ok(items)
    }
}
