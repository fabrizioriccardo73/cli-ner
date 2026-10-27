//! Which apps are running right now, so their live caches can be left alone.
//!
//! Matching a cache folder to its owner is necessarily heuristic: folders are
//! named after the app (`Slack`), its bundle id (`com.microsoft.teams2`) or a
//! helper's bundle id (`com.google.Chrome.helper`). The checks below err on the
//! side of "it's running" — a false positive only skips a cache this time.

use std::collections::HashSet;
use std::process::Command;
use sysinfo::{ProcessesToUpdate, System};

pub struct RunningApps {
    /// Bundle ids of running GUI apps, lowercased (from `lsappinfo`).
    bundle_ids: HashSet<String>,
    /// Running process names, lowercased.
    process_names: Vec<String>,
    /// Names of the `.app` bundles running executables live in, lowercased
    /// (`/Applications/Slack.app/Contents/MacOS/Slack` → `slack`).
    app_bundles: HashSet<String>,
}

impl RunningApps {
    pub fn snapshot() -> Self {
        let mut sys = System::new();
        sys.refresh_processes(ProcessesToUpdate::All);

        let mut process_names = Vec::new();
        let mut app_bundles = HashSet::new();
        for process in sys.processes().values() {
            process_names.push(process.name().to_string_lossy().to_lowercase());
            if let Some(exe) = process.exe() {
                for component in exe.components() {
                    let part = component.as_os_str().to_string_lossy().to_lowercase();
                    if let Some(app) = part.strip_suffix(".app") {
                        app_bundles.insert(app.to_string());
                    }
                }
            }
        }

        Self {
            bundle_ids: running_bundle_ids(),
            process_names,
            app_bundles,
        }
    }

    /// Whether the cache folder `folder_name` belongs to something running.
    pub fn owns(&self, folder_name: &str) -> bool {
        let name = folder_name.to_lowercase();
        let base_id = name.strip_suffix(".helper").unwrap_or(&name);
        let short = base_id.rsplit('.').next().unwrap_or(base_id);

        if self.bundle_ids.contains(&name) || self.bundle_ids.contains(base_id) {
            return true;
        }
        if self.app_bundles.contains(&name) || self.app_bundles.contains(short) {
            return true;
        }

        // `Code` must match `Code` and `Code Helper (Renderer)`, but not `Codex`.
        let helper_prefix = format!("{name} ");
        self.process_names
            .iter()
            .any(|p| *p == name || *p == short || p.starts_with(&helper_prefix))
    }

    #[cfg(test)]
    fn from_parts(bundle_ids: &[&str], process_names: &[&str], app_bundles: &[&str]) -> Self {
        Self {
            bundle_ids: bundle_ids.iter().map(|s| s.to_lowercase()).collect(),
            process_names: process_names.iter().map(|s| s.to_lowercase()).collect(),
            app_bundles: app_bundles.iter().map(|s| s.to_lowercase()).collect(),
        }
    }
}

/// `lsappinfo list` prints one `bundleID="…"` per running GUI app.
fn running_bundle_ids() -> HashSet<String> {
    let Ok(output) = Command::new("/usr/bin/lsappinfo").arg("list").output() else {
        return HashSet::new();
    };

    String::from_utf8_lossy(&output.stdout)
        .split("bundleID=\"")
        .skip(1)
        .filter_map(|chunk| chunk.split('"').next())
        .map(|id| id.to_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_name_matches_main_process_and_helpers_only() {
        let running =
            RunningApps::from_parts(&[], &["Code", "Code Helper (Renderer)", "codex"], &[]);
        assert!(running.owns("Code"));

        let running = RunningApps::from_parts(&[], &["codex"], &[]);
        assert!(
            !running.owns("Code"),
            "`codex` must not claim VS Code's folder"
        );
    }

    #[test]
    fn test_bundle_id_and_helper_folders_match_running_gui_app() {
        let running = RunningApps::from_parts(&["com.google.Chrome"], &[], &[]);
        assert!(running.owns("com.google.Chrome"));
        assert!(running.owns("com.google.Chrome.helper"));
        assert!(!running.owns("com.microsoft.edgemac"));
    }

    #[test]
    fn test_daemon_container_matches_its_process_name() {
        let running = RunningApps::from_parts(&[], &["geod"], &[]);
        assert!(running.owns("com.apple.geod"));
    }

    #[test]
    fn test_app_bundle_name_matches_folder() {
        let running = RunningApps::from_parts(&[], &[], &["obsidian"]);
        assert!(running.owns("obsidian"));
    }

    #[test]
    fn test_snapshot_sees_this_test_process() {
        let running = RunningApps::snapshot();
        assert!(!running.process_names.is_empty());
    }
}
