use crate::utils::fs::expand_tilde;
use std::path::{Component, Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

/// Pattern token resolved at runtime to the per-user cache directory under
/// `/var/folders` (`getconf DARWIN_USER_CACHE_DIR`), which differs per user and
/// per machine and so can never be written as a literal path.
pub const DARWIN_USER_CACHE_TOKEN: &str = "{darwin-user-cache}";

/// Categories of allowed targets
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum CleanCategory {
    UserCache,
    UserLogs,
    TempFiles,
    XcodeDerivedData,
    XcodeArchives,
    XcodeDeviceSupport,
    Homebrew,
    Npm,
    Pip,
    Gradle,
    Maven,
    Cargo,
    Go,
    Pnpm,
    XcodeSimulator,
    XcodeUnavailableSimulators,
    Docker,
    Trash,
    ExtendedAppCache,
    AppSupportCache,
    ContainerCache,
    MetalShaderCache,
    BiomeUsageData,
}

#[allow(dead_code)]
impl CleanCategory {
    pub fn name(&self) -> &'static str {
        match self {
            CleanCategory::UserCache => "User Caches (~/Library/Caches)",
            CleanCategory::UserLogs => "User Logs (~/Library/Logs)",
            CleanCategory::TempFiles => "Temporary Files (/tmp, /var/tmp)",
            CleanCategory::XcodeDerivedData => "Xcode DerivedData",
            CleanCategory::XcodeArchives => "Xcode Archives",
            CleanCategory::XcodeDeviceSupport => "Xcode iOS DeviceSupport",
            CleanCategory::Homebrew => "Homebrew Cache & Unused Packages",
            CleanCategory::Npm => "NPM Cache",
            CleanCategory::Pip => "PIP Cache",
            CleanCategory::Gradle => "Gradle Dependency Cache",
            CleanCategory::Maven => "Maven Repository Cache",
            CleanCategory::Cargo => "Cargo Registry & Git Caches",
            CleanCategory::Go => "Go Module Cache",
            CleanCategory::Pnpm => "pnpm Content-Addressable Store",
            CleanCategory::XcodeSimulator => "iOS Simulator Caches",
            CleanCategory::XcodeUnavailableSimulators => {
                "Unavailable iOS Simulator Devices (permanent — explicit only)"
            }
            CleanCategory::Docker => "Docker Unused Containers & Images",
            CleanCategory::Trash => "macOS Trash",
            CleanCategory::ExtendedAppCache => {
                "Extended Application Caches (beyond ~/Library/Caches)"
            }
            CleanCategory::AppSupportCache => {
                "Chromium/Electron App Caches (~/Library/Application Support/*/Cache…)"
            }
            CleanCategory::ContainerCache => {
                "Sandboxed App Caches (~/Library/Containers/*/Data/Library/Caches)"
            }
            CleanCategory::MetalShaderCache => "Metal Shader Caches (per-user /var/folders)",
            CleanCategory::BiomeUsageData => {
                "Biome Usage & Activity Data (personal — explicit only)"
            }
        }
    }

    pub fn identifier(&self) -> &'static str {
        match self {
            CleanCategory::UserCache => "user-cache",
            CleanCategory::UserLogs => "user-logs",
            CleanCategory::TempFiles => "temp-files",
            CleanCategory::XcodeDerivedData => "xcode-derived-data",
            CleanCategory::XcodeArchives => "xcode-archives",
            CleanCategory::XcodeDeviceSupport => "xcode-device-support",
            CleanCategory::Homebrew => "homebrew",
            CleanCategory::Npm => "npm",
            CleanCategory::Pip => "pip",
            CleanCategory::Gradle => "gradle",
            CleanCategory::Maven => "maven",
            CleanCategory::Cargo => "cargo",
            CleanCategory::Go => "go",
            CleanCategory::Pnpm => "pnpm",
            CleanCategory::XcodeSimulator => "xcode-simulator",
            CleanCategory::XcodeUnavailableSimulators => "unavailable-simulators",
            CleanCategory::Docker => "docker",
            CleanCategory::Trash => "trash",
            CleanCategory::ExtendedAppCache => "extended-app-cache",
            CleanCategory::AppSupportCache => "app-support-cache",
            CleanCategory::ContainerCache => "container-cache",
            CleanCategory::MetalShaderCache => "metal-cache",
            CleanCategory::BiomeUsageData => "biome",
        }
    }
}

