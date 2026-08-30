use crate::tracker::snapshot::{create_snapshot, DiskSnapshot};
use crate::utils::format::format_bytes;
use crate::utils::fs::expand_tilde;
use crate::utils::table::{create_styled_table_with_width, get_terminal_width};
use anyhow::Result;
use chrono::{DateTime, Local};
use colored::*;
use comfy_table::{Cell, Color, Row};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

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
    #[serde(default)]
    pub old_apfs_snapshots_count: Option<usize>,
    #[serde(default)]
    pub new_apfs_snapshots_count: Option<usize>,
    #[serde(default)]
    pub apfs_snapshots_delta: Option<i64>,
    #[serde(default)]
    pub unaccounted_delta_bytes: Option<i64>,
    pub entries: Vec<DiffEntry>,
}

/// Check if `parent` is a strict directory ancestor of `child`
pub fn is_path_ancestor(parent: &str, child: &str) -> bool {
    if parent == child || parent.is_empty() || child.is_empty() {
        return false;
    }
    if parent == "/" {
        return child.starts_with('/') && child != "/";
    }
    if parent == "~" {
        return child.starts_with("~/");
    }
    let p_bytes = parent.as_bytes();
    let c_bytes = child.as_bytes();
    if c_bytes.starts_with(p_bytes) {
        if parent.ends_with('/') {
            c_bytes.len() > p_bytes.len()
        } else {
            c_bytes.get(p_bytes.len()) == Some(&b'/')
        }
    } else {
        false
    }
}

