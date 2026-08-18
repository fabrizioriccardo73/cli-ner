use std::path::Path;
use sysinfo::{ProcessesToUpdate, System};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserDefinition {
    pub name: &'static str,
    /// Exact main process names
    pub process_names: &'static [&'static str],
    /// macOS App bundle names (e.g., "Google Chrome.app")
    pub app_bundle_patterns: &'static [&'static str],
    /// Directories in ~/Library/Caches to protect
    pub cache_directory_names: &'static [&'static str],
}

pub const SUPPORTED_BROWSERS: &[BrowserDefinition] = &[
    BrowserDefinition {
        name: "Google Chrome",
        process_names: &["Google Chrome", "Google Chrome Helper"],
        app_bundle_patterns: &["Google Chrome.app", "Google Chrome Canary.app"],
        cache_directory_names: &[
            "Google",
            "com.google.Chrome",
            "com.google.Chrome.helper",
            "com.google.Chrome.canary",
            "com.google.Keystone",
        ],
    },
    BrowserDefinition {
        name: "Safari",
        process_names: &["Safari", "SafariTechnologyPreview"],
        app_bundle_patterns: &["Safari.app", "Safari Technology Preview.app"],
        cache_directory_names: &[
            "com.apple.Safari",
            "com.apple.SafariTechnologyPreview",
            "com.apple.WebKit.WebContent",
            "com.apple.WebKit.Networking",
            "com.apple.Safari.SafeBrowsing",
        ],
    },
    BrowserDefinition {
        name: "Mozilla Firefox",
        process_names: &["firefox", "Firefox"],
        app_bundle_patterns: &[
            "Firefox.app",
            "Firefox Developer Edition.app",
            "Firefox Nightly.app",
        ],
        cache_directory_names: &["Firefox", "org.mozilla.firefox"],
    },
    BrowserDefinition {
        name: "Brave Browser",
        process_names: &["Brave Browser", "Brave Browser Helper"],
        app_bundle_patterns: &["Brave Browser.app"],
        cache_directory_names: &["BraveSoftware", "com.brave.Browser"],
    },
    BrowserDefinition {
        name: "Microsoft Edge",
        process_names: &["Microsoft Edge", "Microsoft Edge Helper"],
        app_bundle_patterns: &["Microsoft Edge.app", "Microsoft Edge Canary.app"],
        cache_directory_names: &["Microsoft Edge", "com.microsoft.edgemac"],
    },
    BrowserDefinition {
        name: "Arc Browser",
        process_names: &["Arc", "Arc Helper"],
        app_bundle_patterns: &["Arc.app"],
        cache_directory_names: &[
            "company.thebrowser.Browser",
            "company.thebrowser.arc",
            "Arc",
        ],
    },
    BrowserDefinition {
        name: "Opera",
        process_names: &["Opera", "Opera Helper", "Opera GX"],
        app_bundle_patterns: &["Opera.app", "Opera GX.app"],
        cache_directory_names: &["com.operasoftware.Opera", "com.operasoftware.OperaGX"],
    },
    BrowserDefinition {
        name: "Vivaldi",
        process_names: &["Vivaldi", "Vivaldi Helper"],
        app_bundle_patterns: &["Vivaldi.app"],
        cache_directory_names: &["com.vivaldi.Vivaldi", "Vivaldi"],
    },
    BrowserDefinition {
        name: "Chromium",
        process_names: &["Chromium", "Chromium Helper"],
        app_bundle_patterns: &["Chromium.app"],
        cache_directory_names: &["Chromium", "org.chromium.Chromium"],
    },
];

/// Identifies all browsers currently running on the system with strict process and bundle matching.
pub fn get_running_browsers() -> Vec<&'static BrowserDefinition> {
    let mut sys = System::new();
    sys.refresh_processes(ProcessesToUpdate::All);

    let mut running_browsers = Vec::new();

    for browser in SUPPORTED_BROWSERS {
        let is_running = sys.processes().values().any(|process| {
            let proc_name = process.name().to_string_lossy();

            // 1. Exact match on process name
            let matches_proc_name = browser
                .process_names
                .iter()
                .any(|&expected| proc_name == expected);

            if matches_proc_name {
                return true;
            }

            // 2. Match executable path within application bundle
            if let Some(exe_path) = process.exe() {
                let exe_str = exe_path.to_string_lossy();
                for bundle in browser.app_bundle_patterns {
                    let marker = format!("/{}/Contents/MacOS/", bundle);
                    let helper_marker = format!("/{}/Contents/Frameworks/", bundle);
                    if exe_str.contains(&marker) || exe_str.contains(&helper_marker) {
                        return true;
                    }
                }
            }

            false
        });

        if is_running {
            running_browsers.push(browser);
        }
    }

    running_browsers
}

/// Checks if a directory within ~/Library/Caches corresponds to any of the currently running browsers.
pub fn is_cache_entry_for_running_browser(
    dir_name: &str,
    running_browsers: &[&'static BrowserDefinition],
) -> Option<&'static BrowserDefinition> {
    let dir_lower = dir_name.to_lowercase();
    for browser in running_browsers {
        for cache_name in browser.cache_directory_names {
            if dir_lower == cache_name.to_lowercase() {
                return Some(*browser);
            }
        }
    }
    None
}

/// Checks if a full path corresponds to any of the currently running browsers.
#[allow(dead_code)]
pub fn is_cache_path_for_running_browser<P: AsRef<Path>>(
    path: P,
    running_browsers: &[&'static BrowserDefinition],
) -> Option<&'static BrowserDefinition> {
    if let Some(file_name) = path.as_ref().file_name() {
        let name_str = file_name.to_string_lossy();
        is_cache_entry_for_running_browser(&name_str, running_browsers)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_entry_matching() {
        let chrome = &SUPPORTED_BROWSERS[0];
        assert_eq!(chrome.name, "Google Chrome");

        let running = vec![chrome];

        assert_eq!(
            is_cache_entry_for_running_browser("Google", &running),
            Some(chrome)
        );
        assert_eq!(
            is_cache_entry_for_running_browser("com.google.Chrome", &running),
            Some(chrome)
        );
        assert_eq!(
            is_cache_entry_for_running_browser("google", &running),
            Some(chrome)
        );
        assert_eq!(
            is_cache_entry_for_running_browser("com.apple.Safari", &running),
            None
        );
    }

    #[test]
    fn test_safari_matching() {
        let safari = &SUPPORTED_BROWSERS[1];
        let running = vec![safari];

        assert_eq!(
            is_cache_entry_for_running_browser("com.apple.Safari", &running),
            Some(safari)
        );
        assert_eq!(
            is_cache_entry_for_running_browser("com.apple.WebKit.WebContent", &running),
            Some(safari)
        );
        assert_eq!(is_cache_entry_for_running_browser("Google", &running), None);
    }
}
