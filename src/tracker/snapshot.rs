use crate::utils::format::format_bytes;
use crate::utils::fs::{calculate_size, contract_tilde, expand_tilde};
use crate::utils::platform::get_disk_stats;
use crate::utils::table::{create_styled_table_with_width, get_terminal_width};
use anyhow::{bail, Context, Result};
use chrono::{DateTime, Local};
use colored::*;
use comfy_table::{Cell, Color, Row};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};

/// Single item entry recorded in a disk snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotItem {
    pub path: String,
    pub size_bytes: u64,
    pub file_count: usize,
    pub is_dir: bool,
}

/// Disk usage statistics captured at snapshot time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskStatsSnapshot {
    pub name: String,
    pub mount_point: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub used_percent: f64,
}

/// A complete disk snapshot representing filesystem state at a point in time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskSnapshot {
    pub id: String,
    pub name: Option<String>,
    pub timestamp: DateTime<Local>,
    pub root_path: String,
    pub total_size_bytes: u64,
    pub total_files_count: usize,
    pub disk_stats: Option<DiskStatsSnapshot>,
    pub items: HashMap<String, SnapshotItem>,
}

/// Returns the directory where snapshots are stored (`~/.cli-ner/snapshots`)
pub fn get_snapshots_dir() -> PathBuf {
    expand_tilde("~/.cli-ner/snapshots")
}

/// Scans a directory tree up to `max_depth` and records all path sizes
pub fn scan_path_tree<P: AsRef<Path>>(
    root: P,
    current_depth: usize,
    max_depth: usize,
    items: &mut HashMap<String, SnapshotItem>,
) -> (u64, usize) {
    let path = root.as_ref();
    if !path.exists() {
        return (0, 0);
    }

    let contracted = contract_tilde(path);

    if path.is_file() || path.is_symlink() {
        if let Ok(meta) = fs::symlink_metadata(path) {
            let size = if meta.is_file() { meta.len() } else { 0 };
            items.insert(
                contracted.clone(),
                SnapshotItem {
                    path: contracted,
                    size_bytes: size,
                    file_count: 1,
                    is_dir: false,
                },
            );
            return (size, 1);
        }
        return (0, 0);
    }

    // If we've reached the maximum depth, calculate size of this directory directly
    if current_depth >= max_depth {
        let (size, count) = calculate_shallow_size(path);
        items.insert(
            contracted.clone(),
            SnapshotItem {
                path: contracted,
                size_bytes: size,
                file_count: count,
                is_dir: true,
            },
        );
        return (size, count);
    }

    let mut total_dir_size = 0u64;
    let mut total_dir_files = 0usize;

    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            let entry_path = entry.path();
            let entry_name = entry.file_name().to_string_lossy().to_string();

            // Skip git internals, sandboxes, and noisy lock directories
            if entry_name == ".git"
                || entry_name == ".Trash"
                || entry_name == "Containers"
                || entry_name == "Group Containers"
            {
                continue;
            }

            let (sub_size, sub_files) =
                scan_path_tree(&entry_path, current_depth + 1, max_depth, items);
            total_dir_size += sub_size;
            total_dir_files += sub_files;
        }
    }

    items.insert(
        contracted.clone(),
        SnapshotItem {
            path: contracted,
            size_bytes: total_dir_size,
            file_count: total_dir_files,
            is_dir: true,
        },
    );

    (total_dir_size, total_dir_files)
}

/// Calculate shallow size of a directory without deep traversal
fn calculate_shallow_size<P: AsRef<Path>>(path: P) -> (u64, usize) {
    let mut total_size = 0u64;
    let mut total_files = 0usize;

    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    total_size += meta.len();
                    total_files += 1;
                } else if meta.is_dir() {
                    total_files += 1;
                }
            }
        }
    }

    (total_size, total_files)
}

