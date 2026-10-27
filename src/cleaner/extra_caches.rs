//! Cleaners for regenerable developer caches that live outside `~/Library/Caches`
//! and were therefore never reachable from any existing category.

use crate::cleaner::traits::{CleanResult, CleanTargetItem, Cleaner, ExecutionItemResult};
use crate::report::operation_log::{ActionStatus, ActionType};
use crate::safety::allowlist::CleanCategory;
use crate::utils::fs::{calculate_size, expand_tilde};
use anyhow::Result;
use std::path::{Path, PathBuf};
use std::process::Command;

/// Is `cmd` on PATH?
fn has_command(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Turn each direct child of `dir` into a clean target, largest first.
///
/// Listing children rather than the directory itself keeps the root in place (the
/// allowlist marks these paths `contents_only`) and lets the user pick individual
/// entries in interactive mode.
pub(crate) fn children_as_targets(
    dir: &Path,
    describe: impl Fn(&str) -> String,
) -> Vec<CleanTargetItem> {
    let mut items = Vec::new();

    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Ok((size, count)) = calculate_size(&path) {
                if size > 0 {
                    let name = entry.file_name().to_string_lossy().to_string();
                    items.push(CleanTargetItem {
                        path,
                        size_bytes: size,
                        file_count: count,
                        description: describe(&name),
                    });
                }
            }
        }
    }

    items.sort_by_key(|i| std::cmp::Reverse(i.size_bytes));
    items
}

// -------------------------------------------------------------------------------------
// pnpm
// -------------------------------------------------------------------------------------

pub struct PnpmStoreCleaner;

impl Cleaner for PnpmStoreCleaner {
    /// Runs an external tool: what it removes never goes through the Trash.
    fn deletes_permanently(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "pnpm Content-Addressable Store"
    }

    fn category(&self) -> CleanCategory {
        CleanCategory::Pnpm
    }

    fn description(&self) -> &'static str {
        "Prunes package versions in the global pnpm store that no project references any more"
    }

    fn is_available(&self) -> bool {
        has_command("pnpm") && pnpm_store_dir().is_some()
    }

    fn scan(&self) -> Result<Vec<CleanTargetItem>> {
        let Some(store) = pnpm_store_dir() else {
            return Ok(Vec::new());
        };
        let (size, count) = calculate_size(&store).unwrap_or((0, 0));
        if size == 0 {
            return Ok(Vec::new());
        }

        Ok(vec![CleanTargetItem {
            path: store,
            size_bytes: size,
            file_count: count,
            // `pnpm store prune` only drops entries nothing links to, so this is an
            // upper bound on what will actually go.
            description: "pnpm global store (upper bound: only unreferenced packages are pruned)"
                .into(),
        }])
    }

    fn clean_items(
        &self,
        targets: &[CleanTargetItem],
        dry_run: bool,
        _force_permanent: bool,
    ) -> Result<CleanResult> {
        let total: u64 = targets.iter().map(|t| t.size_bytes).sum();
        let mut result = CleanResult::default();

        if dry_run {
            result.total_bytes_freed = total;
            result.items_cleaned = targets.len();
            result.details.push(ExecutionItemResult::new(
                "pnpm store prune".into(),
                total,
                ActionType::DryRun,
                ActionStatus::Success,
            ));
            return Ok(result);
        }

        let Some(store) = pnpm_store_dir() else {
            return Ok(result);
        };

        match Command::new("pnpm").args(["store", "prune"]).status() {
            Ok(s) if s.success() => {
                let after = calculate_size(&store).map(|(s, _)| s).unwrap_or(total);
                let freed = total.saturating_sub(after);
                result.total_bytes_freed = freed;
                result.items_cleaned = 1;
                result.details.push(ExecutionItemResult::new(
                    "pnpm store prune".into(),
                    freed,
                    ActionType::ExternalCommand,
                    ActionStatus::Success,
                ));
            }
            Ok(_) => {
                result.items_failed = 1;
                result.details.push(ExecutionItemResult::new(
                    "pnpm store prune".into(),
                    0,
                    ActionType::ExternalCommand,
                    ActionStatus::Failed("pnpm store prune returned a non-zero exit code".into()),
                ));
            }
            Err(e) => {
                result.items_failed = 1;
                result.details.push(ExecutionItemResult::new(
                    "pnpm store prune".into(),
                    0,
                    ActionType::ExternalCommand,
                    ActionStatus::Failed(e.to_string()),
                ));
            }
        }

        Ok(result)
    }
}

