//! Per-app caches whose location contains the app's own folder name, so they can
//! only be reached through the allowlist's `*` patterns: Chromium/Electron caches
//! under Application Support, sandboxed apps' container caches, and compiled
//! Metal shaders under `/var/folders`. Plus Biome, which is personal data and
//! therefore explicit-only.
//!
//! Every scanner here skips apps that are running: deleting a cache out from
//! under a live Chromium/Metal process is how pages and windows break.

use crate::cleaner::extra_caches::children_as_targets;
use crate::cleaner::traits::{CleanTargetItem, Cleaner};
use crate::safety::allowlist::{darwin_user_cache_dir, CleanCategory};
use crate::safety::blocklist::is_blocked;
use crate::safety::running_apps::RunningApps;
use crate::utils::fs::expand_tilde;
use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

/// Chromium's cache folder names. Must stay in sync with the
/// `AppSupportCache` entries in `safety::allowlist::ALLOWED_TARGETS`.
pub const CHROMIUM_CACHE_DIRS: &[&str] = &[
    "Cache",
    "Code Cache",
    "GPUCache",
    "DawnCache",
    "DawnGraphiteCache",
    "DawnWebGPUCache",
    "GrShaderCache",
    "ShaderCache",
];

const METAL_CACHE_DIRS: &[&str] = &["com.apple.metal", "com.apple.metalfe"];

/// Direct subdirectories of `dir` that are real directories, not symlinks: a
/// symlinked app folder could point anywhere, so it is never descended into.
fn real_subdirs(dir: &Path) -> Vec<(String, PathBuf)> {
    let Ok(entries) = fs::read_dir(dir) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .map(|e| (e.file_name().to_string_lossy().to_string(), e.path()))
        .collect()
}

fn sorted(mut items: Vec<CleanTargetItem>) -> Vec<CleanTargetItem> {
    items.sort_by_key(|i| std::cmp::Reverse(i.size_bytes));
    items
}

// -------------------------------------------------------------------------------------
// ~/Library/Application Support/<app>/{Cache, Code Cache, GPUCache, …}
// -------------------------------------------------------------------------------------

pub struct AppSupportCacheCleaner;

impl Cleaner for AppSupportCacheCleaner {
    fn name(&self) -> &'static str {
        "Chromium/Electron App Caches"
    }

    fn category(&self) -> CleanCategory {
        CleanCategory::AppSupportCache
    }

    fn description(&self) -> &'static str {
        "HTTP, V8 code and GPU/shader caches that Electron and Chromium-based apps keep \
         directly under ~/Library/Application Support/<app>/ (apps that are running are skipped)"
    }

    fn scan(&self) -> Result<Vec<CleanTargetItem>> {
        let running = RunningApps::snapshot();
        let mut items = Vec::new();

        for (app, app_dir) in real_subdirs(&expand_tilde("~/Library/Application Support")) {
            if running.owns(&app) || is_blocked(&app_dir).0 {
                continue;
            }
            for leaf in CHROMIUM_CACHE_DIRS {
                let cache_dir = app_dir.join(leaf);
                if cache_dir.is_dir() && !cache_dir.is_symlink() {
                    items.extend(children_as_targets(&cache_dir, |child| {
                        format!("{} {}: {}", app, leaf, child)
                    }));
                }
            }
        }

        Ok(sorted(items))
    }
}

// -------------------------------------------------------------------------------------
// ~/Library/Containers/<bundle-id>/Data/Library/Caches
// -------------------------------------------------------------------------------------

pub struct ContainerCacheCleaner;

impl Cleaner for ContainerCacheCleaner {
    fn name(&self) -> &'static str {
        "Sandboxed App Caches"
    }

    fn category(&self) -> CleanCategory {
        CleanCategory::ContainerCache
    }

    fn description(&self) -> &'static str {
        "Each sandboxed app's own Library/Caches inside ~/Library/Containers — the folder macOS \
         itself may purge at any time (running apps, Mail and Messages are skipped)"
    }

    fn scan(&self) -> Result<Vec<CleanTargetItem>> {
        let running = RunningApps::snapshot();
        let mut items = Vec::new();

        for (bundle_id, container) in real_subdirs(&expand_tilde("~/Library/Containers")) {
            if running.owns(&bundle_id) {
                continue;
            }
            let caches = container.join("Data/Library/Caches");
            if is_blocked(&caches).0 || !caches.is_dir() || caches.is_symlink() {
                continue;
            }
            items.extend(children_as_targets(&caches, |child| {
                format!("{} cache: {}", bundle_id, child)
            }));
        }

        Ok(sorted(items))
    }
}

