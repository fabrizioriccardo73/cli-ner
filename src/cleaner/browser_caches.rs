//! Caches that Chromium-based browsers keep inside their profiles under
//! `~/Library/Application Support/<vendor>/<browser>/<profile>/`, two or three
//! levels deeper than the generic `app-support-cache` sweep reaches.
//!
//! Only folders named in `safety::browser` are touched, and only inside a folder
//! that really is a Chromium user-data folder (it has a `Local State` file) and
//! a profile in it (it has a `Preferences` file). Passwords, cookies, history,
//! bookmarks, sessions, extensions and site databases sit next to the caches
//! and are never listed: the allowlist has no pattern that matches them.
//!
//! A browser that is running is skipped entirely — Chromium keeps these files
//! open and memory-mapped while it runs.

use crate::cleaner::extra_caches::children_as_targets;
use crate::cleaner::traits::{CleanTargetItem, Cleaner};
use crate::safety::allowlist::CleanCategory;
use crate::safety::browser::{
    get_running_browsers, BrowserDefinition, PROFILE_CACHE_LEAVES, SERVICE_WORKER_CACHE,
    SUPPORTED_BROWSERS, USER_DATA_CACHE_LEAVES,
};
use crate::utils::fs::{calculate_size, expand_tilde};
use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

const TOR_CACHE_ROOT: &str = "~/Library/Application Support/TorBrowser-Data/Browser/Caches";

/// One Chromium user-data folder found on disk.
struct ChromiumInstall {
    browser: &'static BrowserDefinition,
    /// `Google/Chrome`, `Microsoft Edge`, … — for the item descriptions.
    label: &'static str,
    user_data: PathBuf,
    /// Profile folders by name. Opera keeps its only profile in the user-data
    /// folder itself; that one is listed with an empty name.
    profiles: Vec<(String, PathBuf)>,
}

fn is_real_dir(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|m| m.file_type().is_dir())
        .unwrap_or(false)
}

/// Whether `dir` is a Chromium profile: Chromium writes `Preferences` in each one.
fn is_profile(dir: &Path) -> bool {
    dir.join("Preferences").is_file()
}

/// Every known Chromium user-data folder that exists under `support`, with its
/// profiles. A folder reached through a symbolic link anywhere below `support`
/// is ignored: it could point at data that is not a browser profile at all.
fn chromium_installs_in(support: &Path) -> Vec<ChromiumInstall> {
    let Ok(canonical_support) = fs::canonicalize(support) else {
        return Vec::new();
    };
    let mut installs = Vec::new();

    for browser in SUPPORTED_BROWSERS {
        for rel in browser.user_data_dirs {
            let user_data = support.join(rel);
            if !is_real_dir(&user_data) || !user_data.join("Local State").is_file() {
                continue;
            }
            if fs::canonicalize(&user_data).ok() != Some(canonical_support.join(rel)) {
                continue;
            }

            let mut profiles = Vec::new();
            if is_profile(&user_data) {
                profiles.push((String::new(), user_data.clone()));
            }
            if let Ok(entries) = fs::read_dir(&user_data) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if is_real_dir(&path) && is_profile(&path) {
                        profiles.push((entry.file_name().to_string_lossy().to_string(), path));
                    }
                }
            }
            profiles.sort();

            installs.push(ChromiumInstall {
                browser,
                label: rel,
                user_data,
                profiles,
            });
        }
    }

    installs
}

/// A whole cache folder as one target, if it is a real, non-empty folder.
fn folder_target(path: PathBuf, description: String) -> Option<CleanTargetItem> {
    if !is_real_dir(&path) {
        return None;
    }
    let (size, count) = calculate_size(&path).ok()?;
    (size > 0).then_some(CleanTargetItem {
        path,
        size_bytes: size,
        file_count: count,
        description,
    })
}

fn profile_label(install: &ChromiumInstall, profile: &str) -> String {
    if profile.is_empty() {
        install.label.to_string()
    } else {
        format!("{} › {}", install.label, profile)
    }
}

