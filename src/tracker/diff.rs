use crate::tracker::snapshot::{create_snapshot, DiskSnapshot};
use crate::utils::format::format_bytes;
use crate::utils::fs::expand_tilde;
use crate::utils::table::{create_styled_table_with_width, get_terminal_width};
use anyhow::Result;
use chrono::{DateTime, Local};
use colored::*;
use comfy_table::{Cell, Color, Row};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

/// Single entry representing size/file change for a specific path
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffEntry {
    pub path: String,
    pub old_size_bytes: u64,
    pub new_size_bytes: u64,
    pub delta_bytes: i64,
    pub old_files: usize,
    pub new_files: usize,
    pub delta_files: i64,
    pub percentage_change: Option<f64>,
    pub is_new: bool,
    pub is_deleted: bool,
}

/// Comprehensive report comparing two disk snapshots or snapshot vs live state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotDiffReport {
    pub old_id: String,
    pub old_name: Option<String>,
    pub old_timestamp: DateTime<Local>,
    pub new_id: String,
    pub new_name: Option<String>,
    pub new_timestamp: DateTime<Local>,
    pub is_live_comparison: bool,
    pub old_disk_available: Option<u64>,
    pub new_disk_available: Option<u64>,
    pub disk_available_delta: Option<i64>,
    pub total_growth_bytes: u64,
    pub total_reduction_bytes: u64,
    pub net_delta_bytes: i64,
    pub entries: Vec<DiffEntry>,
}

/// Compare two `DiskSnapshot` structures and generate a difference report
pub fn compare_snapshots(old: &DiskSnapshot, new: &DiskSnapshot) -> SnapshotDiffReport {
    let mut all_paths = HashSet::new();
    for p in old.items.keys() {
        all_paths.insert(p.clone());
    }
    for p in new.items.keys() {
        all_paths.insert(p.clone());
    }

    let mut entries = Vec::new();
    let mut total_growth_bytes = 0u64;
    let mut total_reduction_bytes = 0u64;

    for path in all_paths {
        let old_item = old.items.get(&path);
        let new_item = new.items.get(&path);

        let old_size = old_item.map(|i| i.size_bytes).unwrap_or(0);
        let new_size = new_item.map(|i| i.size_bytes).unwrap_or(0);

        let old_files = old_item.map(|i| i.file_count).unwrap_or(0);
        let new_files = new_item.map(|i| i.file_count).unwrap_or(0);

        let delta_bytes = (new_size as i64) - (old_size as i64);
        let delta_files = (new_files as i64) - (old_files as i64);

        if delta_bytes == 0 && delta_files == 0 {
            continue;
        }

        if delta_bytes > 0 {
            total_growth_bytes += delta_bytes as u64;
        } else {
            total_reduction_bytes += delta_bytes.unsigned_abs();
        }

        let is_new = old_item.is_none() && new_item.is_some();
        let is_deleted = old_item.is_some() && new_item.is_none();

        let percentage_change = if old_size > 0 {
            Some(((new_size as f64 - old_size as f64) / old_size as f64) * 100.0)
        } else if new_size > 0 {
            Some(100.0)
        } else {
            None
        };

        entries.push(DiffEntry {
            path,
            old_size_bytes: old_size,
            new_size_bytes: new_size,
            delta_bytes,
            old_files,
            new_files,
            delta_files,
            percentage_change,
            is_new,
            is_deleted,
        });
    }

    // Sort by largest growth first (positive delta descending), then largest reductions
    entries.sort_by(|a, b| b.delta_bytes.cmp(&a.delta_bytes));

    let old_disk_available = old.disk_stats.as_ref().map(|d| d.available_bytes);
    let new_disk_available = new.disk_stats.as_ref().map(|d| d.available_bytes);
    let disk_available_delta = match (old_disk_available, new_disk_available) {
        (Some(old_avail), Some(new_avail)) => Some((new_avail as i64) - (old_avail as i64)),
        _ => None,
    };

    let net_delta_bytes = (total_growth_bytes as i64) - (total_reduction_bytes as i64);

    SnapshotDiffReport {
        old_id: old.id.clone(),
        old_name: old.name.clone(),
        old_timestamp: old.timestamp,
        new_id: new.id.clone(),
        new_name: new.name.clone(),
        new_timestamp: new.timestamp,
        is_live_comparison: false,
        old_disk_available,
        new_disk_available,
        disk_available_delta,
        total_growth_bytes,
        total_reduction_bytes,
        net_delta_bytes,
        entries,
    }
}