/// Computes non-overlapping direct deltas for each path by subtracting immediate child deltas
pub fn compute_direct_deltas(path_deltas: &HashMap<String, i64>) -> HashMap<String, i64> {
    let all_paths: Vec<&String> = path_deltas.keys().collect();
    let mut direct_deltas = HashMap::new();

    for &p in &all_paths {
        let p_delta = *path_deltas.get(p).unwrap_or(&0);

        // Find all descendants of p in path_deltas
        let descendants: Vec<&String> = all_paths
            .iter()
            .copied()
            .filter(|&c| is_path_ancestor(p, c))
            .collect();

        // Immediate children are descendants that have no intermediate ancestor between p and c
        let immediate_children: Vec<&String> = descendants
            .iter()
            .copied()
            .filter(|&c| {
                !descendants
                    .iter()
                    .any(|&m| m != c && is_path_ancestor(m, c))
            })
            .collect();

        let children_delta_sum: i64 = immediate_children
            .iter()
            .map(|&c| *path_deltas.get(c).unwrap_or(&0))
            .sum();

        let direct_delta = p_delta - children_delta_sum;
        direct_deltas.insert(p.clone(), direct_delta);
    }

    direct_deltas
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
    let mut path_deltas = HashMap::new();

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

        path_deltas.insert(path.clone(), delta_bytes);

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

    // Calculate non-overlapping hierarchical direct deltas to avoid double/triple counting
    let direct_deltas = compute_direct_deltas(&path_deltas);
    let mut total_growth_bytes = 0u64;
    let mut total_reduction_bytes = 0u64;

    for delta in direct_deltas.values() {
        if *delta > 0 {
            total_growth_bytes += *delta as u64;
        } else if *delta < 0 {
            total_reduction_bytes += delta.unsigned_abs();
        }
    }

    let old_disk_available = old.disk_stats.as_ref().map(|d| d.available_bytes);
    let new_disk_available = new.disk_stats.as_ref().map(|d| d.available_bytes);
    let disk_available_delta = match (old_disk_available, new_disk_available) {
        (Some(old_avail), Some(new_avail)) => Some((new_avail as i64) - (old_avail as i64)),
        _ => None,
    };

    let net_delta_bytes = (total_growth_bytes as i64) - (total_reduction_bytes as i64);

    let old_apfs_snapshots_count = old.apfs_snapshots_count.or_else(|| {
        if old.apfs_snapshots.is_empty() {
            None
        } else {
            Some(old.apfs_snapshots.len())
        }
    });
    let new_apfs_snapshots_count = new.apfs_snapshots_count.or_else(|| {
        if new.apfs_snapshots.is_empty() {
            None
        } else {
            Some(new.apfs_snapshots.len())
        }
    });
    let apfs_snapshots_delta = match (old_apfs_snapshots_count, new_apfs_snapshots_count) {
        (Some(old_cnt), Some(new_cnt)) => Some((new_cnt as i64) - (old_cnt as i64)),
        _ => None,
    };

    let unaccounted_delta_bytes = disk_available_delta.map(|d_avail| (-d_avail) - net_delta_bytes);

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
        old_apfs_snapshots_count,
        new_apfs_snapshots_count,
        apfs_snapshots_delta,
        unaccounted_delta_bytes,
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
    } else {
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
    }

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
        "⚖️  Net Size Change:       {}\n",
        format_delta_bytes(report.net_delta_bytes)
    ));

    // Highlight top space consumer (preferring leaf / specific paths over general root)
    let top_grower = filtered_entries
        .iter()
        .filter(|e| e.delta_bytes > 0 && e.path != "~" && e.path != "/")
        .filter(|e| !filtered_entries.iter().any(|other| is_path_ancestor(&e.path, &other.path) && other.delta_bytes > 0))
        .max_by_key(|e| e.delta_bytes)
        .or_else(|| {
            filtered_entries
                .iter()
                .filter(|e| e.delta_bytes > 0)
                .max_by_key(|e| e.delta_bytes)
        });

    if let Some(grower) = top_grower {
        let pct_str = grower
            .percentage_change
            .map(|p| format!(" (+{:.1}%)", p))
            .unwrap_or_default();

        out.push_str(&format!(
            "🏆 Top Space Consumer (Grower): {} (+{}{})\n",
            grower.path.bold().white(),
            format_bytes(grower.delta_bytes as u64).bold().red(),
            pct_str.bold().red()
        ));
    }

    // 5. Unaccounted Space Diagnostic Section
    if let Some(unaccounted) = report.unaccounted_delta_bytes {
        if unaccounted.abs() >= 200_000_000 {
            out.push_str(&format!(
                "\n⚠️  {}\n",
                format!(
                    "Unaccounted Storage Discrepancy: ~{}",
                    format_bytes(unaccounted.unsigned_abs())
                )
                .yellow()
                .bold()
            ));

            if unaccounted > 0 {
                if report.net_delta_bytes < 0 && report.disk_available_delta.unwrap_or(0) >= 0 {
                    out.push_str(&format!(
                        "   {} (Disk Free Δ {}) vs Tracked Files Reclaimed (Δ {})\n",
                        "Tracked files were deleted, but disk free space did not increase accordingly:".dimmed(),
                        delta_avail_str(report.disk_available_delta.unwrap_or(0)),
                        format_delta_bytes(report.net_delta_bytes),
                    ));
                } else {
                    out.push_str(&format!(
                        "   {} (Disk Free Δ {}) vs Tracked Files (Δ {})\n",
                        "The system consumed more disk space than what was recorded in individual files:".dimmed(),
                        delta_avail_str(report.disk_available_delta.unwrap_or(0)),
                        format_delta_bytes(report.net_delta_bytes),
                    ));
                }
            } else {
                out.push_str(&format!(
                    "   {} (Disk Free Δ {}) vs Tracked Files (Δ {})\n",
                    "More disk space was freed than what was recorded in individual files:".dimmed(),
                    delta_avail_str(report.disk_available_delta.unwrap_or(0)),
                    format_delta_bytes(report.net_delta_bytes),
                ));
            }

            out.push_str("   🔍 Potential sources of unindexed or system-level space:\n");

            if let Some(count) = report.new_apfs_snapshots_count {
                let delta_str = report
                    .apfs_snapshots_delta
                    .map(|d| {
                        if d > 0 {
                            format!(" (Δ +{})", d).yellow().to_string()
                        } else if d < 0 {
                            format!(" (Δ {})", d).green().to_string()
                        } else {
                            "".to_string()
                        }
                    })
                    .unwrap_or_default();

                out.push_str(&format!(
                    "      • 📸 APFS Local Snapshots: {} active snapshot(s){}. macOS Time Machine snapshots hold deleted blocks until purged.\n        Reclaim via: `{}`\n",
                    count.to_string().bold(),
                    delta_str,
                    "tmutil listlocalsnapshots /".cyan()
                ));
            } else {
                out.push_str(&format!(
                    "      • 📸 APFS Local Snapshots: Hidden Time Machine / system snapshots (run `{}`)\n",
                    "tmutil listlocalsnapshots /".cyan()
                ));
            }

            out.push_str(&format!(
                "      • 🐳 Docker & Sandbox Containers: Virtual VM disks in `{}` or `~/.orbstack`\n",
                "~/Library/Containers".cyan()
            ));
            out.push_str(
                "      • ⚙️ System, Xcode & Purgeable Space: Memory swap (`/var/vm`), macOS update caches, simulator runtimes\n",
            );
            out.push_str(&format!(
                "      💡 Tip: Run `{}` to perform an in-depth audit of phantom developer & system bloat.\n",
                "cli-ner bloat".cyan().bold()
            ));
        }
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
    fn test_is_path_ancestor() {
        assert!(is_path_ancestor("~", "~/Downloads"));
        assert!(is_path_ancestor("~/Downloads", "~/Downloads/Obsidian.dmg"));
        assert!(is_path_ancestor("~/.local", "~/.local/share"));
        assert!(!is_path_ancestor("~/.local", "~/.local_backup"));
        assert!(!is_path_ancestor("~/Downloads", "~/Downloads"));
        assert!(is_path_ancestor("/", "/var/log"));
        assert!(is_path_ancestor("/var", "/var/log"));
        assert!(!is_path_ancestor("/var/log", "/var/logs_backup"));
    }

    #[test]
    fn test_hierarchical_no_double_counting() {
        let mut old_items = HashMap::new();
        // Root ~: 100 GB
        old_items.insert("~".to_string(), SnapshotItem {
            path: "~".to_string(),
            size_bytes: 100_000_000_000,
            file_count: 1000,
            is_dir: true,
        });
        // ~/OrbStack: 60 GB
        old_items.insert("~/OrbStack".to_string(), SnapshotItem {
            path: "~/OrbStack".to_string(),
            size_bytes: 60_000_000_000,
            file_count: 500,
            is_dir: true,
        });
        // ~/OrbStack/docker: 60 GB
        old_items.insert("~/OrbStack/docker".to_string(), SnapshotItem {
            path: "~/OrbStack/docker".to_string(),
            size_bytes: 60_000_000_000,
            file_count: 500,
            is_dir: true,
        });
        // ~/Downloads: 10 GB
        old_items.insert("~/Downloads".to_string(), SnapshotItem {
            path: "~/Downloads".to_string(),
            size_bytes: 10_000_000_000,
            file_count: 10,
            is_dir: true,
        });

        let mut new_items = HashMap::new();
        // Root ~: 52 GB (-48 GB net)
        new_items.insert("~".to_string(), SnapshotItem {
            path: "~".to_string(),
            size_bytes: 52_000_000_000,
            file_count: 511,
            is_dir: true,
        });
        // ~/OrbStack: 10 GB (-50 GB)
        new_items.insert("~/OrbStack".to_string(), SnapshotItem {
            path: "~/OrbStack".to_string(),
            size_bytes: 10_000_000_000,
            file_count: 10,
            is_dir: true,
        });
        // ~/OrbStack/docker: 10 GB (-50 GB)
        new_items.insert("~/OrbStack/docker".to_string(), SnapshotItem {
            path: "~/OrbStack/docker".to_string(),
            size_bytes: 10_000_000_000,
            file_count: 10,
            is_dir: true,
        });
        // ~/Downloads: 12 GB (+2 GB)
        new_items.insert("~/Downloads".to_string(), SnapshotItem {
            path: "~/Downloads".to_string(),
            size_bytes: 12_000_000_000,
            file_count: 11,
            is_dir: true,
        });
        // ~/Downloads/app.dmg: 2 GB (✨ NEW +2 GB)
        new_items.insert("~/Downloads/app.dmg".to_string(), SnapshotItem {
            path: "~/Downloads/app.dmg".to_string(),
            size_bytes: 2_000_000_000,
            file_count: 1,
            is_dir: false,
        });

        let old = DiskSnapshot {
            id: "snap1".to_string(),
            name: Some("before".to_string()),
            timestamp: Local::now(),
            root_path: "~".to_string(),
            total_size_bytes: 100_000_000_000,
            total_files_count: 1000,
            disk_stats: None,
            apfs_snapshots_count: None,
            apfs_snapshots: Vec::new(),
            items: old_items,
        };

        let new = DiskSnapshot {
            id: "snap2".to_string(),
            name: Some("after".to_string()),
            timestamp: Local::now(),
            root_path: "~".to_string(),
            total_size_bytes: 52_000_000_000,
            total_files_count: 511,
            disk_stats: None,
            apfs_snapshots_count: None,
            apfs_snapshots: Vec::new(),
            items: new_items,
        };

        let diff = compare_snapshots(&old, &new);
        // Total growth must be exactly 2 GB (Downloads / app.dmg), NOT 4 GB
        assert_eq!(diff.total_growth_bytes, 2_000_000_000);
        // Total reduction must be exactly 50 GB (OrbStack / docker), NOT 150 GB
        assert_eq!(diff.total_reduction_bytes, 50_000_000_000);
        // Net change must be exactly -48 GB
        assert_eq!(diff.net_delta_bytes, -48_000_000_000);

        let table = format_diff_table(&diff, 10, 1_000_000);
        // Top Space Consumer should be ~/Downloads/app.dmg rather than ~/Downloads or ~
        assert!(table.contains("Top Space Consumer (Grower): ~/Downloads/app.dmg"));
    }

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
            apfs_snapshots_count: None,
            apfs_snapshots: Vec::new(),
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
            apfs_snapshots_count: None,
            apfs_snapshots: Vec::new(),
            items: new_items,
        };

        let diff = compare_snapshots(&old, &new);
        assert_eq!(diff.total_growth_bytes, 6_000_000_000);
        assert_eq!(diff.entries.len(), 2);

        let formatted = format_diff_table(&diff, 10, 100_000_000);
        assert!(formatted.contains("~/Library/Caches/Google"));
        assert!(formatted.contains("~/Downloads/large_video.mov"));
    }

    #[test]
    fn test_unaccounted_discrepancy_diagnostic() {
        use crate::tracker::snapshot::DiskStatsSnapshot;

        let old = DiskSnapshot {
            id: "20260822_170801".to_string(),
            name: None,
            timestamp: Local::now(),
            root_path: "~".to_string(),
            total_size_bytes: 3_000_000_000,
            total_files_count: 100,
            disk_stats: Some(DiskStatsSnapshot {
                name: "Macintosh HD".to_string(),
                mount_point: "/".to_string(),
                total_bytes: 500_000_000_000,
                used_bytes: 469_980_000_000,
                available_bytes: 30_020_000_000, // 30.02 GB
                used_percent: 94.0,
            }),
            apfs_snapshots_count: Some(1),
            apfs_snapshots: vec!["com.apple.TimeMachine.2026-08-22-170801.local".to_string()],
            items: HashMap::new(),
        };

        let new = DiskSnapshot {
            id: "20260824_103350".to_string(),
            name: None,
            timestamp: Local::now(),
            root_path: "~".to_string(),
            total_size_bytes: 3_048_000_000,
            total_files_count: 120,
            disk_stats: Some(DiskStatsSnapshot {
                name: "Macintosh HD".to_string(),
                mount_point: "/".to_string(),
                total_bytes: 500_000_000_000,
                used_bytes: 476_690_000_000,
                available_bytes: 23_310_000_000, // 23.31 GB (delta -6.71 GB)
                used_percent: 95.3,
            }),
            apfs_snapshots_count: Some(3), // +2 snapshots
            apfs_snapshots: vec![
                "com.apple.TimeMachine.2026-08-22-170801.local".to_string(),
                "com.apple.TimeMachine.2026-08-23-120000.local".to_string(),
                "com.apple.TimeMachine.2026-08-24-103350.local".to_string(),
            ],
            items: HashMap::new(),
        };

        let diff = compare_snapshots(&old, &new);
        assert_eq!(diff.disk_available_delta, Some(-6_710_000_000));
        assert_eq!(diff.apfs_snapshots_delta, Some(2));
        assert_eq!(diff.unaccounted_delta_bytes, Some(6_710_000_000));

        let formatted = format_diff_table(&diff, 10, 10_000_000);
        assert!(formatted.contains("Unaccounted Storage Discrepancy"));
        assert!(formatted.contains("APFS Local Snapshots"));
        assert!(formatted.contains("tmutil listlocalsnapshots"));
    }

    #[test]
    fn test_unaccounted_discrepancy_deleted_files_phrasing() {
        use crate::tracker::snapshot::DiskStatsSnapshot;

        let old = DiskSnapshot {
            id: "snap1".to_string(),
            name: None,
            timestamp: Local::now(),
            root_path: "~".to_string(),
            total_size_bytes: 200_000_000_000,
            total_files_count: 1000,
            disk_stats: Some(DiskStatsSnapshot {
                name: "Macintosh HD".to_string(),
                mount_point: "/".to_string(),
                total_bytes: 500_000_000_000,
                used_bytes: 494_000_000_000,
                available_bytes: 6_000_000_000, // 6 GB
                used_percent: 98.8,
            }),
            apfs_snapshots_count: None,
            apfs_snapshots: Vec::new(),
            items: HashMap::new(),
        };

        let new_items = HashMap::new();
        // Suppose 100 GB was deleted
        let mut old_items = HashMap::new();
        old_items.insert("~/OrbStack/docker".to_string(), SnapshotItem {
            path: "~/OrbStack/docker".to_string(),
            size_bytes: 100_000_000_000,
            file_count: 100,
            is_dir: true,
        });

        let mut old = old;
        old.items = old_items;

        let new = DiskSnapshot {
            id: "snap2".to_string(),
            name: None,
            timestamp: Local::now(),
            root_path: "~".to_string(),
            total_size_bytes: 100_000_000_000,
            total_files_count: 900,
            disk_stats: Some(DiskStatsSnapshot {
                name: "Macintosh HD".to_string(),
                mount_point: "/".to_string(),
                total_bytes: 500_000_000_000,
                used_bytes: 480_000_000_000,
                available_bytes: 20_000_000_000, // 20 GB (delta +14 GB)
                used_percent: 96.0,
            }),
            apfs_snapshots_count: None,
            apfs_snapshots: Vec::new(),
            items: new_items,
        };

        let diff = compare_snapshots(&old, &new);
        assert_eq!(diff.total_reduction_bytes, 100_000_000_000);
        assert_eq!(diff.net_delta_bytes, -100_000_000_000);
        assert_eq!(diff.disk_available_delta, Some(14_000_000_000));
        // Discrepancy: -14 GB - (-100 GB) = +86 GB
        assert_eq!(diff.unaccounted_delta_bytes, Some(86_000_000_000));

        let formatted = format_diff_table(&diff, 10, 1_000_000);
        assert!(formatted.contains("Tracked files were deleted, but disk free space did not increase accordingly"));
    }
}