fn is_running(browser: &BrowserDefinition, running: &[&'static BrowserDefinition]) -> bool {
    running.iter().any(|r| r.name == browser.name)
}

fn sorted(mut items: Vec<CleanTargetItem>) -> Vec<CleanTargetItem> {
    items.sort_by_key(|i| std::cmp::Reverse(i.size_bytes));
    items
}

/// Cache folders of every profile of every browser that is not running.
fn scan_browser_caches(
    support: &Path,
    running: &[&'static BrowserDefinition],
) -> Vec<CleanTargetItem> {
    let mut items = Vec::new();

    for install in chromium_installs_in(support) {
        if is_running(install.browser, running) {
            continue;
        }

        for leaf in USER_DATA_CACHE_LEAVES {
            items.extend(folder_target(
                install.user_data.join(leaf),
                format!("{} {}", install.label, leaf),
            ));
        }
        for (profile, dir) in &install.profiles {
            for leaf in PROFILE_CACHE_LEAVES {
                // Opera's profile is the user-data folder: don't list a folder twice.
                if profile.is_empty() && USER_DATA_CACHE_LEAVES.contains(leaf) {
                    continue;
                }
                items.extend(folder_target(
                    dir.join(leaf),
                    format!("{} {}", profile_label(&install, profile), leaf),
                ));
            }
        }
    }

    items
}

// -------------------------------------------------------------------------------------
// Browser caches — part of the default run
// -------------------------------------------------------------------------------------

pub struct BrowserCacheCleaner;

impl Cleaner for BrowserCacheCleaner {
    fn name(&self) -> &'static str {
        "Browser Profile Caches"
    }

    fn category(&self) -> CleanCategory {
        CleanCategory::BrowserCache
    }

    fn description(&self) -> &'static str {
        "Code, GPU and shader caches inside each profile of Chrome, Edge, Brave, Arc, Vivaldi, \
         Opera and other Chromium browsers, plus Tor Browser's cache. Passwords, cookies, \
         history, bookmarks, open tabs and extensions are never touched; running browsers are \
         skipped"
    }

    fn scan(&self) -> Result<Vec<CleanTargetItem>> {
        let running = get_running_browsers();
        let mut items =
            scan_browser_caches(&expand_tilde("~/Library/Application Support"), &running);

        let tor_running = running.iter().any(|b| b.name == "Tor Browser");
        let tor = expand_tilde(TOR_CACHE_ROOT);
        if !tor_running && is_real_dir(&tor) {
            items.extend(children_as_targets(&tor, |child| {
                format!("Tor Browser cache: {}", child)
            }));
        }

        Ok(sorted(items))
    }
}

// -------------------------------------------------------------------------------------
// Service Worker Cache Storage — site data, explicit only
// -------------------------------------------------------------------------------------

pub struct BrowserOfflineDataCleaner;

