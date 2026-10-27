use crate::utils::format::format_bytes;
use crate::utils::fs::{calculate_size, contract_tilde, expand_tilde, get_allocated_size};
use crate::utils::platform::get_disk_stats;
use crate::utils::table::{create_styled_table_with_width, get_terminal_width};
use anyhow::Result;
use colored::*;
use comfy_table::{Cell, Color, Row};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Categories of phantom disk bloat on macOS
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum BloatCategory {
    TimeMachineSnapshots,
    SleepImageAndSwap,
    XcodeAndSimulators,
    DockerContainers,
    UserCaches,
    BrowserProfiles,
    SystemAndUserLogs,
    CrashReports,
    MessagesAndMail,
    CloudStorage,
    SoftwareUpdate,
    SpotlightIndex,
    Trash,
    DevPackageCaches,
}

impl BloatCategory {
    pub fn display_name(&self) -> &'static str {
        match self {
            Self::TimeMachineSnapshots => "🕐 Time Machine Snapshots",
            Self::SleepImageAndSwap => "💤 Sleep Image & Swap",
            Self::XcodeAndSimulators => "🔨 Xcode & Simulators",
            Self::DockerContainers => "🐳 Docker & Containers",
            Self::UserCaches => "🗄️ User App Caches",
            Self::BrowserProfiles => "🌐 Browser Data & Profiles",
            Self::SystemAndUserLogs => "📋 System & User Logs",
            Self::CrashReports => "💥 Crash & Diagnostics",
            Self::MessagesAndMail => "✉️ Mail & Message Downloads",
            Self::CloudStorage => "☁️ iCloud & Cloud Cache",
            Self::SoftwareUpdate => "🔄 Software Update Cache",
            Self::SpotlightIndex => "🔍 Spotlight Index",
            Self::Trash => "🗑️ Trash Bin",
            Self::DevPackageCaches => "📦 Dev Package Caches",
        }
    }
}

/// Severity classification of a bloat source based on size
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, PartialOrd, Ord)]
pub enum BloatSeverity {
    Low,      // < 1 GB
    Medium,   // 1 GB .. 5 GB
    High,     // 5 GB .. 20 GB
    Critical, // >= 20 GB
}

impl BloatSeverity {
    pub fn from_bytes(bytes: u64) -> Self {
        if bytes >= 20_000_000_000 {
            Self::Critical
        } else if bytes >= 5_000_000_000 {
            Self::High
        } else if bytes >= 1_000_000_000 {
            Self::Medium
        } else {
            Self::Low
        }
    }

    pub fn badge(&self) -> String {
        match self {
            Self::Critical => "CRITICAL".red().bold().to_string(),
            Self::High => "HIGH".magenta().bold().to_string(),
            Self::Medium => "MEDIUM".yellow().to_string(),
            Self::Low => "LOW".cyan().to_string(),
        }
    }
}

/// A detected bloat source with diagnostics and recovery hints
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BloatSource {
    pub category: BloatCategory,
    pub label: String,
    pub size_bytes: u64,
    pub path_or_command: String,
    pub severity: BloatSeverity,
    pub reclaimable: bool,
    pub reclaim_hint: String,
    pub requires_sudo: bool,
    /// True when `size_bytes` could not be measured directly and is not a fact.
    /// Such rows are shown to the user but kept out of every headline total.
    #[serde(default)]
    pub size_is_estimate: bool,
    pub details: Option<String>,
}

/// Summary of the main disk state
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiskSummary {
    pub name: String,
    pub mount_point: String,
    pub total_bytes: u64,
    pub used_bytes: u64,
    pub available_bytes: u64,
    pub used_percent: f64,
}

/// Complete report returned by the bloat analyzer
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BloatReport {
    pub disk: Option<DiskSummary>,
    pub sources: Vec<BloatSource>,
    pub total_bloat_bytes: u64,
    pub scan_timestamp: chrono::DateTime<chrono::Local>,
}

// -------------------------------------------------------------------------------------------------
// Scanner Functions
// -------------------------------------------------------------------------------------------------

/// 1. Scan for APFS Time Machine local snapshots via `tmutil`
pub fn scan_time_machine_snapshots() -> Option<BloatSource> {
    let output = Command::new("tmutil")
        .arg("listlocalsnapshotdates")
        .output()
        .or_else(|_| {
            Command::new("tmutil")
                .arg("listlocalsnapshots")
                .arg("/")
                .output()
        })
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let snapshot_lines: Vec<&str> = stdout
        .lines()
        .map(|l| l.trim())
        .filter(|l| {
            !l.is_empty()
                && !l.starts_with("Snapshot dates")
                && !l.starts_with("Snapshots for")
                && (l.contains("com.apple.TimeMachine")
                    || l.chars()
                        .all(|c| c.is_ascii_digit() || c == '-' || c == ':'))
        })
        .collect();

    let count = snapshot_lines.len();
    if count == 0 {
        return None;
    }

    // APFS local snapshots pin blocks that deleted files would otherwise release, and
    // macOS exposes no supported way to ask how many bytes any given snapshot holds.
    // Rather than invent a per-snapshot figure and fold it into the totals, the count is
    // reported with the size explicitly marked as unknown.
    Some(BloatSource {
        category: BloatCategory::TimeMachineSnapshots,
        label: format!("{} APFS local snapshot(s)", count),
        size_bytes: 0,
        path_or_command: "tmutil listlocalsnapshotdates".to_string(),
        severity: BloatSeverity::Low,
        reclaimable: true,
        reclaim_hint: "sudo tmutil thinlocalsnapshots / 9999999999 4 (or deletelocalsnapshots <date>)".to_string(),
        requires_sudo: true,
        size_is_estimate: true,
        details: Some(format!(
            "{} snapshot(s) active: {}. Each one holds on to blocks freed since it was taken; \
             macOS reports no per-snapshot size, so this space shows up as \"purgeable\" in Finder.",
            count,
            snapshot_lines.join(", ")
        )),
    })
}