/// Create a new snapshot of the specified root path (or user home) and optionally save to disk
pub fn create_snapshot(
    root: Option<PathBuf>,
    name: Option<String>,
    max_depth: usize,
    persist: bool,
) -> Result<DiskSnapshot> {
    let root_path = root
        .map(expand_tilde)
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")));

    let now = Local::now();
    let timestamp_str = now.format("%Y%m%d_%H%M%S").to_string();
    let id = if let Some(ref custom_name) = name {
        let sanitized: String = custom_name
            .chars()
            .map(|c| if c.is_alphanumeric() || c == '-' || c == '_' { c } else { '_' })
            .collect();
        format!("{}_{}", timestamp_str, sanitized)
    } else {
        timestamp_str
    };

    let mut items = HashMap::new();
    let (total_size_bytes, total_files_count) =
        scan_path_tree(&root_path, 0, max_depth, &mut items);

    // Also capture critical system locations for comprehensive macOS tracking
    let system_critical_paths = [
        "/var/vm/sleepimage",
        "/var/vm",
        "/Library/Logs",
        "/var/log",
        "/Library/Updates",
    ];

    for sys_path in system_critical_paths {
        let p = Path::new(sys_path);
        if p.exists() {
            if let Ok((size, count)) = calculate_size(p) {
                if size > 0 {
                    items.insert(
                        sys_path.to_string(),
                        SnapshotItem {
                            path: sys_path.to_string(),
                            size_bytes: size,
                            file_count: count,
                            is_dir: p.is_dir(),
                        },
                    );
                }
            }
        }
    }

    // Capture root disk statistics
    let disk_stats = get_disk_stats();
    let root_disk = disk_stats
        .into_iter()
        .find(|d| d.mount_point == "/" || d.mount_point == "/System/Volumes/Data")
        .map(|d| {
            let used_pct = if d.total_space > 0 {
                (d.used_space as f64 / d.total_space as f64) * 100.0
            } else {
                0.0
            };
            DiskStatsSnapshot {
                name: d.name,
                mount_point: d.mount_point,
                total_bytes: d.total_space,
                used_bytes: d.used_space,
                available_bytes: d.available_space,
                used_percent: used_pct,
            }
        });

    let snapshot = DiskSnapshot {
        id,
        name,
        timestamp: now,
        root_path: contract_tilde(&root_path),
        total_size_bytes,
        total_files_count,
        disk_stats: root_disk,
        items,
    };

    // Save snapshot to disk if requested
    if persist {
        save_snapshot(&snapshot)?;
    }

    Ok(snapshot)
}

/// Save a snapshot object to `~/.cli-ner/snapshots/<id>.json`
pub fn save_snapshot(snapshot: &DiskSnapshot) -> Result<PathBuf> {
    let dir = get_snapshots_dir();
    fs::create_dir_all(&dir)
        .with_context(|| format!("Failed to create snapshot directory at {}", dir.display()))?;

    let file_path = dir.join(format!("{}.json", snapshot.id));
    let file = File::create(&file_path)
        .with_context(|| format!("Failed to create snapshot file at {}", file_path.display()))?;

    let writer = BufWriter::new(file);
    serde_json::to_writer(writer, snapshot)
        .with_context(|| format!("Failed to serialize snapshot {}", snapshot.id))?;

    Ok(file_path)
}

/// List all saved snapshots sorted from newest to oldest
pub fn list_snapshots() -> Result<Vec<DiskSnapshot>> {
    let dir = get_snapshots_dir();
    if !dir.exists() {
        return Ok(Vec::new());
    }

    let mut snapshots = Vec::new();
    for entry in fs::read_dir(&dir)?.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            if let Ok(file) = File::open(&path) {
                let reader = BufReader::new(file);
                if let Ok(snap) = serde_json::from_reader::<_, DiskSnapshot>(reader) {
                    snapshots.push(snap);
                }
            }
        }
    }

    snapshots.sort_by_key(|s| std::cmp::Reverse(s.timestamp));
    Ok(snapshots)
}

/// Load a specific snapshot by ID or label
pub fn load_snapshot(id_or_name: &str) -> Result<DiskSnapshot> {
    let snapshots = list_snapshots()?;
    if let Some(found) = snapshots.iter().find(|s| {
        s.id == id_or_name
            || s.id.starts_with(id_or_name)
            || s.name.as_deref() == Some(id_or_name)
    }) {
        return Ok(found.clone());
    }

    bail!(
        "Snapshot '{}' not found. Run `cli-ner snapshot list` to view available snapshots.",
        id_or_name
    );
}

/// Get the most recent saved snapshot
pub fn get_latest_snapshot() -> Result<Option<DiskSnapshot>> {
    let snapshots = list_snapshots()?;
    Ok(snapshots.into_iter().next())
}

