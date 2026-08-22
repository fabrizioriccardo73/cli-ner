use crate::utils::format::format_bytes;
use crate::utils::fs::{calculate_size, contract_tilde};
use crate::utils::table::{create_styled_table_with_width, get_terminal_width};
use anyhow::Result;
use comfy_table::{Cell, Color, Row};
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ScannedDirectory {
    pub path: PathBuf,
    pub size_bytes: u64,
    pub file_count: usize,
}

/// Scan a directory and list the size and file count of its top subdirectories / entries.
pub fn scan_directory_entries<P: AsRef<Path>>(
    path: P,
    top_n: usize,
) -> Result<(Vec<ScannedDirectory>, u64)> {
    let path = path.as_ref();
    let mut entries = Vec::new();
    let mut total_size = 0u64;

    if path.is_dir() {
        for entry in fs::read_dir(path)?.flatten() {
            let entry_path = entry.path();
            if let Ok((size, count)) = calculate_size(&entry_path) {
                total_size += size;
                entries.push(ScannedDirectory {
                    path: entry_path,
                    size_bytes: size,
                    file_count: count,
                });
            }
        }
    }

    // Sort by size descending
    entries.sort_by_key(|b| std::cmp::Reverse(b.size_bytes));
    entries.truncate(top_n);

    Ok((entries, total_size))
}

/// Print formatted table of scanned directory entries
pub fn format_scanned_table(
    target_path: &Path,
    entries: &[ScannedDirectory],
    total_size: u64,
) -> String {
    format_scanned_table_with_width(target_path, entries, total_size, get_terminal_width())
}

/// Print formatted table of scanned directory entries with explicit width
pub fn format_scanned_table_with_width(
    target_path: &Path,
    entries: &[ScannedDirectory],
    total_size: u64,
    width: u16,
) -> String {
    let mut table = create_styled_table_with_width(width);
    let is_compact = width < 85;

    if is_compact {
        table.set_header(vec![
            Cell::new("Item Name").fg(Color::Cyan),
            Cell::new("Size").fg(Color::Green),
            Cell::new("Files").fg(Color::Yellow),
            Cell::new("Share").fg(Color::Magenta),
        ]);
    } else {
        table.set_header(vec![
            Cell::new("Item Name").fg(Color::Cyan),
            Cell::new("Size").fg(Color::Green),
            Cell::new("Files").fg(Color::Yellow),
            Cell::new("Share").fg(Color::Magenta),
            Cell::new("Path").fg(Color::DarkGrey),
        ]);
    }

    for entry in entries {
        let name = entry
            .path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_else(|| entry.path.display().to_string());

        let percentage = if total_size > 0 {
            (entry.size_bytes as f64 / total_size as f64) * 100.0
        } else {
            0.0
        };

        if is_compact {
            table.add_row(Row::from(vec![
                Cell::new(name),
                Cell::new(format_bytes(entry.size_bytes)).fg(Color::Green),
                Cell::new(entry.file_count.to_string()),
                Cell::new(format!("{:.1}%", percentage)),
            ]));
        } else {
            table.add_row(Row::from(vec![
                Cell::new(name),
                Cell::new(format_bytes(entry.size_bytes)).fg(Color::Green),
                Cell::new(entry.file_count.to_string()),
                Cell::new(format!("{:.1}%", percentage)),
                Cell::new(contract_tilde(&entry.path)).fg(Color::DarkGrey),
            ]));
        }
    }

    format!(
        "📂 Scanned Target: {}\nTotal Size: {}\n\n{}",
        contract_tilde(target_path),
        format_bytes(total_size),
        table
    )
}