/// 2. Scan sleep image and swap files in `/var/vm`
pub fn scan_sleep_image_and_swap() -> Vec<BloatSource> {
    let mut sources = Vec::new();
    let vm_dir = Path::new("/var/vm");

    if !vm_dir.exists() {
        return sources;
    }

    // Check sleepimage
    let sleepimage_path = vm_dir.join("sleepimage");
    if let Ok(meta) = fs::symlink_metadata(&sleepimage_path) {
        let size = get_allocated_size(&meta);
        if size > 0 {
            sources.push(BloatSource {
                category: BloatCategory::SleepImageAndSwap,
                label: "macOS Hibernation Sleep Image".to_string(),
                size_bytes: size,
                path_or_command: sleepimage_path.display().to_string(),
                severity: BloatSeverity::from_bytes(size),
                reclaimable: true,
                reclaim_hint:
                    "sudo pmset -a hibernatemode 0 && sudo rm /var/vm/sleepimage (for desktop Macs)"
                        .to_string(),
                requires_sudo: true,
                size_is_estimate: false,
                details: Some(
                    "Pre-allocated RAM dump file created for Mac safe sleep / hibernation."
                        .to_string(),
                ),
            });
        }
    }

    // Check swap files
    if let Ok(entries) = fs::read_dir(vm_dir) {
        let mut total_swap = 0u64;
        let mut swap_count = 0usize;
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with("swapfile") {
                if let Ok(meta) = entry.metadata() {
                    total_swap += get_allocated_size(&meta);
                    swap_count += 1;
                }
            }
        }

        if total_swap > 0 {
            sources.push(BloatSource {
                category: BloatCategory::SleepImageAndSwap,
                label: format!("Virtual Memory Swap ({} file(s))", swap_count),
                size_bytes: total_swap,
                path_or_command: "/var/vm/swapfile*".to_string(),
                severity: BloatSeverity::from_bytes(total_swap),
                reclaimable: false,
                reclaim_hint:
                    "Restarting macOS will automatically flush and reclaim dynamic swap space."
                        .to_string(),
                requires_sudo: false,
                size_is_estimate: false,
                details: Some(
                    "Dynamic swap memory created by macOS when physical RAM is under pressure."
                        .to_string(),
                ),
            });
        }
    }

    sources
}

/// 3. Scan Xcode, Simulators, DerivedData, and iOS DeviceSupport
pub fn scan_xcode_and_simulators() -> Vec<BloatSource> {
    let mut sources = Vec::new();

    let targets = [
        (
            "Xcode DerivedData (Build cache & indexes)",
            "~/Library/Developer/Xcode/DerivedData",
            "cli-ner clean --category xcode --execute",
            true,
        ),
        (
            "Xcode iOS DeviceSupport (Old iOS symbol files)",
            "~/Library/Developer/Xcode/iOS DeviceSupport",
            "cli-ner clean --category xcode --execute",
            true,
        ),
        (
            "Xcode Archives (Built IPA/App archives)",
            "~/Library/Developer/Xcode/Archives",
            "Review ~/Library/Developer/Xcode/Archives and delete old builds",
            true,
        ),
        (
            "iOS Simulator Devices (Runtime disk images)",
            "~/Library/Developer/CoreSimulator/Devices",
            "xcrun simctl erase all (or delete unused devices via Xcode)",
            true,
        ),
        (
            "Xcode App Caches",
            "~/Library/Caches/com.apple.dt.Xcode",
            "cli-ner clean --category xcode --execute",
            true,
        ),
    ];

    for (label, path_str, hint, reclaimable) in targets {
        let expanded = expand_tilde(path_str);
        if expanded.exists() {
            if let Ok((size, count)) = calculate_size(&expanded) {
                if size > 50_000_000 {
                    // >= 50 MB
                    sources.push(BloatSource {
                        category: BloatCategory::XcodeAndSimulators,
                        label: label.to_string(),
                        size_bytes: size,
                        path_or_command: contract_tilde(&expanded),
                        severity: BloatSeverity::from_bytes(size),
                        reclaimable,
                        reclaim_hint: hint.to_string(),
                        requires_sudo: false,
                        size_is_estimate: false,
                        details: Some(format!("Contains {} item(s)", count)),
                    });
                }
            }
        }
    }

    sources
}

