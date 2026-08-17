use crate::utils::format::format_bytes;
use anyhow::Result;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, Row, Table};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LargeFileEntry {
    pub path: PathBuf,
    pub size_bytes: u64,
}

/// Recursively find files larger than `min_size_bytes` under `root_path`.
pub fn find_large_files<P: AsRef<Path>>(
    root_path: P,
    min_size_bytes: u64,
    limit: usize,
) -> Result<Vec<LargeFileEntry>> {
    let mut large_files = Vec::new();

    for entry in WalkDir::new(root_path.as_ref())
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if let Ok(meta) = entry.metadata() {
            if meta.is_file() && meta.len() >= min_size_bytes {
                large_files.push(LargeFileEntry {
                    path: entry.path().to_path_buf(),
                    size_bytes: meta.len(),
                });
            }
        }
    }

    // Sort by size descending
    large_files.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    large_files.truncate(limit);

    Ok(large_files)
}

/// Print formatted table of large files found
pub fn format_large_files_table(files: &[LargeFileEntry], min_size: u64) -> String {
    if files.is_empty() {
        return format!(
            "No files found exceeding {} in the specified path.",
            format_bytes(min_size)
        );
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec![
            Cell::new("#").fg(Color::DarkGrey),
            Cell::new("File Name").fg(Color::Cyan),
            Cell::new("Size").fg(Color::Green),
            Cell::new("Path").fg(Color::DarkGrey),
        ]);

    for (i, file) in files.iter().enumerate() {
        let name = file
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| file.path.display().to_string());

        table.add_row(Row::from(vec![
            Cell::new((i + 1).to_string()).fg(Color::DarkGrey),
            Cell::new(name),
            Cell::new(format_bytes(file.size_bytes)).fg(Color::Green),
            Cell::new(file.path.display().to_string()).fg(Color::DarkGrey),
        ]));
    }

    format!(
        "🔍 Large Files (>= {}):\n\n{}",
        format_bytes(min_size),
        table
    )
}
