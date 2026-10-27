use std::path::Path;
use sysinfo::{ProcessesToUpdate, System};

/// Cache folders Chromium keeps inside each profile (`<user data>/Default`,
/// `<user data>/Profile 1`, …). All are rebuilt by the browser on demand.
/// The main HTTP cache usually lives under ~/Library/Caches instead, but some
/// browsers (Arc, Electron-style builds) keep `Cache` in the profile.
pub const PROFILE_CACHE_LEAVES: &[&str] = &[
    "Cache",
    "Code Cache",
    "GPUCache",
    "DawnCache",
    "DawnGraphiteCache",
    "DawnWebGPUCache",
    "GrShaderCache",
    "ShaderCache",
];

/// Cache folders at the top of a Chromium user-data folder, shared by all
/// profiles: compiled GPU shaders, and installer packages of components and
/// extensions kept for differential updates (re-downloaded when needed —
/// installed extensions themselves live in each profile and are not touched).
pub const USER_DATA_CACHE_LEAVES: &[&str] = &[
    "GrShaderCache",
    "ShaderCache",
    "GraphiteDawnCache",
    "component_crx_cache",
    "extensions_crx_cache",
];

/// Offline copies sites store through the Cache API. Site data rather than a
/// plain cache, so it has its own explicit-only category.
pub const SERVICE_WORKER_CACHE: &str = "Service Worker/CacheStorage";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BrowserDefinition {
    pub name: &'static str,
    /// Exact main process names
    pub process_names: &'static [&'static str],
    /// macOS App bundle names (e.g., "Google Chrome.app")
    pub app_bundle_patterns: &'static [&'static str],
    /// Directories in ~/Library/Caches to protect
    pub cache_directory_names: &'static [&'static str],
    /// Chromium "user data" folders, relative to `~/Library/Application Support`.
    /// Each holds a `Local State` file and one folder per profile; the profile
    /// caches inside are cleaned by `cleaner::browser_caches`. Empty for
    /// browsers that keep every cache under ~/Library/Caches (Safari, Firefox).
    pub user_data_dirs: &'static [&'static str],
}