/// 4. Scan Docker & Container VM disks and storage
pub fn scan_docker_and_containers() -> Vec<BloatSource> {
    let mut sources = Vec::new();

    let targets = [
        (
            "Docker Desktop VM Disk (`Docker.raw`)",
            "~/Library/Containers/com.docker.docker/Data/vms/0/data/Docker.raw",
            "cli-ner docker wizard (or docker system prune -a --volumes)",
        ),
        (
            "Docker Desktop VM Disk Legacy (`Docker.raw`)",
            "~/Library/Containers/com.docker.docker/Data/vms/0/Docker.raw",
            "cli-ner docker wizard",
        ),
        (
            "OrbStack Linux/Container Storage",
            "~/.orbstack/data",
            "orbctl prune (or orbctl system reset)",
        ),
        (
            "Colima VM Disk & Data",
            "~/.colima",
            "colima prune (or colima delete)",
        ),
    ];

    for (label, path_str, hint) in targets {
        let expanded = expand_tilde(path_str);
        if expanded.exists() {
            if let Ok((size, _)) = calculate_size(&expanded) {
                if size > 100_000_000 {
                    // >= 100 MB
                    sources.push(BloatSource {
                        category: BloatCategory::DockerContainers,
                        label: label.to_string(),
                        size_bytes: size,
                        path_or_command: contract_tilde(&expanded),
                        severity: BloatSeverity::from_bytes(size),
                        reclaimable: true,
                        reclaim_hint: hint.to_string(),
                        requires_sudo: false,
                        size_is_estimate: false,
                        details: Some(
                            "Virtual disk image storing containers, images, and BuildKit layers."
                                .to_string(),
                        ),
                    });
                }
            }
        }
    }

    sources
}

/// 5. Scan User Application Caches (`~/Library/Caches`)
pub fn scan_user_caches() -> Vec<BloatSource> {
    let mut sources = Vec::new();
    let caches_dir = expand_tilde("~/Library/Caches");

    if !caches_dir.exists() {
        return sources;
    }

    let mut total_cache_size = 0u64;
    let mut top_offenders: Vec<(String, u64)> = Vec::new();

    if let Ok(entries) = fs::read_dir(&caches_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Ok((size, _)) = calculate_size(&path) {
                total_cache_size += size;
                if size > 200_000_000 {
                    // >= 200 MB
                    let name = path
                        .file_name()
                        .map(|n| n.to_string_lossy().to_string())
                        .unwrap_or_else(|| path.display().to_string());
                    top_offenders.push((name, size));
                }
            }
        }
    }

    top_offenders.sort_by_key(|(_, size)| std::cmp::Reverse(*size));

    if total_cache_size > 500_000_000 {
        let top_str = top_offenders
            .iter()
            .take(4)
            .map(|(name, sz)| format!("{}: {}", name, format_bytes(*sz)))
            .collect::<Vec<_>>()
            .join(", ");

        sources.push(BloatSource {
            category: BloatCategory::UserCaches,
            label: "User Application Caches (`~/Library/Caches`)".to_string(),
            size_bytes: total_cache_size,
            path_or_command: "~/Library/Caches".to_string(),
            severity: BloatSeverity::from_bytes(total_cache_size),
            reclaimable: true,
            reclaim_hint: "cli-ner clean --category user-cache --execute".to_string(),
            requires_sudo: false,
            size_is_estimate: false,
            details: if !top_str.is_empty() {
                Some(format!("Top consumers: {}", top_str))
            } else {
                None
            },
        });
    }

    sources
}

/// 6. Scan Browser Profiles and Persistent Application Support data
pub fn scan_browser_profiles() -> Vec<BloatSource> {
    let mut sources = Vec::new();

    let browsers = [
        (
            "Google Chrome Data",
            "~/Library/Application Support/Google/Chrome",
        ),
        ("Arc Browser Data", "~/Library/Application Support/Arc"),
        (
            "Brave Browser Data",
            "~/Library/Application Support/BraveSoftware/Brave-Browser",
        ),
        (
            "Microsoft Edge Data",
            "~/Library/Application Support/Microsoft Edge",
        ),
        (
            "Mozilla Firefox Profiles",
            "~/Library/Application Support/Firefox/Profiles",
        ),
        ("Apple Safari Data", "~/Library/Safari"),
    ];

    for (label, path_str) in browsers {
        let expanded = expand_tilde(path_str);
        if expanded.exists() {
            if let Ok((size, _)) = calculate_size(&expanded) {
                if size > 300_000_000 {
                    // >= 300 MB
                    sources.push(BloatSource {
                        category: BloatCategory::BrowserProfiles,
                        label: label.to_string(),
                        size_bytes: size,
                        path_or_command: contract_tilde(&expanded),
                        severity: BloatSeverity::from_bytes(size),
                        reclaimable: true,
                        reclaim_hint: "Clear browser browsing history, cookies, and extension storage in browser settings.".to_string(),
                        requires_sudo: false,
                        size_is_estimate: false,
                        details: Some("IndexedDB, local storage, extensions, service workers, and GPU cache.".to_string()),
                    });
                }
            }
        }
    }

    sources
}