/// Compare a saved snapshot against the current live filesystem state
pub fn compare_with_live(old: &DiskSnapshot, max_depth: usize) -> Result<SnapshotDiffReport> {
    let root_path_buf = expand_tilde(&old.root_path);
    let live_snapshot =
        create_snapshot(Some(root_path_buf), Some("live_current".into()), max_depth, false)?;

    let mut report = compare_snapshots(old, &live_snapshot);
    report.is_live_comparison = true;
    report.new_id = "LIVE CURRENT".to_string();
    report.new_name = Some("Live Filesystem State".to_string());

    Ok(report)
}

/// Format the diff report into a clean, color-coded terminal table
pub fn format_diff_table(
    report: &SnapshotDiffReport,
    top_n: usize,
    min_delta_bytes: u64,
) -> String {
    format_diff_table_with_width(report, top_n, min_delta_bytes, get_terminal_width())
}

/// Format the diff report into a clean terminal table with custom width
pub fn format_diff_table_with_width(
    report: &SnapshotDiffReport,
    top_n: usize,
    min_delta_bytes: u64,
    width: u16,
) -> String {
    let mut out = String::new();

    // 1. Comparison Header Banner
    let old_label = report
        .old_name
        .as_deref()
        .map(|n| format!(" ({})", n))
        .unwrap_or_default();
    let new_label = report
        .new_name
        .as_deref()
        .map(|n| format!(" ({})", n))
        .unwrap_or_default();

    out.push_str(&format!(
        "📊 {}\n",
        format!(
            "Disk Differential Comparison: {}{} [{}] vs {}{} [{}]",
            report.old_id.cyan().bold(),
            old_label,
            report.old_timestamp.format("%Y-%m-%d %H:%M").to_string().dimmed(),
            report.new_id.green().bold(),
            new_label,
            report.new_timestamp.format("%Y-%m-%d %H:%M").to_string().dimmed(),
        )
        .bold()
    ));

    // 2. Global Disk Space Delta Banner
    if let (Some(old_avail), Some(new_avail)) = (report.old_disk_available, report.new_disk_available) {
        let delta_avail = (new_avail as i64) - (old_avail as i64);
        let delta_str = delta_avail_str(delta_avail);

        out.push_str(&format!(
            "💾 Free Disk Space: {} → {} (Δ {})\n",
            format_bytes(old_avail).white(),
            format_bytes(new_avail).bold(),
            delta_str
        ));
    }

    // 3. Filter entries by minimum delta threshold
    let filtered_entries: Vec<&DiffEntry> = report
        .entries
        .iter()
        .filter(|e| e.delta_bytes.unsigned_abs() >= min_delta_bytes)
        .take(top_n)
        .collect();

    if filtered_entries.is_empty() {
        out.push_str(&format!(
            "\n{}\n",
            format!(
                "✨ No significant size variations (>= {}) detected between snapshots.",
                format_bytes(min_delta_bytes)
            )
            .green()
        ));
        return out;
    }

    let mut table = create_styled_table_with_width(width);
    let is_compact = width < 100;

    if is_compact {
        table.set_header(vec![
            Cell::new("Path / Target").fg(Color::Cyan),
            Cell::new("Previous").fg(Color::DarkGrey),
            Cell::new("Current").fg(Color::White),
            Cell::new("Growth (Δ)").fg(Color::Yellow),
            Cell::new("Status").fg(Color::Magenta),
        ]);

        for e in &filtered_entries {
            let status_str = if e.is_new {
                "NEW".green().to_string()
            } else if e.is_deleted {
                "DELETED".red().to_string()
            } else if let Some(pct) = e.percentage_change {
                if pct >= 0.0 {
                    format!("+{:.1}%", pct).red().to_string()
                } else {
                    format!("{:.1}%", pct).green().to_string()
                }
            } else {
                "-".to_string()
            };

            table.add_row(Row::from(vec![
                Cell::new(&e.path),
                Cell::new(format_bytes(e.old_size_bytes)),
                Cell::new(format_bytes(e.new_size_bytes)),
                Cell::new(format_delta_bytes(e.delta_bytes)),
                Cell::new(status_str),
            ]));
        }
    } else {
        table.set_header(vec![
            Cell::new("Path / Target Directory").fg(Color::Cyan),
            Cell::new("Previous Size").fg(Color::DarkGrey),
            Cell::new("Current Size").fg(Color::White),
            Cell::new("Growth (Δ Bytes)").fg(Color::Yellow),
            Cell::new("Files (Δ)").fg(Color::Blue),
            Cell::new("Change (%)").fg(Color::Magenta),
        ]);

        for e in &filtered_entries {
            let change_str = if e.is_new {
                "✨ NEW".green().bold().to_string()
            } else if e.is_deleted {
                "🗑️ DELETED".red().to_string()
            } else if let Some(pct) = e.percentage_change {
                if pct >= 0.0 {
                    format!("+{:.1}%", pct).red().bold().to_string()
                } else {
                    format!("{:.1}%", pct).green().bold().to_string()
                }
            } else {
                "-".to_string()
            };

            let files_delta_str = if e.delta_files > 0 {
                format!("+{}", e.delta_files).yellow().to_string()
            } else if e.delta_files < 0 {
                format!("{}", e.delta_files).green().to_string()
            } else {
                "0".to_string()
            };

            table.add_row(Row::from(vec![
                Cell::new(&e.path),
                Cell::new(format_bytes(e.old_size_bytes)).fg(Color::DarkGrey),
                Cell::new(format_bytes(e.new_size_bytes)).fg(Color::White),
                Cell::new(format_delta_bytes(e.delta_bytes)),
                Cell::new(files_delta_str),
                Cell::new(change_str),
            ]));
        }
    }

    out.push_str(&format!("\n{}\n", table));

    // 4. Growth Summary and Top Offenders
    out.push_str(&format!(
        "\n📈 Total Space Growth:    {}\n",
        format!("+{}", format_bytes(report.total_growth_bytes)).red().bold()
    ));
    out.push_str(&format!(
        "📉 Total Space Reclaimed: {}\n",
        format!("-{}", format_bytes(report.total_reduction_bytes)).green().bold()
    ));
    out.push_str(&format!(
        "⚖️  Net Size Change:       {}\n\n",
        format_delta_bytes(report.net_delta_bytes)
    ));

    // Highlight top space consumer
    if let Some(top_grower) = filtered_entries.iter().find(|e| e.delta_bytes > 0) {
        let pct_str = top_grower
            .percentage_change
            .map(|p| format!(" (+{:.1}%)", p))
            .unwrap_or_default();

        out.push_str(&format!(
            "🏆 Top Space Consumer (Grower): {} (+{}{})\n",
            top_grower.path.bold().white(),
            format_bytes(top_grower.delta_bytes as u64).bold().red(),
            pct_str.bold().red()
        ));
    }

    out
}