/// Information about an allowed cleaning root directory.
///
/// `path_pattern` may start with `~` or [`DARWIN_USER_CACHE_TOKEN`], and may
/// contain `*` components. A `*` matches exactly one ordinary path component
/// (an app or bundle-id folder name) — never zero, never several, never `..` —
/// so `~/Library/Application Support/*/Cache` authorizes
/// `…/Application Support/Slack/Cache` but not `…/Application Support/Slack`
/// nor `…/Application Support/Slack/Profile/Cache`.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AllowedTarget {
    pub category: CleanCategory,
    pub path_pattern: &'static str,
    /// If true, we only delete items inside this directory, NEVER the root directory itself.
    pub contents_only: bool,
}

impl AllowedTarget {
    /// The pattern's fixed prefix, up to (not including) its first `*` component.
    /// For a pattern without wildcards this is the whole root.
    pub fn literal_base(&self) -> Option<PathBuf> {
        let expanded = expand_pattern(self.path_pattern)?;
        Some(
            expanded
                .components()
                .take_while(|c| c.as_os_str() != "*")
                .collect(),
        )
    }
}

pub const ALLOWED_TARGETS: &[AllowedTarget] = &[
    AllowedTarget {
        category: CleanCategory::UserCache,
        path_pattern: "~/Library/Caches",
        contents_only: true,
    },
    AllowedTarget {
        category: CleanCategory::UserLogs,
        path_pattern: "~/Library/Logs",
        contents_only: true,
    },
    AllowedTarget {
        category: CleanCategory::TempFiles,
        path_pattern: "/private/var/tmp",
        contents_only: true,
    },
    AllowedTarget {
        category: CleanCategory::TempFiles,
        path_pattern: "/private/tmp",
        contents_only: true,
    },
    AllowedTarget {
        category: CleanCategory::TempFiles,
        path_pattern: "/tmp",
        contents_only: true,
    },
    AllowedTarget {
        category: CleanCategory::TempFiles,
        path_pattern: "/var/tmp",
        contents_only: true,
    },
    AllowedTarget {
        category: CleanCategory::XcodeDerivedData,
        path_pattern: "~/Library/Developer/Xcode/DerivedData",
        contents_only: true,
    },
    AllowedTarget {
        category: CleanCategory::XcodeArchives,
        path_pattern: "~/Library/Developer/Xcode/Archives",
        contents_only: true,
    },
    AllowedTarget {
        category: CleanCategory::XcodeDeviceSupport,
        path_pattern: "~/Library/Developer/Xcode/iOS DeviceSupport",
        contents_only: true,
    },
    AllowedTarget {
        category: CleanCategory::Trash,
        path_pattern: "~/.Trash",
        contents_only: true,
    },
    AllowedTarget {
        category: CleanCategory::Npm,
        path_pattern: "~/.npm/_cacache",
        contents_only: true,
    },
    AllowedTarget {
        category: CleanCategory::Pip,
        path_pattern: "~/Library/Caches/pip",
        contents_only: true,
    },
    AllowedTarget {
        category: CleanCategory::Gradle,
        path_pattern: "~/.gradle/caches",
        contents_only: true,
    },
    AllowedTarget {
        category: CleanCategory::Maven,
        path_pattern: "~/.m2/repository",
        contents_only: true,
    },
    AllowedTarget {
        category: CleanCategory::Cargo,
        path_pattern: "~/.cargo/registry/cache",
        contents_only: true,
    },
    // Extracted crate sources: rebuilt from `registry/cache` (or re-downloaded) on the
    // next build, so removing them costs a decompression, never a re-resolution.
    AllowedTarget {
        category: CleanCategory::Cargo,
        path_pattern: "~/.cargo/registry/src",
        contents_only: true,
    },
    AllowedTarget {
        category: CleanCategory::Cargo,
        path_pattern: "~/.cargo/git/checkouts",
        contents_only: true,
    },
    AllowedTarget {
        category: CleanCategory::Cargo,
        path_pattern: "~/.cargo/git/db",
        contents_only: true,
    },
    // Downloaded Gradle distributions. Each project pins its own version via the
    // wrapper, so stale ones pile up and are re-fetched on demand if still needed.
    AllowedTarget {
        category: CleanCategory::Gradle,
        path_pattern: "~/.gradle/wrapper/dists",
        contents_only: true,
    },
    AllowedTarget {
        category: CleanCategory::Pnpm,
        path_pattern: "~/Library/pnpm/store",
        contents_only: true,
    },
    // Simulator *caches* only. Device data under CoreSimulator/Devices is never listed
    // here: it holds installed apps and their databases.
    AllowedTarget {
        category: CleanCategory::XcodeSimulator,
        path_pattern: "~/Library/Developer/CoreSimulator/Caches",
        contents_only: true,
    },
    // Downloaded dynamic wallpaper/screensaver assets: re-downloaded on demand,
    // not reachable from the generic ~/Library/Caches sweep.
    AllowedTarget {
        category: CleanCategory::ExtendedAppCache,
        path_pattern: "~/Library/Application Support/com.apple.mobileAssetDesktop",
        contents_only: true,
    },
    // fontconfig's own cache, under ~/.cache rather than ~/Library/Caches.
    AllowedTarget {
        category: CleanCategory::ExtendedAppCache,
        path_pattern: "~/.cache/fontconfig",
        contents_only: true,
    },
    // Chromium/Electron apps keep their HTTP, V8 code and GPU/shader caches one
    // level under their Application Support folder instead of ~/Library/Caches.
    // Only these exact Chromium cache folder names are allowed: everything else
    // in an app's Application Support folder is that app's real data.
    AllowedTarget {
        category: CleanCategory::AppSupportCache,
        path_pattern: "~/Library/Application Support/*/Cache",
        contents_only: true,
    },
    AllowedTarget {
        category: CleanCategory::AppSupportCache,
        path_pattern: "~/Library/Application Support/*/Code Cache",
        contents_only: true,
    },
    AllowedTarget {
        category: CleanCategory::AppSupportCache,
        path_pattern: "~/Library/Application Support/*/GPUCache",
        contents_only: true,
    },
    AllowedTarget {
        category: CleanCategory::AppSupportCache,
        path_pattern: "~/Library/Application Support/*/DawnCache",
        contents_only: true,
    },
    AllowedTarget {
        category: CleanCategory::AppSupportCache,
        path_pattern: "~/Library/Application Support/*/DawnGraphiteCache",
        contents_only: true,
    },
    AllowedTarget {
        category: CleanCategory::AppSupportCache,
        path_pattern: "~/Library/Application Support/*/DawnWebGPUCache",
        contents_only: true,
    },
    AllowedTarget {
        category: CleanCategory::AppSupportCache,
        path_pattern: "~/Library/Application Support/*/GrShaderCache",
        contents_only: true,
    },
    AllowedTarget {
        category: CleanCategory::AppSupportCache,
        path_pattern: "~/Library/Application Support/*/ShaderCache",
        contents_only: true,
    },
    // A sandboxed app's own Caches folder. Apple's contract for this location is
    // that the system may purge it at any time, so apps must tolerate its loss.
    // Mail and Messages containers stay protected by the blocklist, which the
    // validator checks before the allowlist.
    AllowedTarget {
        category: CleanCategory::ContainerCache,
        path_pattern: "~/Library/Containers/*/Data/Library/Caches",
        contents_only: true,
    },
    // Compiled Metal shaders, shared and per-app. Rebuilt the next time the
    // shader is used.
    AllowedTarget {
        category: CleanCategory::MetalShaderCache,
        path_pattern: "{darwin-user-cache}/com.apple.metal",
        contents_only: true,
    },
    AllowedTarget {
        category: CleanCategory::MetalShaderCache,
        path_pattern: "{darwin-user-cache}/com.apple.metalfe",
        contents_only: true,
    },
    AllowedTarget {
        category: CleanCategory::MetalShaderCache,
        path_pattern: "{darwin-user-cache}/*/com.apple.metal",
        contents_only: true,
    },
    AllowedTarget {
        category: CleanCategory::MetalShaderCache,
        path_pattern: "{darwin-user-cache}/*/com.apple.metalfe",
        contents_only: true,
    },
    // On-device usage and activity history (app usage, Siri suggestions). Not
    // a cache: the cleaner for it only runs when this category is asked for by
    // name, never as part of `--category all`.
    AllowedTarget {
        category: CleanCategory::BiomeUsageData,
        path_pattern: "~/Library/Biome",
        contents_only: true,
    },
];