/// 7. Scan System and User Logs
pub fn scan_system_and_user_logs(include_system: bool) -> Vec<BloatSource> {
    let mut sources = Vec::new();

    let log_dirs = [
        (
            "User Logs (`~/Library/Logs`)",
            "~/Library/Logs",
            "cli-ner clean --category user-logs --execute",
            false,
        ),
        (
            "System Logs (`/Library/Logs`)",
            "/Library/Logs",
            "sudo rm -rf /Library/Logs/*",
            true,
        ),
        (
            "Unix / ASL Logs (`/var/log`)",
            "/var/log",
            "sudo log erase --all (or sudo rm -rf /var/log/asl/*)",
            true,
        ),
    ];

    for (label, path_str, hint, sudo) in log_dirs {
        if sudo && !include_system {
            continue;
        }
        let expanded = expand_tilde(path_str);
        if expanded.exists() {
            if let Ok((size, count)) = calculate_size(&expanded) {
                if size > 100_000_000 {
                    // >= 100 MB
                    sources.push(BloatSource {
                        category: BloatCategory::SystemAndUserLogs,
                        label: label.to_string(),
                        size_bytes: size,
                        path_or_command: contract_tilde(&expanded),
                        severity: BloatSeverity::from_bytes(size),
                        reclaimable: true,
                        reclaim_hint: hint.to_string(),
                        requires_sudo: sudo,
                        size_is_estimate: false,
                        details: Some(format!("Contains {} log files/folders", count)),
                    });
                }
            }
        }
    }

    sources
}

/// 8. Scan Crash and Diagnostic Reports
pub fn scan_crash_reports(include_system: bool) -> Vec<BloatSource> {
    let mut sources = Vec::new();

    let dirs = [
        (
            "User Diagnostic Reports",
            "~/Library/Logs/DiagnosticReports",
        ),
        (
            "System Diagnostic Reports",
            "/Library/Logs/DiagnosticReports",
        ),
    ];

    for (label, path_str) in dirs {
        if path_str.starts_with("/Library") && !include_system {
            continue;
        }
        let expanded = expand_tilde(path_str);
        if expanded.exists() {
            if let Ok((size, count)) = calculate_size(&expanded) {
                if size > 50_000_000 {
                    // >= 50 MB
                    sources.push(BloatSource {
                        category: BloatCategory::CrashReports,
                        label: label.to_string(),
                        size_bytes: size,
                        path_or_command: contract_tilde(&expanded),
                        severity: BloatSeverity::from_bytes(size),
                        reclaimable: true,
                        reclaim_hint: "Safe to delete old crash/spin dump reports (.ips, .crash)."
                            .to_string(),
                        requires_sudo: path_str.starts_with("/Library"),
                        size_is_estimate: false,
                        details: Some(format!("{} report file(s)", count)),
                    });
                }
            }
        }
    }

    sources
}

/// 9. Scan Mail Downloads and Message Attachments
pub fn scan_messages_and_mail() -> Vec<BloatSource> {
    let mut sources = Vec::new();

    let targets = [
        (
            "Apple Mail Attachments & Downloads",
            "~/Library/Containers/com.apple.mail/Data/Library/Mail Downloads",
        ),
        ("Apple Mail Storage", "~/Library/Mail"),
        (
            "iMessage / SMS Attachments",
            "~/Library/Messages/Attachments",
        ),
    ];

    for (label, path_str) in targets {
        let expanded = expand_tilde(path_str);
        if expanded.exists() {
            if let Ok((size, count)) = calculate_size(&expanded) {
                if size > 200_000_000 {
                    // >= 200 MB
                    sources.push(BloatSource {
                        category: BloatCategory::MessagesAndMail,
                        label: label.to_string(),
                        size_bytes: size,
                        path_or_command: contract_tilde(&expanded),
                        severity: BloatSeverity::from_bytes(size),
                        reclaimable: true,
                        reclaim_hint: "Delete unnecessary large downloaded media/attachments in Mail or Messages settings.".to_string(),
                        requires_sudo: false,
                        size_is_estimate: false,
                        details: Some(format!("Contains {} cached file(s)", count)),
                    });
                }
            }
        }
    }

    sources
}

/// 10. Scan iCloud & Cloud Storage Cache
pub fn scan_cloud_storage() -> Vec<BloatSource> {
    let mut sources = Vec::new();

    let targets = [
        (
            "iCloud Mobile Documents Cache",
            "~/Library/Mobile Documents",
        ),
        (
            "macOS CloudStorage Provider Cache",
            "~/Library/CloudStorage",
        ),
    ];

    for (label, path_str) in targets {
        let expanded = expand_tilde(path_str);
        if expanded.exists() {
            if let Ok((size, count)) = calculate_size(&expanded) {
                if size > 500_000_000 {
                    // >= 500 MB
                    sources.push(BloatSource {
                        category: BloatCategory::CloudStorage,
                        label: label.to_string(),
                        size_bytes: size,
                        path_or_command: contract_tilde(&expanded),
                        severity: BloatSeverity::from_bytes(size),
                        reclaimable: true,
                        reclaim_hint:
                            "Use 'Remove Download' in Finder to evict local copies of cloud files."
                                .to_string(),
                        requires_sudo: false,
                        size_is_estimate: false,
                        details: Some(format!("{} cloud file(s) cached locally on disk", count)),
                    });
                }
            }
        }
    }

    sources
}