/// Delete a specific snapshot by ID or name
pub fn delete_snapshot(id_or_name: &str) -> Result<bool> {
    let dir = get_snapshots_dir();
    if !dir.exists() {
        return Ok(false);
    }

    let snapshots = list_snapshots()?;
    if let Some(target) = snapshots.iter().find(|s| {
        s.id == id_or_name
            || s.id.starts_with(id_or_name)
            || s.name.as_deref() == Some(id_or_name)
    }) {
        let file_path = dir.join(format!("{}.json", target.id));
        if file_path.exists() {
            fs::remove_file(file_path)?;
            return Ok(true);
        }
    }

    Ok(false)
}

/// Delete all saved snapshots
pub fn delete_all_snapshots() -> Result<usize> {
    let dir = get_snapshots_dir();
    if !dir.exists() {
        return Ok(0);
    }

    let mut count = 0;
    for entry in fs::read_dir(&dir)?.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) == Some("json") {
            fs::remove_file(path)?;
            count += 1;
        }
    }

    Ok(count)
}

/// Format the snapshot list into a styled terminal table
pub fn format_snapshots_table(snapshots: &[DiskSnapshot]) -> String {
    format_snapshots_table_with_width(snapshots, get_terminal_width())
}

/// Format the snapshot list into a styled terminal table with custom width
pub fn format_snapshots_table_with_width(snapshots: &[DiskSnapshot], width: u16) -> String {
    if snapshots.is_empty() {
        return "No snapshots found in ~/.cli-ner/snapshots/.\nRun `cli-ner snapshot create` to take your first disk snapshot."
            .yellow()
            .to_string();
    }

    let mut table = create_styled_table_with_width(width);
    table.set_header(vec![
        Cell::new("Snapshot ID").fg(Color::Cyan),
        Cell::new("Timestamp").fg(Color::White),
        Cell::new("Label / Name").fg(Color::Yellow),
        Cell::new("Target Path").fg(Color::DarkGrey),
        Cell::new("Items / Files").fg(Color::Magenta),
        Cell::new("Free Disk Space").fg(Color::Green),
    ]);

    for snap in snapshots {
        let name_str = snap.name.as_deref().unwrap_or("-");
        let free_space = snap
            .disk_stats
            .as_ref()
            .map(|d| format_bytes(d.available_bytes))
            .unwrap_or_else(|| "-".to_string());

        table.add_row(Row::from(vec![
            Cell::new(&snap.id),
            Cell::new(snap.timestamp.format("%Y-%m-%d %H:%M:%S").to_string()),
            Cell::new(name_str),
            Cell::new(&snap.root_path),
            Cell::new(format!("{} items", snap.items.len())),
            Cell::new(free_space).fg(Color::Green),
        ]));
    }

    format!("📸 Saved Disk Snapshots:\n\n{}", table)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_scan_path_tree() {
        let temp_dir = std::env::temp_dir().join("cli_ner_test_snap_tree");
        let _ = fs::create_dir_all(temp_dir.join("sub1/sub2"));
        let test_file = temp_dir.join("sub1/test.txt");
        fs::write(&test_file, "hello world snapshot").unwrap();

        let mut items = HashMap::new();
        let (size, count) = scan_path_tree(&temp_dir, 0, 3, &mut items);

        assert!(size >= 20);
        assert!(count >= 1);
        assert!(!items.is_empty());

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_snapshot_serialization_and_format() {
        let snap = DiskSnapshot {
            id: "20260822_120000_test".to_string(),
            name: Some("test-label".to_string()),
            timestamp: Local::now(),
            root_path: "~".to_string(),
            total_size_bytes: 10_000_000_000,
            total_files_count: 500,
            disk_stats: Some(DiskStatsSnapshot {
                name: "Macintosh HD".to_string(),
                mount_point: "/".to_string(),
                total_bytes: 500_000_000_000,
                used_bytes: 400_000_000_000,
                available_bytes: 100_000_000_000,
                used_percent: 80.0,
            }),
            items: HashMap::new(),
        };

        let formatted = format_snapshots_table_with_width(&[snap], 120);
        assert!(formatted.contains("20260822_120000_test"));
        assert!(formatted.contains("test-label"));
    }
}