/// Resolve the per-user `/var/folders/…/C` directory once per process.
pub fn darwin_user_cache_dir() -> Option<&'static Path> {
    static DIR: OnceLock<Option<PathBuf>> = OnceLock::new();
    DIR.get_or_init(|| {
        let output = Command::new("/usr/bin/getconf")
            .arg("DARWIN_USER_CACHE_DIR")
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let dir = String::from_utf8_lossy(&output.stdout).trim().to_string();
        (!dir.is_empty()).then(|| PathBuf::from(dir))
    })
    .as_deref()
}

/// Expand a pattern's leading `~` or [`DARWIN_USER_CACHE_TOKEN`]. `*`
/// components are left in place for [`match_pattern_root`]. `None` when the
/// token cannot be resolved on this machine.
pub fn expand_pattern(pattern: &str) -> Option<PathBuf> {
    if let Some(rest) = pattern.strip_prefix(DARWIN_USER_CACHE_TOKEN) {
        let base = darwin_user_cache_dir()?;
        let rest = rest.trim_start_matches('/');
        return Some(if rest.is_empty() {
            base.to_path_buf()
        } else {
            base.join(rest)
        });
    }
    Some(expand_tilde(pattern))
}

/// If `path` is the pattern's root or lies inside it, return that concrete root
/// (each `*` replaced by the component it matched).
///
/// Everything below the root must be ordinary names: a `..` would otherwise let a
/// path climb back out of an allowlisted root while still sharing its prefix.
fn match_pattern_root(pattern: &Path, path: &Path) -> Option<PathBuf> {
    let mut root = PathBuf::new();
    let mut actual = path.components();

    for expected in pattern.components() {
        let component = actual.next()?;
        let is_wildcard = expected.as_os_str() == "*";
        match (is_wildcard, component) {
            (true, Component::Normal(_)) => root.push(component),
            (true, _) => return None,
            (false, c) if c == expected => root.push(c),
            (false, _) => return None,
        }
    }

    if actual.any(|c| !matches!(c, Component::Normal(_))) {
        return None;
    }

    Some(root)
}