/// Locate the pnpm store, preferring pnpm's own answer over the default path.
fn pnpm_store_dir() -> Option<PathBuf> {
    if let Ok(output) = Command::new("pnpm").args(["store", "path"]).output() {
        if output.status.success() {
            let path = PathBuf::from(String::from_utf8_lossy(&output.stdout).trim().to_string());
            if path.is_dir() {
                return Some(path);
            }
        }
    }

    let default = expand_tilde("~/Library/pnpm/store");
    default.is_dir().then_some(default)
}

// -------------------------------------------------------------------------------------
// Go
// -------------------------------------------------------------------------------------

pub struct GoModuleCacheCleaner;

impl Cleaner for GoModuleCacheCleaner {
    /// Runs an external tool: what it removes never goes through the Trash.
    fn deletes_permanently(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "Go Module Cache"
    }

    fn category(&self) -> CleanCategory {
        CleanCategory::Go
    }

    fn description(&self) -> &'static str {
        "Clears the downloaded Go module cache (GOMODCACHE) via `go clean -modcache`"
    }

    fn is_available(&self) -> bool {
        has_command("go")
    }

    fn scan(&self) -> Result<Vec<CleanTargetItem>> {
        if !self.is_available() {
            return Ok(Vec::new());
        }
        let Some(modcache) = go_mod_cache_dir() else {
            return Ok(Vec::new());
        };

        let (size, count) = calculate_size(&modcache).unwrap_or((0, 0));
        if size == 0 {
            return Ok(Vec::new());
        }

        Ok(vec![CleanTargetItem {
            path: modcache,
            size_bytes: size,
            file_count: count,
            description: "Downloaded Go modules (re-fetched on the next build)".into(),
        }])
    }

    fn clean_items(
        &self,
        targets: &[CleanTargetItem],
        dry_run: bool,
        _force_permanent: bool,
    ) -> Result<CleanResult> {
        let total: u64 = targets.iter().map(|t| t.size_bytes).sum();
        let mut result = CleanResult::default();

        if dry_run {
            result.total_bytes_freed = total;
            result.items_cleaned = targets.len();
            result.details.push(ExecutionItemResult::new(
                "go clean -modcache".into(),
                total,
                ActionType::DryRun,
                ActionStatus::Success,
            ));
            return Ok(result);
        }

        // The module cache is deliberately read-only on disk, so it can only be removed
        // through Go's own tooling — a plain unlink or a move to the Trash would leave
        // a half-deleted cache behind.
        match Command::new("go").args(["clean", "-modcache"]).status() {
            Ok(s) if s.success() => {
                let after = go_mod_cache_dir()
                    .and_then(|p| calculate_size(&p).ok())
                    .map(|(s, _)| s)
                    .unwrap_or(0);
                let freed = total.saturating_sub(after);
                result.total_bytes_freed = freed;
                result.items_cleaned = 1;
                result.details.push(ExecutionItemResult::new(
                    "go clean -modcache".into(),
                    freed,
                    ActionType::ExternalCommand,
                    ActionStatus::Success,
                ));
            }
            Ok(_) => {
                result.items_failed = 1;
                result.details.push(ExecutionItemResult::new(
                    "go clean -modcache".into(),
                    0,
                    ActionType::ExternalCommand,
                    ActionStatus::Failed("go clean -modcache returned a non-zero exit code".into()),
                ));
            }
            Err(e) => {
                result.items_failed = 1;
                result.details.push(ExecutionItemResult::new(
                    "go clean -modcache".into(),
                    0,
                    ActionType::ExternalCommand,
                    ActionStatus::Failed(e.to_string()),
                ));
            }
        }

        Ok(result)
    }
}

/// Ask the Go toolchain where its module cache lives, falling back to the default.
fn go_mod_cache_dir() -> Option<PathBuf> {
    if let Ok(output) = Command::new("go").args(["env", "GOMODCACHE"]).output() {
        if output.status.success() {
            let raw = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if !raw.is_empty() {
                let path = PathBuf::from(raw);
                if path.is_dir() {
                    return Some(path);
                }
            }
        }
    }

    let default = expand_tilde("~/go/pkg/mod");
    default.is_dir().then_some(default)
}

// -------------------------------------------------------------------------------------
// iOS Simulator
// -------------------------------------------------------------------------------------

pub struct SimulatorCleaner;

