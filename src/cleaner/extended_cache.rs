//! Cache locations outside `~/Library/Caches` that the generic `UserCacheCleaner`
//! sweep never reaches, because they live under a different root entirely.
//!
//! Scope deliberately narrow: only fixed, user-space, no-sudo, redownloadable/
//! regenerable paths. System-level caches under `/System` or `/cores`, and any
//! pattern with a variable app-name segment in the middle of the path (e.g.
//! `~/Library/Application Support/<app>/Cache`), are intentionally left out —
//! see docs/MAINTENANCE_ROADMAP.md, Fase M3, for why.

use crate::cleaner::traits::{CleanTargetItem, Cleaner};
use crate::safety::allowlist::CleanCategory;
use crate::utils::fs::{calculate_size, expand_tilde};
use anyhow::Result;
use std::fs;

/// Roots whose *contents* (not the root itself) are safe to clean: each is a
/// fixed, known, user-space cache directory with no personal data in it.
const EXTENDED_CACHE_ROOTS: &[(&str, &str)] = &[
    (
        "~/Library/Application Support/com.apple.mobileAssetDesktop",
        "Downloaded dynamic wallpaper/screensaver asset",
    ),
    (
        "~/.cache/fontconfig",
        "fontconfig cache entry",
    ),
];

pub struct ExtendedAppCacheCleaner;

impl Cleaner for ExtendedAppCacheCleaner {
    fn name(&self) -> &'static str {
        "Extended Application Caches"
    }

    fn category(&self) -> CleanCategory {
        CleanCategory::ExtendedAppCache
    }

    fn description(&self) -> &'static str {
        "Removes cache content from fixed user-space locations outside ~/Library/Caches \
         (downloaded dynamic wallpaper assets, fontconfig cache) — all redownloadable or \
         regenerated on demand"
    }

    fn scan(&self) -> Result<Vec<CleanTargetItem>> {
        let mut items = Vec::new();

        for (root_pattern, label) in EXTENDED_CACHE_ROOTS {
            let root = expand_tilde(root_pattern);
            if !root.is_dir() {
                continue;
            }

            if let Ok(entries) = fs::read_dir(&root) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if let Ok((size, count)) = calculate_size(&path) {
                        if size > 0 {
                            let name = entry.file_name().to_string_lossy().to_string();
                            items.push(CleanTargetItem {
                                path,
                                size_bytes: size,
                                file_count: count,
                                description: format!("{}: {}", label, name),
                            });
                        }
                    }
                }
            }
        }

        items.sort_by_key(|b| std::cmp::Reverse(b.size_bytes));
        Ok(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::safety::allowlist::find_allowed_target;

    /// Every root this cleaner scans must already be registered in the
    /// allowlist, or `clean_items`'s shared validator would reject everything
    /// it finds — silently turning the cleaner into a no-op.
    ///
    /// Pure path comparison, no filesystem access: `find_allowed_target` does
    /// not require the path to exist, so this never touches real directories
    /// under the user's home (unlike the other cleaners' tests, which use a
    /// scratch subdirectory of an already-existing, already-allowlisted root).
    #[test]
    fn test_every_scanned_root_is_allowlisted() {
        for (root_pattern, _) in EXTENDED_CACHE_ROOTS {
            let synthetic_child = expand_tilde(root_pattern).join("cli-ner-allowlist-probe");
            assert!(
                find_allowed_target(&synthetic_child).is_some(),
                "{} is not covered by any AllowedTarget",
                root_pattern
            );
        }
    }
}