/// Verifies if a given path is within the allowed targets for cleaning.
pub fn find_allowed_target<P: AsRef<Path>>(path: P) -> Option<(&'static AllowedTarget, PathBuf)> {
    let path = path.as_ref();
    let abs_path = if path.is_relative() {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    } else {
        path.to_path_buf()
    };

    for target in ALLOWED_TARGETS {
        let Some(pattern) = expand_pattern(target.path_pattern) else {
            continue;
        };
        if let Some(root) = match_pattern_root(&pattern, &abs_path) {
            return Some((target, root));
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn home() -> PathBuf {
        dirs::home_dir().unwrap()
    }

    #[test]
    fn test_wildcard_matches_exactly_one_app_component() {
        let path = home().join("Library/Application Support/Slack/Cache/Cache_Data");
        let (target, root) = find_allowed_target(&path).expect("must be allowed");
        assert_eq!(target.category, CleanCategory::AppSupportCache);
        assert_eq!(root, home().join("Library/Application Support/Slack/Cache"));
    }

    #[test]
    fn test_wildcard_never_authorizes_the_app_folder_itself() {
        let app_data = home().join("Library/Application Support/Slack/storage/data.db");
        assert!(find_allowed_target(&app_data).is_none());

        let app_root = home().join("Library/Application Support/Slack");
        assert!(find_allowed_target(&app_root).is_none());
    }

    #[test]
    fn test_wildcard_does_not_match_deeper_cache_folders() {
        // A `Cache` nested inside a profile is that profile's business, not ours.
        let nested = home().join("Library/Application Support/Google/Chrome/Default/Cache/x");
        assert!(find_allowed_target(&nested).is_none());
    }

    #[test]
    fn test_parent_dir_components_cannot_escape_a_root() {
        let escape = home().join("Library/Application Support/Slack/Cache/../../../Documents");
        assert!(find_allowed_target(&escape).is_none());

        let escape = home().join("Library/Caches/../Keychains/login.keychain-db");
        assert!(find_allowed_target(&escape).is_none());
    }

    #[test]
    fn test_container_caches_match_only_the_caches_folder() {
        let cache = home().join("Library/Containers/com.example.app/Data/Library/Caches/blob");
        let (target, root) = find_allowed_target(&cache).expect("must be allowed");
        assert_eq!(target.category, CleanCategory::ContainerCache);
        assert_eq!(
            root,
            home().join("Library/Containers/com.example.app/Data/Library/Caches")
        );

        let prefs = home().join("Library/Containers/com.example.app/Data/Library/Preferences/x");
        assert!(find_allowed_target(&prefs).is_none());
    }

    #[test]
    fn test_literal_base_stops_at_first_wildcard() {
        let target = ALLOWED_TARGETS
            .iter()
            .find(|t| t.path_pattern == "~/Library/Containers/*/Data/Library/Caches")
            .unwrap();
        assert_eq!(
            target.literal_base().unwrap(),
            home().join("Library/Containers")
        );
    }

    #[test]
    fn test_metal_cache_resolves_under_darwin_user_cache_dir() {
        let Some(base) = darwin_user_cache_dir() else {
            return;
        };
        let shader = base.join("com.example.app/com.apple.metal/shaders.bin");
        let (target, root) = find_allowed_target(&shader).expect("must be allowed");
        assert_eq!(target.category, CleanCategory::MetalShaderCache);
        assert_eq!(root, base.join("com.example.app/com.apple.metal"));

        // Everything else under /var/folders stays off-limits.
        assert!(find_allowed_target(base.join("com.example.app/other")).is_none());
    }
}