impl Cleaner for SimulatorCleaner {
    fn name(&self) -> &'static str {
        "iOS Simulator Caches"
    }

    fn category(&self) -> CleanCategory {
        CleanCategory::XcodeSimulator
    }

    fn description(&self) -> &'static str {
        "Removes CoreSimulator caches (simulator devices and their app data are never touched here)"
    }

    fn is_available(&self) -> bool {
        expand_tilde("~/Library/Developer/CoreSimulator/Caches").is_dir()
    }

    fn scan(&self) -> Result<Vec<CleanTargetItem>> {
        let caches = expand_tilde("~/Library/Developer/CoreSimulator/Caches");
        if !caches.is_dir() {
            return Ok(Vec::new());
        }

        Ok(children_as_targets(&caches, |name| {
            format!("CoreSimulator cache: {}", name)
        }))
    }
}

// -------------------------------------------------------------------------------------
// iOS Simulator devices whose runtime is not installed — explicit only
// -------------------------------------------------------------------------------------

/// One simulator device that `simctl` currently reports as unavailable.
struct UnavailableDevice {
    udid: String,
    name: String,
    runtime: String,
}

/// Parse `xcrun simctl list devices unavailable -j`. Only entries with a real
/// UUID and `isAvailable == false` are kept: the UDID ends up as an argument to
/// `simctl delete`, so nothing else may get through.
fn parse_unavailable_devices(json: &str) -> Vec<UnavailableDevice> {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(json) else {
        return Vec::new();
    };
    let Some(runtimes) = value.get("devices").and_then(|d| d.as_object()) else {
        return Vec::new();
    };

    let mut devices = Vec::new();
    for (runtime, list) in runtimes {
        for device in list.as_array().into_iter().flatten() {
            let available = device
                .get("isAvailable")
                .and_then(|v| v.as_bool())
                .unwrap_or(true);
            let Some(udid) = device.get("udid").and_then(|v| v.as_str()) else {
                continue;
            };
            if available || uuid::Uuid::parse_str(udid).is_err() {
                continue;
            }
            let name = device
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("Unnamed device");
            let runtime = runtime.rsplit('.').next().unwrap_or(runtime).to_string();
            devices.push(UnavailableDevice {
                udid: udid.to_uppercase(),
                name: name.to_string(),
                runtime,
            });
        }
    }
    devices
}

fn list_unavailable_devices() -> Result<Vec<UnavailableDevice>> {
    let output = Command::new("xcrun")
        .args(["simctl", "list", "devices", "unavailable", "-j"])
        .output()?;
    if !output.status.success() {
        anyhow::bail!("`xcrun simctl list` failed");
    }
    Ok(parse_unavailable_devices(&String::from_utf8_lossy(
        &output.stdout,
    )))
}

/// Deletes simulator devices whose runtime is no longer installed.
///
/// Not part of `all` nor `xcode`: a device only *looks* dead. Reinstalling its
/// runtime (Xcode › Settings › Platforms) brings it back with every app and its
/// data, while `simctl delete` removes all of that for good, without the Trash.
/// So it runs only with `--category unavailable-simulators`, one item per device.
pub struct UnavailableSimulatorsCleaner;