pub const SUPPORTED_BROWSERS: &[BrowserDefinition] = &[
    BrowserDefinition {
        name: "Google Chrome",
        process_names: &["Google Chrome", "Google Chrome Helper"],
        app_bundle_patterns: &[
            "Google Chrome.app",
            "Google Chrome Beta.app",
            "Google Chrome Dev.app",
            "Google Chrome Canary.app",
        ],
        cache_directory_names: &[
            "Google",
            "com.google.Chrome",
            "com.google.Chrome.helper",
            "com.google.Chrome.beta",
            "com.google.Chrome.dev",
            "com.google.Chrome.canary",
            "com.google.Keystone",
        ],
        user_data_dirs: &[
            "Google/Chrome",
            "Google/Chrome Beta",
            "Google/Chrome Dev",
            "Google/Chrome Canary",
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
        user_data_dirs: &[],
    },
    BrowserDefinition {
        name: "Mozilla Firefox",
        process_names: &["firefox", "Firefox"],
        app_bundle_patterns: &[
            "Firefox.app",
            "Firefox Developer Edition.app",
            "Firefox Nightly.app",
        ],
        cache_directory_names: &["Firefox", "org.mozilla.firefox", "Mozilla"],
        user_data_dirs: &[],
    },
    BrowserDefinition {
        name: "Brave Browser",
        process_names: &["Brave Browser", "Brave Browser Helper"],
        app_bundle_patterns: &[
            "Brave Browser.app",
            "Brave Browser Beta.app",
            "Brave Browser Nightly.app",
        ],
        cache_directory_names: &["BraveSoftware", "com.brave.Browser"],
        user_data_dirs: &[
            "BraveSoftware/Brave-Browser",
            "BraveSoftware/Brave-Browser-Beta",
            "BraveSoftware/Brave-Browser-Nightly",
        ],
    },
    BrowserDefinition {
        name: "Microsoft Edge",
        process_names: &["Microsoft Edge", "Microsoft Edge Helper"],
        app_bundle_patterns: &[
            "Microsoft Edge.app",
            "Microsoft Edge Beta.app",
            "Microsoft Edge Dev.app",
            "Microsoft Edge Canary.app",
        ],
        cache_directory_names: &["Microsoft Edge", "com.microsoft.edgemac"],
        user_data_dirs: &[
            "Microsoft Edge",
            "Microsoft Edge Beta",
            "Microsoft Edge Dev",
            "Microsoft Edge Canary",
        ],
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
        user_data_dirs: &["Arc/User Data"],
    },
    BrowserDefinition {
        name: "Opera",
        process_names: &["Opera", "Opera Helper", "Opera GX", "Opera Air"],
        app_bundle_patterns: &["Opera.app", "Opera GX.app", "Opera Air.app"],
        cache_directory_names: &[
            "com.operasoftware.Opera",
            "com.operasoftware.OperaGX",
            "com.operasoftware.OperaAir",
        ],
        user_data_dirs: &[
            "com.operasoftware.Opera",
            "com.operasoftware.OperaGX",
            "com.operasoftware.OperaAir",
        ],
    },
    BrowserDefinition {
        name: "Vivaldi",
        process_names: &["Vivaldi", "Vivaldi Helper"],
        app_bundle_patterns: &["Vivaldi.app"],
        cache_directory_names: &["com.vivaldi.Vivaldi", "Vivaldi"],
        user_data_dirs: &["Vivaldi"],
    },
    BrowserDefinition {
        name: "Chromium",
        process_names: &["Chromium", "Chromium Helper"],
        app_bundle_patterns: &["Chromium.app"],
        cache_directory_names: &["Chromium", "org.chromium.Chromium"],
        user_data_dirs: &["Chromium"],
    },
    BrowserDefinition {
        name: "Dia",
        process_names: &["Dia", "Dia Helper"],
        app_bundle_patterns: &["Dia.app"],
        cache_directory_names: &["company.thebrowser.dia", "Dia"],
        user_data_dirs: &["Dia/User Data"],
    },
    BrowserDefinition {
        name: "Comet",
        process_names: &["Comet", "Comet Helper"],
        app_bundle_patterns: &["Comet.app"],
        cache_directory_names: &["Comet", "ai.perplexity.comet"],
        user_data_dirs: &["Comet"],
    },
    BrowserDefinition {
        name: "ChatGPT Atlas",
        process_names: &["ChatGPT Atlas"],
        app_bundle_patterns: &["ChatGPT Atlas.app"],
        cache_directory_names: &["com.openai.atlas"],
        user_data_dirs: &["com.openai.atlas/browser-data/host"],
    },
    BrowserDefinition {
        name: "Helium",
        process_names: &["Helium", "Helium Helper"],
        app_bundle_patterns: &["Helium.app"],
        cache_directory_names: &["net.imput.helium"],
        user_data_dirs: &["net.imput.helium"],
    },
    BrowserDefinition {
        name: "Thorium",
        process_names: &["Thorium", "Thorium Helper"],
        app_bundle_patterns: &["Thorium.app"],
        cache_directory_names: &["Thorium"],
        user_data_dirs: &["Thorium"],
    },
    BrowserDefinition {
        name: "Yandex Browser",
        process_names: &["Yandex", "Yandex Helper"],
        app_bundle_patterns: &["Yandex.app"],
        cache_directory_names: &["Yandex", "ru.yandex.desktop.yandex-browser"],
        user_data_dirs: &["Yandex/YandexBrowser"],
    },
    BrowserDefinition {
        name: "BrowserOS",
        process_names: &["BrowserOS", "BrowserOS Helper"],
        app_bundle_patterns: &["BrowserOS.app"],
        cache_directory_names: &["BrowserOS"],
        user_data_dirs: &["BrowserOS"],
    },
    BrowserDefinition {
        name: "DuckDuckGo",
        process_names: &["DuckDuckGo"],
        app_bundle_patterns: &["DuckDuckGo.app"],
        cache_directory_names: &["com.duckduckgo.macos.browser"],
        user_data_dirs: &[],
    },
    BrowserDefinition {
        name: "Orion",
        process_names: &["Orion"],
        app_bundle_patterns: &["Orion.app", "Orion RC.app"],
        cache_directory_names: &["com.kagi.kagimacOS", "com.kagi.kagimacOS.IconService"],
        user_data_dirs: &[],
    },
    BrowserDefinition {
        name: "Zen Browser",
        process_names: &["zen"],
        app_bundle_patterns: &["Zen.app", "Zen Browser.app"],
        cache_directory_names: &["zen"],
        user_data_dirs: &[],
    },
    BrowserDefinition {
        name: "LibreWolf",
        process_names: &["librewolf"],
        app_bundle_patterns: &["LibreWolf.app"],
        cache_directory_names: &["librewolf"],
        user_data_dirs: &[],
    },
    BrowserDefinition {
        name: "Floorp",
        process_names: &["floorp"],
        app_bundle_patterns: &["Floorp.app"],
        cache_directory_names: &["Floorp"],
        user_data_dirs: &[],
    },
    BrowserDefinition {
        name: "Waterfox",
        process_names: &["waterfox"],
        app_bundle_patterns: &["Waterfox.app"],
        cache_directory_names: &["Waterfox"],
        user_data_dirs: &[],
    },
    // Both run an executable called `firefox`, so they are recognised by bundle.
    BrowserDefinition {
        name: "Tor Browser",
        process_names: &[],
        app_bundle_patterns: &["Tor Browser.app"],
        cache_directory_names: &[],
        user_data_dirs: &[],
    },
    BrowserDefinition {
        name: "Mullvad Browser",
        process_names: &[],
        app_bundle_patterns: &["Mullvad Browser.app"],
        cache_directory_names: &["MullvadBrowser"],
        user_data_dirs: &[],
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