/// 11. Scan Software Update & Installer Sandboxes
pub fn scan_software_update(include_system: bool) -> Vec<BloatSource> {
    let mut sources = Vec::new();

    let targets = [
        ("macOS Software Update Cache", "/Library/Updates", true),
        ("Installer Sandboxes", "/Library/InstallerSandboxes", true),
        (
            "User Software Update Cache",
            "~/Library/Caches/com.apple.SoftwareUpdate",
            false,
        ),
    ];

    for (label, path_str, sudo) in targets {
        if sudo && !include_system {
            continue;
        }
        let expanded = expand_tilde(path_str);
        if expanded.exists() {
            if let Ok((size, _)) = calculate_size(&expanded) {
                if size > 100_000_000 {
                    // >= 100 MB
                    sources.push(BloatSource {
                        category: BloatCategory::SoftwareUpdate,
                        label: label.to_string(),
                        size_bytes: size,
                        path_or_command: contract_tilde(&expanded),
                        severity: BloatSeverity::from_bytes(size),
                        reclaimable: true,
                        reclaim_hint: "macOS staged installer files. Usually cleaned automatically after installation or safe to delete if orphaned.".to_string(),
                        requires_sudo: sudo,
                        size_is_estimate: false,
                        details: Some("Staged macOS system update downloads.".to_string()),
                    });
                }
            }
        }
    }

    sources
}

/// 12. Scan Spotlight Index size
pub fn scan_spotlight_index() -> Vec<BloatSource> {
    let mut sources = Vec::new();

    let targets = [
        ("Spotlight Index (`/.Spotlight-V100`)", "/.Spotlight-V100"),
        (
            "Spotlight Index Data (`/System/Volumes/Data/.Spotlight-V100`)",
            "/System/Volumes/Data/.Spotlight-V100",
        ),
    ];

    for (label, path_str) in targets {
        let path = Path::new(path_str);
        if path.exists() {
            if let Ok((size, _)) = calculate_size(path) {
                if size > 500_000_000 {
                    // >= 500 MB
                    sources.push(BloatSource {
                        category: BloatCategory::SpotlightIndex,
                        label: label.to_string(),
                        size_bytes: size,
                        path_or_command: path_str.to_string(),
                        severity: BloatSeverity::from_bytes(size),
                        reclaimable: true,
                        reclaim_hint: "sudo mdutil -E / (rebuilds and flushes corrupted/bloated Spotlight index)".to_string(),
                        requires_sudo: true,
                        size_is_estimate: false,
                        details: Some("Metadata and full-text index database for macOS Spotlight search.".to_string()),
                    });
                }
            }
        }
    }

    sources
}

/// 13. Scan Trash Bin (`~/.Trash`)
pub fn scan_trash() -> Option<BloatSource> {
    let trash_path = expand_tilde("~/.Trash");
    if trash_path.exists() {
        if let Ok((size, count)) = calculate_size(&trash_path) {
            if size > 100_000_000 {
                // >= 100 MB
                return Some(BloatSource {
                    category: BloatCategory::Trash,
                    label: "macOS Trash Bin (`~/.Trash`)".to_string(),
                    size_bytes: size,
                    path_or_command: "~/.Trash".to_string(),
                    severity: BloatSeverity::from_bytes(size),
                    reclaimable: true,
                    reclaim_hint:
                        "cli-ner clean --category trash --execute (or empty Trash in Finder)"
                            .to_string(),
                    requires_sudo: false,
                    size_is_estimate: false,
                    details: Some(format!(
                        "Contains {} item(s) pending permanent deletion",
                        count
                    )),
                });
            }
        }
    }
    None
}

/// 14. Scan Developer Package Manager Global Caches
pub fn scan_dev_package_caches() -> Vec<BloatSource> {
    let mut sources = Vec::new();

    let targets = [
        (
            "Homebrew Downloads Cache",
            "~/Library/Caches/Homebrew",
            "cli-ner clean --category brew --execute",
        ),
        (
            "CocoaPods Pod Cache",
            "~/Library/Caches/CocoaPods",
            "pod cache clean --all",
        ),
        (
            "npm Global Cache",
            "~/.npm/_cacache",
            "cli-ner clean --category npm --execute",
        ),
        (
            "Yarn Global Cache",
            "~/Library/Caches/Yarn",
            "yarn cache clean",
        ),
        (
            "pnpm Global Store",
            "~/Library/pnpm/store",
            "pnpm store prune",
        ),
        (
            "pip / Python Wheel Cache",
            "~/Library/Caches/pip",
            "cli-ner clean --category pip --execute",
        ),
        (
            "Gradle Dependency Cache",
            "~/.gradle/caches",
            "cli-ner clean --category gradle --execute (or --category dev)",
        ),
        (
            "Maven Local Repository",
            "~/.m2/repository",
            "cli-ner clean --category maven --execute (or --category dev)",
        ),
        (
            "Rust Cargo Registry Cache",
            "~/.cargo/registry/cache",
            "cli-ner clean --category cargo --execute",
        ),
    ];

    for (label, path_str, hint) in targets {
        let expanded = expand_tilde(path_str);
        if expanded.exists() {
            if let Ok((size, count)) = calculate_size(&expanded) {
                if size > 150_000_000 {
                    // >= 150 MB
                    sources.push(BloatSource {
                        category: BloatCategory::DevPackageCaches,
                        label: label.to_string(),
                        size_bytes: size,
                        path_or_command: contract_tilde(&expanded),
                        severity: BloatSeverity::from_bytes(size),
                        reclaimable: true,
                        reclaim_hint: hint.to_string(),
                        requires_sudo: false,
                        size_is_estimate: false,
                        details: Some(format!("Contains {} cached packages/artifacts", count)),
                    });
                }
            }
        }
    }

    sources
}