impl Cleaner for UnavailableSimulatorsCleaner {
    fn name(&self) -> &'static str {
        "Unavailable iOS Simulator Devices"
    }

    fn category(&self) -> CleanCategory {
        CleanCategory::XcodeUnavailableSimulators
    }

    fn description(&self) -> &'static str {
        "Deletes simulator devices whose runtime is not installed, with their apps and data \
         (PERMANENT — reinstalling the runtime would have brought them back). Explicit only."
    }

    fn is_available(&self) -> bool {
        has_command("xcrun")
    }

    fn included_in_default_run(&self) -> bool {
        false
    }

    fn deletes_permanently(&self) -> bool {
        true
    }

    fn scan(&self) -> Result<Vec<CleanTargetItem>> {
        if !self.is_available() {
            return Ok(Vec::new());
        }
        let devices_dir = expand_tilde("~/Library/Developer/CoreSimulator/Devices");

        Ok(list_unavailable_devices()?
            .into_iter()
            .map(|d| {
                let path = devices_dir.join(&d.udid);
                let (size, count) = calculate_size(&path).unwrap_or((0, 0));
                CleanTargetItem {
                    path,
                    size_bytes: size,
                    file_count: count,
                    description: format!(
                        "Unavailable simulator: {} ({}) — apps and data deleted permanently",
                        d.name, d.runtime
                    ),
                }
            })
            .collect())
    }

    fn clean_items(
        &self,
        targets: &[CleanTargetItem],
        dry_run: bool,
        _force_permanent: bool,
    ) -> Result<CleanResult> {
        let mut result = CleanResult::default();

        // Re-check right before deleting: if the runtime was reinstalled since the
        // scan, the device is usable again and must be left alone.
        let still_unavailable: Vec<String> = if dry_run {
            Vec::new()
        } else {
            list_unavailable_devices()?
                .into_iter()
                .map(|d| d.udid)
                .collect()
        };

        for item in targets {
            let label = item.path.display().to_string();
            let udid = item
                .path
                .file_name()
                .map(|n| n.to_string_lossy().to_uppercase())
                .filter(|u| uuid::Uuid::parse_str(u).is_ok());

            let Some(udid) = udid else {
                result.items_skipped += 1;
                result.details.push(ExecutionItemResult::new(
                    label,
                    item.size_bytes,
                    ActionType::ExternalCommand,
                    ActionStatus::Skipped("not a simulator device UDID".into()),
                ));
                continue;
            };

            if dry_run {
                result.total_bytes_freed += item.size_bytes;
                result.items_cleaned += 1;
                result.details.push(ExecutionItemResult::new(
                    format!("xcrun simctl delete {}", udid),
                    item.size_bytes,
                    ActionType::DryRun,
                    ActionStatus::Success,
                ));
                continue;
            }

            if !still_unavailable.contains(&udid) {
                result.items_skipped += 1;
                result.details.push(ExecutionItemResult::new(
                    label,
                    item.size_bytes,
                    ActionType::ExternalCommand,
                    ActionStatus::Skipped("device is available again — left untouched".into()),
                ));
                continue;
            }

            match Command::new("xcrun")
                .args(["simctl", "delete", &udid])
                .status()
            {
                Ok(s) if s.success() => {
                    result.total_bytes_freed += item.size_bytes;
                    result.items_cleaned += 1;
                    result.details.push(ExecutionItemResult::new(
                        format!("xcrun simctl delete {} ({})", udid, label),
                        item.size_bytes,
                        ActionType::ExternalCommand,
                        ActionStatus::Success,
                    ));
                }
                Ok(_) => {
                    result.items_failed += 1;
                    result.details.push(ExecutionItemResult::new(
                        label,
                        0,
                        ActionType::ExternalCommand,
                        ActionStatus::Failed("simctl delete returned a non-zero exit code".into()),
                    ));
                }
                Err(e) => {
                    result.items_failed += 1;
                    result.details.push(ExecutionItemResult::new(
                        label,
                        0,
                        ActionType::ExternalCommand,
                        ActionStatus::Failed(e.to_string()),
                    ));
                }
            }
        }

        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simulator_caches_never_delete_devices() {
        assert!(!SimulatorCleaner.deletes_permanently());
        assert!(SimulatorCleaner.included_in_default_run());
    }

    #[test]
    fn test_unavailable_simulators_are_explicit_and_permanent() {
        assert!(!UnavailableSimulatorsCleaner.included_in_default_run());
        assert!(UnavailableSimulatorsCleaner.deletes_permanently());
    }

    #[test]
    fn test_parse_keeps_only_unavailable_devices_with_a_real_udid() {
        let json = r#"{"devices":{
            "com.apple.CoreSimulator.SimRuntime.iOS-16-0":[
                {"udid":"5A8D9C1E-0B7A-4F33-9E0C-1D2B3C4D5E6F","name":"iPhone 14","isAvailable":false},
                {"udid":"0F1E2D3C-4B5A-6978-8796-A5B4C3D2E1F0","name":"iPhone 15","isAvailable":true},
                {"udid":"--help","name":"evil","isAvailable":false},
                {"name":"no udid","isAvailable":false}
            ]}}"#;
        let devices = parse_unavailable_devices(json);
        assert_eq!(devices.len(), 1);
        assert_eq!(devices[0].udid, "5A8D9C1E-0B7A-4F33-9E0C-1D2B3C4D5E6F");
        assert_eq!(devices[0].name, "iPhone 14");
        assert_eq!(devices[0].runtime, "iOS-16-0");
    }

    #[test]
    fn test_parse_tolerates_garbage() {
        assert!(parse_unavailable_devices("not json").is_empty());
        assert!(parse_unavailable_devices(r#"{"devices":{}}"#).is_empty());
    }

    /// A target that is not a device UDID is refused, even in a real run.
    #[test]
    fn test_non_udid_target_is_skipped() {
        let item = CleanTargetItem {
            path: expand_tilde("~/Library/Developer/CoreSimulator/Devices/not-a-udid"),
            size_bytes: 1,
            file_count: 1,
            description: String::new(),
        };
        let result = UnavailableSimulatorsCleaner
            .clean_items(&[item], true, false)
            .unwrap();
        assert_eq!(result.items_skipped, 1);
        assert_eq!(result.items_cleaned, 0);
    }
}
