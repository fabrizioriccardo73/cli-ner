//! Cleaners for regenerable developer caches that live outside `~/Library/Caches`
//! and were therefore never reachable from any existing category.

use crate::cleaner::traits::{
    trash_or_delete_targets, CleanResult, CleanTargetItem, Cleaner, ExecutionItemResult,
};
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
fn children_as_targets(dir: &Path, describe: impl Fn(&str) -> String) -> Vec<CleanTargetItem> {
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
        "iOS Simulator Caches & Unavailable Devices"
    }

    fn category(&self) -> CleanCategory {
        CleanCategory::XcodeSimulator
    }

    fn description(&self) -> &'static str {
        "Removes CoreSimulator caches and deletes simulator devices whose runtime is no longer installed (installed simulators and their app data are left untouched)"
    }

    fn is_available(&self) -> bool {
        has_command("xcrun") || expand_tilde("~/Library/Developer/CoreSimulator").is_dir()
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

    fn clean_items(
        &self,
        targets: &[CleanTargetItem],
        dry_run: bool,
        force_permanent: bool,
    ) -> Result<CleanResult> {
        // Caches go through the standard validated trash/delete path.
        let mut result = trash_or_delete_targets(targets, dry_run, force_permanent)?;

        // Then drop devices bound to runtimes that are no longer installed. These are
        // unusable by definition — Xcode itself offers the same action — so nothing
        // reachable is lost. Devices for *installed* runtimes are never touched.
        if !has_command("xcrun") {
            return Ok(result);
        }

        if dry_run {
            result.details.push(ExecutionItemResult::new(
                "xcrun simctl delete unavailable".into(),
                0,
                ActionType::DryRun,
                ActionStatus::Success,
            ));
            return Ok(result);
        }

        let devices_dir = expand_tilde("~/Library/Developer/CoreSimulator/Devices");
        let before = calculate_size(&devices_dir).map(|(s, _)| s).unwrap_or(0);

        match Command::new("xcrun")
            .args(["simctl", "delete", "unavailable"])
            .status()
        {
            Ok(s) if s.success() => {
                let after = calculate_size(&devices_dir)
                    .map(|(s, _)| s)
                    .unwrap_or(before);
                let freed = before.saturating_sub(after);
                result.total_bytes_freed += freed;
                result.items_cleaned += 1;
                result.details.push(ExecutionItemResult::new(
                    "xcrun simctl delete unavailable".into(),
                    freed,
                    ActionType::ExternalCommand,
                    ActionStatus::Success,
                ));
            }
            Ok(_) => {
                result.items_failed += 1;
                result.details.push(ExecutionItemResult::new(
                    "xcrun simctl delete unavailable".into(),
                    0,
                    ActionType::ExternalCommand,
                    ActionStatus::Failed("simctl returned a non-zero exit code".into()),
                ));
            }
            Err(e) => {
                result.items_failed += 1;
                result.details.push(ExecutionItemResult::new(
                    "xcrun simctl delete unavailable".into(),
                    0,
                    ActionType::ExternalCommand,
                    ActionStatus::Failed(e.to_string()),
                ));
            }
        }

        Ok(result)
    }
}