// -------------------------------------------------------------------------------------------------
// Orchestration & Report Formatting
// -------------------------------------------------------------------------------------------------

/// Sum sizes while counting each region of the filesystem exactly once.
///
/// Several scanners deliberately overlap — `~/Library/Caches` is reported as a whole and
/// again broken down per package manager, `~/Library/Logs` overlaps its DiagnosticReports
/// subfolder — so adding every row would report gigabytes that exist only once as if they
/// existed twice. Ancestors win: the broadest row is counted and its descendants absorbed.
/// Rows whose size is only an estimate never contribute.
pub fn non_overlapping_total(sources: &[&BloatSource]) -> u64 {
    let mut order: Vec<&&BloatSource> = sources.iter().collect();
    // Shortest paths first, so a parent is always counted before its children are examined.
    order.sort_by_key(|s| {
        source_fs_path(s)
            .map(|p| p.components().count())
            .unwrap_or(usize::MAX)
    });

    let mut counted: Vec<PathBuf> = Vec::new();
    let mut total = 0u64;

    for source in order {
        if source.size_is_estimate {
            continue;
        }
        match source_fs_path(source) {
            Some(path) => {
                if counted.iter().any(|seen| path.starts_with(seen)) {
                    continue;
                }
                counted.push(path.clone());
                total += source.size_bytes;
            }
            // Not a filesystem location (a docker/tmutil query): nothing can overlap it.
            None => total += source.size_bytes,
        }
    }

    total
}

/// The filesystem location a source refers to, when it refers to one at all.
fn source_fs_path(source: &BloatSource) -> Option<PathBuf> {
    let raw = source.path_or_command.trim();
    if raw.starts_with('/') || raw.starts_with('~') {
        Some(expand_tilde(raw))
    } else {
        None
    }
}

/// Run complete phantom disk bloat analysis across macOS subsystems.
///
/// `include_system` opts into the scans that reach outside the user's home directory
/// (`/Library`, `/var/log`, `/var/vm`, the Spotlight index). Those are slow, frequently
/// unreadable without elevated privileges, and reclaiming them needs `sudo` anyway.
pub fn run_full_bloat_analysis(include_system: bool) -> Result<BloatReport> {
    let mut sources = Vec::new();

    // 1. Time Machine Local Snapshots
    if let Some(tm) = scan_time_machine_snapshots() {
        sources.push(tm);
    }

    // 2. Sleep image & Swap (lives under /var/vm)
    if include_system {
        sources.extend(scan_sleep_image_and_swap());
    }

    // 3. Xcode & Simulators
    sources.extend(scan_xcode_and_simulators());

    // 4. Docker & Containers
    sources.extend(scan_docker_and_containers());

    // 5. User App Caches
    sources.extend(scan_user_caches());

    // 6. Browser Profiles
    sources.extend(scan_browser_profiles());

    // 7. System & User Logs
    sources.extend(scan_system_and_user_logs(include_system));

    // 8. Crash Reports
    sources.extend(scan_crash_reports(include_system));

    // 9. Messages & Mail
    sources.extend(scan_messages_and_mail());

    // 10. Cloud Storage
    sources.extend(scan_cloud_storage());

    // 11. Software Update
    sources.extend(scan_software_update(include_system));

    // 12. Spotlight Index (root-level, requires elevated privileges to read fully)
    if include_system {
        sources.extend(scan_spotlight_index());
    }

    // 13. Trash
    if let Some(trash) = scan_trash() {
        sources.push(trash);
    }

    // 14. Dev Package Caches
    sources.extend(scan_dev_package_caches());

    // Sort by size descending
    sources.sort_by_key(|s| std::cmp::Reverse(s.size_bytes));

    let total_bloat_bytes = non_overlapping_total(&sources.iter().collect::<Vec<_>>());

    // Get root disk info
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
            DiskSummary {
                name: d.name,
                mount_point: d.mount_point,
                total_bytes: d.total_space,
                used_bytes: d.used_space,
                available_bytes: d.available_space,
                used_percent: used_pct,
            }
        });

    Ok(BloatReport {
        disk: root_disk,
        sources,
        total_bloat_bytes,
        scan_timestamp: chrono::Local::now(),
    })
}

