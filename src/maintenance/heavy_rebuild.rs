//! Fase M5 — rebuilds with a big, visible cost: hours of Spotlight
//! re-indexing, or a scan of the whole disk for `locate`. Both are opt-in
//! (`is_opt_in`), so they only run with `maintain --include-heavy` and after
//! their own confirmation prompt.

use crate::maintenance::traits::{ActionCategory, MaintenanceAction, RiskLevel, SystemCommand};
use std::path::Path;
use std::process::Command;

const MDUTIL_PATH: &str = "/usr/bin/mdutil";
const LOCATE_UPDATEDB_PATH: &str = "/usr/libexec/locate.updatedb";

/// Reads `mdutil -s /` and reports whether Spotlight indexing is on for the
/// boot volume. `None` if the status can't be read at all.
fn spotlight_indexing_enabled() -> Option<bool> {
    let output = Command::new(MDUTIL_PATH).args(["-s", "/"]).output().ok()?;
    if !output.status.success() {
        return None;
    }
    parse_indexing_status(&String::from_utf8_lossy(&output.stdout))
}

fn parse_indexing_status(status: &str) -> Option<bool> {
    let status = status.to_lowercase();
    if status.contains("indexing enabled") {
        Some(true)
    } else if status.contains("indexing disabled") {
        Some(false)
    } else {
        None
    }
}

/// Erases and rebuilds the Spotlight index of the boot volume.
///
/// The reference tool also deleted `~/Library/Metadata/CoreSpotlight/*` with a
/// shell glob first. That is left out: `mdutil -E` already rebuilds the index,
/// and those folders hold apps' own in-app search data.
pub struct SpotlightRebuildAction;

impl MaintenanceAction for SpotlightRebuildAction {
    fn name(&self) -> &'static str {
        "Rebuild Spotlight Index"
    }

    fn category(&self) -> ActionCategory {
        ActionCategory::SystemIndex
    }

    fn description(&self) -> &'static str {
        "Erases the Spotlight index of the boot volume and rebuilds it from scratch \
         (mdutil -E /). Fixes missing or stale search results. Re-indexing takes from \
         minutes to hours with heavy CPU and disk use, and search is incomplete until it ends."
    }

    fn risk(&self) -> RiskLevel {
        RiskLevel::Medium
    }

    fn requires_admin(&self) -> bool {
        true
    }

    fn is_reversible(&self) -> bool {
        true
    }

    fn is_opt_in(&self) -> bool {
        true
    }

    fn is_available(&self) -> bool {
        Path::new(MDUTIL_PATH).is_file()
    }

    fn preflight(&self) -> Result<(), String> {
        match spotlight_indexing_enabled() {
            Some(true) => Ok(()),
            Some(false) => {
                Err("Spotlight indexing is disabled on / — there is no index to rebuild".into())
            }
            None => Err("couldn't read the Spotlight status of / (mdutil -s /)".into()),
        }
    }

    fn commands(&self) -> Vec<SystemCommand> {
        vec![SystemCommand::new(
            MDUTIL_PATH,
            vec!["-E".to_string(), "/".to_string()],
        )]
    }
}

/// Builds (or rebuilds) the database used by the `locate` command.
///
/// Runs Apple's own `locate.updatedb` as root, which drops to the `nobody` user
/// for the scan and installs the result read-only. So only paths readable by
/// everyone end up in the database. The reference tool instead unloaded and
/// reloaded the `com.apple.locate` LaunchDaemon; that is left out because
/// loading it with `-w` enables the weekly job for good, a lasting system change.
pub struct LocateDatabaseRebuildAction;

impl MaintenanceAction for LocateDatabaseRebuildAction {
    fn name(&self) -> &'static str {
        "Rebuild locate Database"
    }

    fn category(&self) -> ActionCategory {
        ActionCategory::SystemIndex
    }

    fn description(&self) -> &'static str {
        "Rebuilds /var/db/locate.database so the `locate` command finds current files. \
         Scans the whole disk (a few minutes of disk activity) as the unprivileged \
         `nobody` user. Does not turn on the weekly automatic update."
    }

    fn risk(&self) -> RiskLevel {
        RiskLevel::Medium
    }

    fn requires_admin(&self) -> bool {
        true
    }

    fn is_reversible(&self) -> bool {
        true
    }

    fn is_opt_in(&self) -> bool {
        true
    }

    fn is_available(&self) -> bool {
        Path::new(LOCATE_UPDATEDB_PATH).is_file()
    }

    fn commands(&self) -> Vec<SystemCommand> {
        vec![SystemCommand::new(LOCATE_UPDATEDB_PATH, Vec::new())]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_heavy_rebuilds_are_opt_in_admin_and_need_confirmation() {
        for action in [
            &SpotlightRebuildAction as &dyn MaintenanceAction,
            &LocateDatabaseRebuildAction,
        ] {
            assert!(action.is_opt_in());
            assert!(action.requires_admin());
            assert!(action.risk().requires_confirmation());
            assert_eq!(action.category(), ActionCategory::SystemIndex);
        }
    }

    #[test]
    fn test_spotlight_rebuild_targets_only_the_boot_volume() {
        let commands = SpotlightRebuildAction.commands();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].display(), "/usr/bin/mdutil -E /");
        // `-a` would re-index every mounted volume, external drives included.
        assert!(!commands[0].args.iter().any(|a| a == "-a"));
    }

    #[test]
    fn test_locate_rebuild_never_touches_launchctl() {
        let commands = LocateDatabaseRebuildAction.commands();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].program, LOCATE_UPDATEDB_PATH);
        assert!(commands[0].args.is_empty());
    }

    #[test]
    fn test_parse_indexing_status() {
        assert_eq!(
            parse_indexing_status("/:\n\tIndexing enabled. \n"),
            Some(true)
        );
        assert_eq!(
            parse_indexing_status("/:\n\tIndexing disabled.\n"),
            Some(false)
        );
        assert_eq!(parse_indexing_status("Error: unknown volume"), None);
    }

    #[test]
    fn test_spotlight_preflight_matches_indexing_status() {
        let enabled = spotlight_indexing_enabled() == Some(true);
        assert_eq!(SpotlightRebuildAction.preflight().is_ok(), enabled);
    }
}