impl Cleaner for BrowserOfflineDataCleaner {
    fn name(&self) -> &'static str {
        "Browser Offline Site Data"
    }

    fn category(&self) -> CleanCategory {
        CleanCategory::BrowserOfflineData
    }

    fn description(&self) -> &'static str {
        "Offline copies that websites and web apps saved in Chromium browser profiles through \
         Service Workers (Cache Storage). Not a plain cache: logins, cookies, passwords, history \
         and site databases stay, but web apps that work offline need a connection once to \
         download their files again. Only runs with `--category browser-offline-data`; running \
         browsers are skipped"
    }

    fn included_in_default_run(&self) -> bool {
        false
    }

    fn scan(&self) -> Result<Vec<CleanTargetItem>> {
        let running = get_running_browsers();
        let support = expand_tilde("~/Library/Application Support");
        let mut items = Vec::new();

        for install in chromium_installs_in(&support) {
            if is_running(install.browser, &running) {
                continue;
            }
            for (profile, dir) in &install.profiles {
                items.extend(folder_target(
                    dir.join(SERVICE_WORKER_CACHE),
                    format!("{} offline site data", profile_label(&install, profile)),
                ));
            }
        }

        Ok(sorted(items))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::safety::allowlist::find_allowed_target;
    use crate::safety::browser::SUPPORTED_BROWSERS;

    fn write(path: &Path, bytes: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![b'x'; bytes]).unwrap();
    }

    /// A fake Application Support with a Chrome install holding two profiles,
    /// their caches, and the personal files that must never be listed.
    fn fake_support(tag: &str) -> PathBuf {
        let support = std::env::temp_dir().join(format!("cli-ner-browser-{tag}"));
        let _ = fs::remove_dir_all(&support);
        let chrome = support.join("Google/Chrome");
        write(&chrome.join("Local State"), 10);
        write(&chrome.join("GrShaderCache/data_0"), 4096);
        write(&chrome.join("component_crx_cache/abc"), 4096);
        for profile in ["Default", "Profile 1"] {
            let p = chrome.join(profile);
            write(&p.join("Preferences"), 10);
            write(&p.join("Code Cache/js/index"), 4096);
            write(&p.join("GPUCache/data_0"), 4096);
            write(
                &p.join("Service Worker/CacheStorage/origin/index.txt"),
                4096,
            );
            write(&p.join("Service Worker/ScriptCache/index"), 4096);
            for personal in ["Login Data", "Cookies", "History", "Bookmarks", "Web Data"] {
                write(&p.join(personal), 4096);
            }
            write(&p.join("Local Storage/leveldb/000003.log"), 4096);
            write(&p.join("IndexedDB/site/000003.log"), 4096);
        }
        // Not a profile (no Preferences): its folders are left alone.
        write(&chrome.join("Crashpad/Code Cache/x"), 4096);
        support
    }

    fn not_running() -> Vec<&'static BrowserDefinition> {
        Vec::new()
    }

    #[test]
    fn test_scan_lists_only_cache_folders_of_real_profiles() {
        let support = fake_support("scan");
        let items = scan_browser_caches(&support, &not_running());
        let chrome = support.join("Google/Chrome");

        let mut paths: Vec<PathBuf> = items.iter().map(|i| i.path.clone()).collect();
        paths.sort();
        let mut expected = vec![
            chrome.join("GrShaderCache"),
            chrome.join("component_crx_cache"),
            chrome.join("Default/Code Cache"),
            chrome.join("Default/GPUCache"),
            chrome.join("Profile 1/Code Cache"),
            chrome.join("Profile 1/GPUCache"),
        ];
        expected.sort();
        assert_eq!(paths, expected);

        let _ = fs::remove_dir_all(&support);
    }

    #[test]
    fn test_running_browser_is_skipped() {
        let support = fake_support("running");
        let chrome = SUPPORTED_BROWSERS
            .iter()
            .find(|b| b.name == "Google Chrome")
            .unwrap();
        assert!(scan_browser_caches(&support, &[chrome]).is_empty());
        let _ = fs::remove_dir_all(&support);
    }

    #[test]
    fn test_folder_without_local_state_is_not_a_browser() {
        let support = fake_support("nolocalstate");
        fs::remove_file(support.join("Google/Chrome/Local State")).unwrap();
        assert!(scan_browser_caches(&support, &not_running()).is_empty());
        let _ = fs::remove_dir_all(&support);
    }

    #[test]
    fn test_symlinked_user_data_folder_is_ignored() {
        let support = fake_support("symlink");
        let real = support.join("Google/Chrome");
        let elsewhere = support.join("Elsewhere");
        fs::rename(&real, &elsewhere).unwrap();
        std::os::unix::fs::symlink(&elsewhere, &real).unwrap();

        assert!(scan_browser_caches(&support, &not_running()).is_empty());
        let _ = fs::remove_dir_all(&support);
    }

    /// Opera keeps its profile in the user-data folder itself.
    #[test]
    fn test_profile_in_the_user_data_folder_is_found_once() {
        let support = std::env::temp_dir().join("cli-ner-browser-opera");
        let _ = fs::remove_dir_all(&support);
        let opera = support.join("com.operasoftware.Opera");
        write(&opera.join("Local State"), 10);
        write(&opera.join("Preferences"), 10);
        write(&opera.join("Code Cache/js/index"), 4096);
        write(&opera.join("GrShaderCache/data_0"), 4096);

        let mut paths: Vec<PathBuf> = scan_browser_caches(&support, &not_running())
            .into_iter()
            .map(|i| i.path)
            .collect();
        paths.sort();
        assert_eq!(
            paths,
            vec![opera.join("Code Cache"), opera.join("GrShaderCache")]
        );
        let _ = fs::remove_dir_all(&support);
    }

    /// The scanner and the allowlist describe the same folders in two places;
    /// every folder the scanner would offer must pass validation.
    #[test]
    fn test_every_leaf_of_every_browser_is_allowlisted() {
        let support = expand_tilde("~/Library/Application Support");
        for browser in SUPPORTED_BROWSERS {
            for rel in browser.user_data_dirs {
                let root = support.join(rel);
                for leaf in USER_DATA_CACHE_LEAVES.iter().chain(PROFILE_CACHE_LEAVES) {
                    let (t, r) = find_allowed_target(root.join(leaf)).unwrap();
                    assert_eq!(t.category, CleanCategory::BrowserCache);
                    assert_eq!(r, root.join(leaf));
                }
                for leaf in PROFILE_CACHE_LEAVES {
                    let path = root.join("Profile 7").join(leaf);
                    let (t, _) = find_allowed_target(&path).unwrap();
                    assert_eq!(
                        t.category,
                        CleanCategory::BrowserCache,
                        "{}",
                        path.display()
                    );
                }
                let sw = root.join("Default").join(SERVICE_WORKER_CACHE);
                let (t, _) = find_allowed_target(&sw).unwrap();
                assert_eq!(t.category, CleanCategory::BrowserOfflineData);
            }
        }
        assert!(find_allowed_target(expand_tilde(TOR_CACHE_ROOT).join("profile")).is_some());
    }

    #[test]
    fn test_offline_site_data_never_runs_as_part_of_all() {
        assert!(!BrowserOfflineDataCleaner.included_in_default_run());
        assert!(BrowserCacheCleaner.included_in_default_run());
    }
}