// -------------------------------------------------------------------------------------
// $(getconf DARWIN_USER_CACHE_DIR)[/<bundle-id>]/com.apple.metal[fe]
// -------------------------------------------------------------------------------------

pub struct MetalShaderCacheCleaner;

impl Cleaner for MetalShaderCacheCleaner {
    fn name(&self) -> &'static str {
        "Metal Shader Caches"
    }

    fn category(&self) -> CleanCategory {
        CleanCategory::MetalShaderCache
    }

    fn description(&self) -> &'static str {
        "Compiled GPU shaders (shared and per-app) in the per-user cache under /var/folders; \
         recompiled the next time each shader is used (running apps' own caches are skipped)"
    }

    fn is_available(&self) -> bool {
        darwin_user_cache_dir().is_some()
    }

    fn scan(&self) -> Result<Vec<CleanTargetItem>> {
        let Some(base) = darwin_user_cache_dir() else {
            return Ok(Vec::new());
        };
        let running = RunningApps::snapshot();
        let mut items = Vec::new();

        let mut roots: Vec<(String, PathBuf)> = METAL_CACHE_DIRS
            .iter()
            .map(|d| ("shared".to_string(), base.join(d)))
            .collect();
        for (bundle_id, dir) in real_subdirs(base) {
            if running.owns(&bundle_id) {
                continue;
            }
            for d in METAL_CACHE_DIRS {
                roots.push((bundle_id.clone(), dir.join(d)));
            }
        }

        for (owner, root) in roots {
            if root.is_dir() && !root.is_symlink() {
                items.extend(children_as_targets(&root, |child| {
                    format!("Metal shaders ({}): {}", owner, child)
                }));
            }
        }

        Ok(sorted(items))
    }
}

// -------------------------------------------------------------------------------------
// ~/Library/Biome — personal data, explicit only
// -------------------------------------------------------------------------------------

pub struct BiomeCleaner;

impl Cleaner for BiomeCleaner {
    fn name(&self) -> &'static str {
        "Biome Usage & Activity Data"
    }

    fn category(&self) -> CleanCategory {
        CleanCategory::BiomeUsageData
    }

    fn description(&self) -> &'static str {
        "On-device history of app usage and activity used for Siri and system suggestions. \
         Personal data, not a cache: suggestions start over from scratch. Only runs with \
         `--category biome`; reading it requires Full Disk Access"
    }

    fn included_in_default_run(&self) -> bool {
        false
    }

    fn scan(&self) -> Result<Vec<CleanTargetItem>> {
        let biome = expand_tilde("~/Library/Biome");
        if !biome.is_dir() {
            return Ok(Vec::new());
        }
        // Surface "Operation not permitted" instead of reporting an empty folder:
        // the clean command explains Full Disk Access when it sees that error.
        fs::read_dir(&biome)?;
        Ok(children_as_targets(&biome, |child| {
            format!("Biome stream: {}", child)
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::safety::allowlist::find_allowed_target;

    /// The scanner and the allowlist describe the same folders in two places; if
    /// they drift apart, the validator silently rejects everything found.
    #[test]
    fn test_every_chromium_leaf_is_allowlisted() {
        let app = expand_tilde("~/Library/Application Support/SomeElectronApp");
        for leaf in CHROMIUM_CACHE_DIRS {
            let probe = app.join(leaf).join("entry");
            let (target, _) =
                find_allowed_target(&probe).unwrap_or_else(|| panic!("{leaf} is not allowlisted"));
            assert_eq!(target.category, CleanCategory::AppSupportCache);
        }
    }

    #[test]
    fn test_every_metal_leaf_is_allowlisted() {
        let Some(base) = darwin_user_cache_dir() else {
            return;
        };
        for leaf in METAL_CACHE_DIRS {
            for root in [base.join(leaf), base.join("com.example.app").join(leaf)] {
                let (target, _) = find_allowed_target(root.join("entry"))
                    .unwrap_or_else(|| panic!("{} is not allowlisted", root.display()));
                assert_eq!(target.category, CleanCategory::MetalShaderCache);
            }
        }
    }

    #[test]
    fn test_biome_never_runs_as_part_of_all() {
        assert!(!BiomeCleaner.included_in_default_run());
        assert!(AppSupportCacheCleaner.included_in_default_run());
    }

    #[test]
    fn test_scanned_container_targets_pass_the_blocklist() {
        for item in ContainerCacheCleaner.scan().unwrap() {
            assert!(!is_blocked(&item.path).0, "{}", item.path.display());
        }
    }
}
