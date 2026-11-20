use crate::maintenance::traits::{ActionCategory, MaintenanceAction, RiskLevel, SystemCommand};
use crate::utils::fs::expand_tilde;
#[cfg(test)]
use crate::utils::platform::is_process_running;
use crate::utils::platform::is_process_running_exact;
use std::fs;
use std::path::{Path, PathBuf};

const LSREGISTER_PATH: &str = "/System/Library/Frameworks/CoreServices.framework/Versions/A/Frameworks/LaunchServices.framework/Versions/A/Support/lsregister";

/// Cleans up the LaunchServices database: drops stale application records (the
/// duplicates in the "Open With" menu) and re-registers the apps in the local,
/// system and user domains.
///
/// Deliberately `-gc`, never `-delete`: `lsregister -h` says that after
/// `-delete` "You must then reboot!" — until then icons, "Open With" and file
/// associations are broken. `-gc` compacts the live database in place, so
/// nothing needs a restart. No files are deleted, no admin rights are needed.
pub struct LaunchServicesRebuildAction;

impl MaintenanceAction for LaunchServicesRebuildAction {
    fn name(&self) -> &'static str {
        "Rebuild LaunchServices Database"
    }

    fn category(&self) -> ActionCategory {
        ActionCategory::SystemIndex
    }

    fn description(&self) -> &'static str {
        "Removes stale entries (duplicates in the \"Open With\" menu) from the \
         LaunchServices database and re-registers installed apps. The database is \
         compacted in place, never deleted, so no restart is needed."
    }

    fn risk(&self) -> RiskLevel {
        RiskLevel::Low
    }

    fn is_reversible(&self) -> bool {
        true
    }

    fn is_available(&self) -> bool {
        Path::new(LSREGISTER_PATH).is_file()
    }

    fn commands(&self) -> Vec<SystemCommand> {
        vec![SystemCommand::new(
            LSREGISTER_PATH,
            // `-gc`, not `-delete` (which needs a reboot) — see the type's doc comment.
            ["-gc", "-f", "-all", "local,system,user"]
                .iter()
                .map(|s| s.to_string())
                .collect(),
        )]
    }
}

/// Finds Mail.app's envelope index database, whatever its storage-format
/// version directory happens to be called (`V8`, `V9`, `V10`, …) — that name
/// changes across macOS releases, so it is never hardcoded.
///
/// An upgraded Mac can keep an older `V<n>` folder around next to the live one;
/// the highest version number is the one Mail actually uses.
fn find_envelope_index() -> Option<PathBuf> {
    let mail_dir = expand_tilde("~/Library/Mail");
    let entries = fs::read_dir(&mail_dir).ok()?;

    entries
        .flatten()
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().to_string();
            let version: u32 = name.strip_prefix('V')?.parse().ok()?;
            let candidate = entry.path().join("MailData").join("Envelope Index");
            candidate.is_file().then_some((version, candidate))
        })
        .max_by_key(|(version, _)| *version)
        .map(|(_, path)| path)
}

/// Compacts Mail.app's envelope index with a SQLite `VACUUM` to reclaim slack
/// space and fix minor index corruption. Only ever runs while Mail.app is
/// closed: vacuuming a database the app still has open risks corrupting it,
/// which is why `preflight()` checks for the running process first.
pub struct MailIndexVacuumAction;

impl MaintenanceAction for MailIndexVacuumAction {
    fn name(&self) -> &'static str {
        "Vacuum Mail Envelope Index"
    }

    fn category(&self) -> ActionCategory {
        ActionCategory::SystemIndex
    }

    fn description(&self) -> &'static str {
        "Runs a SQLite VACUUM on Mail.app's envelope index database to reclaim slack \
         space and fix minor corruption. Requires Mail.app to be closed."
    }

    fn risk(&self) -> RiskLevel {
        RiskLevel::Low
    }

    fn is_reversible(&self) -> bool {
        true
    }

    fn is_available(&self) -> bool {
        find_envelope_index().is_some()
    }

    fn preflight(&self) -> Result<(), String> {
        // Exact match: a substring check would also hit the `maild` daemon, which
        // is always running, and block this action forever.
        if is_process_running_exact("Mail") {
            return Err(
                "Mail.app is running — close it first so the index file isn't locked.".into(),
            );
        }
        Ok(())
    }

    fn commands(&self) -> Vec<SystemCommand> {
        match find_envelope_index() {
            Some(path) => vec![SystemCommand::new(
                "/usr/bin/sqlite3",
                vec![path.display().to_string(), "vacuum".to_string()],
            )],
            None => Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_launch_services_action_is_self_healing_and_low_risk() {
        let action = LaunchServicesRebuildAction;
        assert_eq!(action.risk(), RiskLevel::Low);
        assert!(action.is_reversible());
        assert!(!action.requires_admin());
    }

    #[test]
    fn test_launch_services_commands_cover_all_three_domains() {
        let action = LaunchServicesRebuildAction;
        let commands = action.commands();
        assert_eq!(commands.len(), 1);

        assert_eq!(commands[0].args, ["-gc", "-f", "-all", "local,system,user"]);
    }

    /// `-delete` throws the database away and needs a reboot before icons and
    /// "Open With" work again; the action must never use it.
    #[test]
    fn test_launch_services_never_deletes_the_database() {
        let commands = LaunchServicesRebuildAction.commands();
        assert!(!commands[0].args.iter().any(|a| a == "-delete"));
    }

    #[test]
    fn test_mail_vacuum_availability_matches_envelope_index_presence() {
        // This machine may or may not have Mail.app configured; assert the two
        // checks the action makes (is_available, commands) stay consistent
        // with each other rather than asserting a specific outcome.
        let action = MailIndexVacuumAction;
        let found = find_envelope_index().is_some();
        assert_eq!(action.is_available(), found);
        assert_eq!(action.commands().is_empty(), !found);
    }

    #[test]
    fn test_mail_vacuum_preflight_matches_running_mail_process() {
        let action = MailIndexVacuumAction;
        let running = is_process_running_exact("Mail");
        assert_eq!(action.preflight().is_err(), running);
    }

    #[test]
    fn test_mail_check_ignores_the_maild_daemon() {
        // `maild` runs on every Mac whether Mail.app is open or not.
        if is_process_running("maild") && !is_process_running_exact("Mail") {
            assert!(MailIndexVacuumAction.preflight().is_ok());
        }
    }
}