/// Format the bloat report into a rich terminal table
pub fn format_bloat_table(report: &BloatReport, min_size_bytes: u64, detailed: bool) -> String {
    format_bloat_table_with_width(report, min_size_bytes, detailed, get_terminal_width())
}

/// Format the bloat report into a rich terminal table with explicit width
pub fn format_bloat_table_with_width(
    report: &BloatReport,
    min_size_bytes: u64,
    detailed: bool,
    width: u16,
) -> String {
    let mut out = String::new();

    // Disk summary header
    if let Some(disk) = &report.disk {
        out.push_str(&format!(
            "💾 {}\n",
            format!(
                "Disk Status: {} used / {} total ({:.1}% used) — {} free",
                format_bytes(disk.used_bytes).bold().yellow(),
                format_bytes(disk.total_bytes).bold().white(),
                disk.used_percent,
                format_bytes(disk.available_bytes).bold().green()
            )
            .bold()
        ));
    }

    // Rows whose size could not be measured are always shown: filtering them by a byte
    // threshold would silently hide them behind a number they do not actually have.
    let filtered_sources: Vec<&BloatSource> = report
        .sources
        .iter()
        .filter(|s| s.size_is_estimate || s.size_bytes >= min_size_bytes)
        .collect();

    if filtered_sources.is_empty() {
        out.push_str(&format!(
            "\n{}\n",
            "✨ No significant phantom bloat sources found exceeding threshold!".green()
        ));
        return out;
    }

    let size_cell = |s: &BloatSource| {
        if s.size_is_estimate {
            Cell::new("not measurable").fg(Color::DarkGrey)
        } else {
            Cell::new(format_bytes(s.size_bytes)).fg(Color::Green)
        }
    };

    let mut table = create_styled_table_with_width(width);
    let is_compact = width < 100;

    if is_compact {
        table.set_header(vec![
            Cell::new("Category").fg(Color::Cyan),
            Cell::new("Source").fg(Color::White),
            Cell::new("Size").fg(Color::Green),
            Cell::new("Severity").fg(Color::Magenta),
            Cell::new("Recovery Hint").fg(Color::Yellow),
        ]);

        for s in &filtered_sources {
            table.add_row(Row::from(vec![
                Cell::new(s.category.display_name()),
                Cell::new(&s.label),
                size_cell(s),
                Cell::new(s.severity.badge()),
                Cell::new(&s.reclaim_hint).fg(Color::Yellow),
            ]));
        }
    } else {
        table.set_header(vec![
            Cell::new("Category").fg(Color::Cyan),
            Cell::new("Source Description").fg(Color::White),
            Cell::new("Size").fg(Color::Green),
            Cell::new("Severity").fg(Color::Magenta),
            Cell::new("Reclaimable").fg(Color::Blue),
            Cell::new("Recovery Action / Hint").fg(Color::Yellow),
        ]);

        for s in &filtered_sources {
            let reclaimable_str = if !s.reclaimable {
                "⚠️ Reboot".yellow().to_string()
            } else if s.requires_sudo {
                "✅ Yes (sudo)".green().to_string()
            } else {
                "✅ Yes".green().to_string()
            };

            let hint_str = if detailed {
                if let Some(details) = &s.details {
                    format!(
                        "{}\n💡 {}\n📂 {}",
                        s.reclaim_hint, details, s.path_or_command
                    )
                } else {
                    format!("{}\n📂 {}", s.reclaim_hint, s.path_or_command)
                }
            } else {
                s.reclaim_hint.clone()
            };

            table.add_row(Row::from(vec![
                Cell::new(s.category.display_name()),
                Cell::new(&s.label),
                size_cell(s),
                Cell::new(s.severity.badge()),
                Cell::new(reclaimable_str),
                Cell::new(hint_str).fg(Color::Yellow),
            ]));
        }
    }

    out.push_str(&format!("\n{}\n", table));

    // Summary section
    let total_filtered = non_overlapping_total(&filtered_sources);
    out.push_str(&format!(
        "\n📊 {}\n",
        format!(
            "Total Identified Phantom Bloat: {}",
            format_bytes(total_filtered).bold().green()
        )
        .bold()
    ));
    out.push_str(&format!(
        "   {}\n",
        "Overlapping locations (e.g. ~/Library/Caches and the package-manager caches inside it) are counted once."
            .dimmed()
    ));

    let unmeasured: Vec<&&BloatSource> = filtered_sources
        .iter()
        .filter(|s| s.size_is_estimate)
        .collect();
    if !unmeasured.is_empty() {
        out.push_str(&format!(
            "   {}\n",
            format!(
                "{} row(s) have no measurable size and are excluded from the total above.",
                unmeasured.len()
            )
            .dimmed()
        ));
    }

    if let Some(top) = filtered_sources.iter().find(|s| !s.size_is_estimate) {
        out.push_str(&format!(
            "🏆 Top Opportunity: {} ({})\n   ↳ Action: {}\n",
            top.label.bold().white(),
            format_bytes(top.size_bytes).bold().green(),
            top.reclaim_hint.bold().cyan()
        ));
    }

    if !detailed {
        out.push_str(&format!(
            "\n💡 Tip: Run `{}` for exact filesystem paths and extended diagnostics.\n",
            "cli-ner bloat --detailed".cyan()
        ));
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bloat_severity_classification() {
        assert_eq!(BloatSeverity::from_bytes(500_000_000), BloatSeverity::Low);
        assert_eq!(
            BloatSeverity::from_bytes(2_000_000_000),
            BloatSeverity::Medium
        );
        assert_eq!(
            BloatSeverity::from_bytes(10_000_000_000),
            BloatSeverity::High
        );
        assert_eq!(
            BloatSeverity::from_bytes(25_000_000_000),
            BloatSeverity::Critical
        );
    }

    #[test]
    fn test_format_bloat_table() {
        let report = BloatReport {
            disk: Some(DiskSummary {
                name: "Macintosh HD".to_string(),
                mount_point: "/".to_string(),
                total_bytes: 500_000_000_000,
                used_bytes: 250_000_000_000,
                available_bytes: 250_000_000_000,
                used_percent: 50.0,
            }),
            sources: vec![
                BloatSource {
                    category: BloatCategory::XcodeAndSimulators,
                    label: "Xcode DerivedData".to_string(),
                    size_bytes: 15_000_000_000,
                    path_or_command: "~/Library/Developer/Xcode/DerivedData".to_string(),
                    severity: BloatSeverity::High,
                    reclaimable: true,
                    reclaim_hint: "cli-ner clean --category xcode --execute".to_string(),
                    requires_sudo: false,
                    size_is_estimate: false,
                    details: Some("Cached build indexes".to_string()),
                },
                BloatSource {
                    category: BloatCategory::DockerContainers,
                    label: "Docker VM disk".to_string(),
                    size_bytes: 25_000_000_000,
                    path_or_command: "Docker.raw".to_string(),
                    severity: BloatSeverity::Critical,
                    reclaimable: true,
                    reclaim_hint: "cli-ner docker wizard".to_string(),
                    requires_sudo: false,
                    size_is_estimate: false,
                    details: None,
                },
            ],
            total_bloat_bytes: 40_000_000_000,
            scan_timestamp: chrono::Local::now(),
        };

        let formatted = format_bloat_table(&report, 100_000_000, false);
        assert!(formatted.contains("Docker VM disk"));
        assert!(formatted.contains("Xcode DerivedData"));
        assert!(formatted.contains("40.00 GB") || formatted.contains("40 GB"));
    }

    fn source_at(path: &str, size: u64, estimate: bool) -> BloatSource {
        BloatSource {
            category: BloatCategory::UserCaches,
            label: path.to_string(),
            size_bytes: size,
            path_or_command: path.to_string(),
            severity: BloatSeverity::from_bytes(size),
            reclaimable: true,
            reclaim_hint: String::new(),
            requires_sudo: false,
            size_is_estimate: estimate,
            details: None,
        }
    }

    /// `~/Library/Caches` is reported whole *and* broken down per package manager.
    /// The headline total must count those gigabytes once, not once per row.
    #[test]
    fn test_total_counts_nested_locations_once() {
        let sources = [
            source_at("~/Library/Caches", 5_000_000_000, false),
            source_at("~/Library/Caches/Homebrew", 2_000_000_000, false),
            source_at("~/Library/Caches/pip", 1_000_000_000, false),
            source_at("~/.gradle/caches", 3_000_000_000, false),
        ];

        let total = non_overlapping_total(&sources.iter().collect::<Vec<_>>());
        assert_eq!(
            total, 8_000_000_000,
            "the two caches nested under ~/Library/Caches must be absorbed by it"
        );
    }

    /// A sibling that merely shares a name prefix is a different directory entirely.
    #[test]
    fn test_total_does_not_absorb_name_prefix_siblings() {
        let sources = [
            source_at("~/Library/Caches", 1_000_000_000, false),
            source_at("~/Library/CachesOld", 500_000_000, false),
        ];

        let total = non_overlapping_total(&sources.iter().collect::<Vec<_>>());
        assert_eq!(total, 1_500_000_000);
    }

    /// Rows whose size could not be measured must never inflate a total.
    #[test]
    fn test_total_excludes_unmeasurable_rows() {
        let mut tm = source_at("tmutil listlocalsnapshotdates", 0, true);
        tm.category = BloatCategory::TimeMachineSnapshots;

        let sources = [source_at("~/.gradle/caches", 3_000_000_000, false), tm];
        let total = non_overlapping_total(&sources.iter().collect::<Vec<_>>());
        assert_eq!(total, 3_000_000_000);
    }

    /// Non-path rows (docker/tmutil queries) cannot overlap anything and always count.
    #[test]
    fn test_total_keeps_non_path_sources() {
        let sources = [
            source_at("~/Library/Caches", 1_000_000_000, false),
            source_at("docker:build-cache", 2_000_000_000, false),
        ];
        let total = non_overlapping_total(&sources.iter().collect::<Vec<_>>());
        assert_eq!(total, 3_000_000_000);
    }
}