fn delta_avail_str(delta: i64) -> String {
    if delta < 0 {
        format!("-{}", format_bytes(delta.unsigned_abs())).red().bold().to_string()
    } else {
        format!("+{}", format_bytes(delta as u64)).green().bold().to_string()
    }
}

fn format_delta_bytes(delta: i64) -> String {
    if delta > 0 {
        format!("+{}", format_bytes(delta as u64)).red().bold().to_string()
    } else if delta < 0 {
        format!("-{}", format_bytes(delta.unsigned_abs())).green().bold().to_string()
    } else {
        "0 B".dimmed().to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tracker::snapshot::SnapshotItem;
    use std::collections::HashMap;

    #[test]
    fn test_compare_snapshots_growth() {
        let mut old_items = HashMap::new();
        old_items.insert(
            "~/Library/Caches/Google".to_string(),
            SnapshotItem {
                path: "~/Library/Caches/Google".to_string(),
                size_bytes: 1_000_000_000,
                file_count: 100,
                is_dir: true,
            },
        );

        let mut new_items = HashMap::new();
        new_items.insert(
            "~/Library/Caches/Google".to_string(),
            SnapshotItem {
                path: "~/Library/Caches/Google".to_string(),
                size_bytes: 5_000_000_000,
                file_count: 500,
                is_dir: true,
            },
        );
        new_items.insert(
            "~/Downloads/large_video.mov".to_string(),
            SnapshotItem {
                path: "~/Downloads/large_video.mov".to_string(),
                size_bytes: 2_000_000_000,
                file_count: 1,
                is_dir: false,
            },
        );

        let old = DiskSnapshot {
            id: "snap1".to_string(),
            name: Some("before".to_string()),
            timestamp: Local::now(),
            root_path: "~".to_string(),
            total_size_bytes: 1_000_000_000,
            total_files_count: 100,
            disk_stats: None,
            items: old_items,
        };

        let new = DiskSnapshot {
            id: "snap2".to_string(),
            name: Some("after".to_string()),
            timestamp: Local::now(),
            root_path: "~".to_string(),
            total_size_bytes: 7_000_000_000,
            total_files_count: 501,
            disk_stats: None,
            items: new_items,
        };

        let diff = compare_snapshots(&old, &new);
        assert_eq!(diff.total_growth_bytes, 6_000_000_000);
        assert_eq!(diff.entries.len(), 2);

        let formatted = format_diff_table(&diff, 10, 100_000_000);
        assert!(formatted.contains("~/Library/Caches/Google"));
        assert!(formatted.contains("~/Downloads/large_video.mov"));
    }
}
