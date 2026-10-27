use crate::utils::fs::expand_tilde;
use std::path::{Path, PathBuf};

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
    Docker,
    Trash,
    ExtendedAppCache,
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
            CleanCategory::XcodeSimulator => "iOS Simulator Caches & Unavailable Devices",
            CleanCategory::Docker => "Docker Unused Containers & Images",
            CleanCategory::Trash => "macOS Trash",
            CleanCategory::ExtendedAppCache => {
                "Extended Application Caches (beyond ~/Library/Caches)"
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
            CleanCategory::Docker => "docker",
            CleanCategory::Trash => "trash",
            CleanCategory::ExtendedAppCache => "extended-app-cache",
        }
    }
}

/// Information about an allowed cleaning root directory.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct AllowedTarget {
    pub category: CleanCategory,
    pub path_pattern: &'static str,
    /// If true, we only delete items inside this directory, NEVER the root directory itself.
    pub contents_only: bool,
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
];

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
        let expanded = expand_tilde(target.path_pattern);
        if abs_path == expanded {
            // Target matches root
            return Some((target, expanded));
        } else if abs_path.starts_with(&expanded) {
            return Some((target, expanded));
        }
    }

    None
}
