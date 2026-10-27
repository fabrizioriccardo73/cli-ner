//! Caches and leftovers owned by root, which no `Cleaner` may touch (that
//! pipeline never elevates). Here they run only with `--allow-admin`, through
//! the native `sudo` prompt.
//!
//! Deletion is `find <fixed root> -mindepth 1 … -delete`: argv only, no shell,
//! no glob expansion by cli-ner; `-mindepth 1` keeps the root folder itself,
//! and `find` never follows symlinks unless asked to.

use crate::maintenance::traits::{ActionCategory, MaintenanceAction, RiskLevel, SystemCommand};
use std::path::Path;
use std::process::Command;

const FIND_PATH: &str = "/usr/bin/find";
const LPSTAT_PATH: &str = "/usr/bin/lpstat";

const CORESYMBOLICATION_CACHE: &str = "/System/Library/Caches/com.apple.coresymbolicationd";
const SYSTEM_CACHES: &str = "/System/Library/Caches";
const CUPS_CACHE: &str = "/private/var/spool/cups/cache";
const CUPS_TMP: &str = "/private/var/spool/cups/tmp";
const CORES_DIR: &str = "/cores";

fn args(parts: &[&str]) -> Vec<String> {
    parts.iter().map(|s| s.to_string()).collect()
}

/// Root-owned system service caches that macOS regenerates on its own: the
/// symbolication cache and the international text-data caches.
pub struct SystemServiceCachesAction;

impl MaintenanceAction for SystemServiceCachesAction {
    fn name(&self) -> &'static str {
        "Clear System Service Caches"
    }

    fn category(&self) -> ActionCategory {
        ActionCategory::ExtendedCache
    }

    fn description(&self) -> &'static str {
        "Removes the coresymbolicationd cache and the com.apple.IntlDataCache.* files from \
         /System/Library/Caches. Both are rebuilt automatically (the text-data cache at the \
         next login, so a restart afterwards is recommended)."
    }

    fn risk(&self) -> RiskLevel {
        RiskLevel::Low
    }

    fn requires_admin(&self) -> bool {
        true
    }

    fn is_reversible(&self) -> bool {
        true
    }

    fn is_available(&self) -> bool {
        Path::new(FIND_PATH).is_file() && Path::new(SYSTEM_CACHES).is_dir()
    }

    fn commands(&self) -> Vec<SystemCommand> {
        let mut commands = Vec::new();
        if Path::new(CORESYMBOLICATION_CACHE).is_dir() {
            commands.push(SystemCommand::new(
                FIND_PATH,
                args(&[CORESYMBOLICATION_CACHE, "-mindepth", "1", "-delete"]),
            ));
        }
        commands.push(SystemCommand::new(
            FIND_PATH,
            args(&[
                SYSTEM_CACHES,
                "-mindepth",
                "1",
                "-maxdepth",
                "1",
                "-type",
                "f",
                "-name",
                "com.apple.IntlDataCache.*",
                "-delete",
            ]),
        ));
        commands
    }
}

/// The CUPS print spooler's cache and temporary job files (`root:lp`).
pub struct CupsSpoolCacheAction;

impl MaintenanceAction for CupsSpoolCacheAction {
    fn name(&self) -> &'static str {
        "Clear Print Spooler Cache"
    }

    fn category(&self) -> ActionCategory {
        ActionCategory::ExtendedCache
    }

    fn description(&self) -> &'static str {
        "Empties CUPS's cache and tmp spool folders under /private/var/spool/cups. Refuses to \
         run while print jobs are queued, since tmp holds the files of jobs in flight."
    }

    fn risk(&self) -> RiskLevel {
        RiskLevel::Low
    }

    fn requires_admin(&self) -> bool {
        true
    }

    fn is_reversible(&self) -> bool {
        true
    }

    fn is_available(&self) -> bool {
        Path::new(FIND_PATH).is_file() && Path::new("/private/var/spool/cups").is_dir()
    }

    fn preflight(&self) -> Result<(), String> {
        // `lpstat -o` lists every queued job and prints nothing when there are none
        // (or when the scheduler isn't running, which also means no jobs).
        let queued = Command::new(LPSTAT_PATH)
            .arg("-o")
            .output()
            .map(|o| !String::from_utf8_lossy(&o.stdout).trim().is_empty())
            .unwrap_or(false);
        if queued {
            return Err("print jobs are queued — wait for them to finish first".into());
        }
        Ok(())
    }

    fn commands(&self) -> Vec<SystemCommand> {
        [CUPS_CACHE, CUPS_TMP]
            .iter()
            .map(|dir| SystemCommand::new(FIND_PATH, args(&[dir, "-mindepth", "1", "-delete"])))
            .collect()
    }
}

/// Crash core dumps in `/cores`. Each can be gigabytes; they only matter to
/// whoever is debugging the crash that produced them.
pub struct CoreDumpsAction;

impl CoreDumpsAction {
    fn has_core_dumps() -> bool {
        std::fs::read_dir(CORES_DIR)
            .map(|entries| {
                entries
                    .flatten()
                    .any(|e| e.file_name().to_string_lossy().starts_with("core."))
            })
            .unwrap_or(false)
    }
}

impl MaintenanceAction for CoreDumpsAction {
    fn name(&self) -> &'static str {
        "Delete Crash Core Dumps"
    }

    fn category(&self) -> ActionCategory {
        ActionCategory::DiskMaintenance
    }

    fn description(&self) -> &'static str {
        "Deletes core.* files from /cores. Permanent: anyone debugging those crashes loses \
         the dumps. Admin is needed for dumps written by root processes."
    }

    fn risk(&self) -> RiskLevel {
        RiskLevel::Medium
    }

    fn requires_admin(&self) -> bool {
        true
    }

    fn is_reversible(&self) -> bool {
        false
    }

    fn is_available(&self) -> bool {
        Path::new(FIND_PATH).is_file() && Path::new(CORES_DIR).is_dir()
    }

    fn preflight(&self) -> Result<(), String> {
        if !Self::has_core_dumps() {
            return Err("no core dumps in /cores — nothing to delete".into());
        }
        Ok(())
    }

    fn commands(&self) -> Vec<SystemCommand> {
        vec![SystemCommand::new(
            FIND_PATH,
            args(&[
                CORES_DIR,
                "-mindepth",
                "1",
                "-maxdepth",
                "1",
                "-type",
                "f",
                "-name",
                "core.*",
                "-delete",
            ]),
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn all_commands() -> Vec<SystemCommand> {
        let mut commands = SystemServiceCachesAction.commands();
        commands.extend(CupsSpoolCacheAction.commands());
        commands.extend(CoreDumpsAction.commands());
        commands
    }

    /// Every privileged deletion must start at one of a handful of fixed roots,
    /// keep that root (`-mindepth 1`), and never follow links (`-L`/`-H`).
    #[test]
    fn test_privileged_deletes_are_fixed_and_keep_their_root() {
        let allowed_roots = [
            CORESYMBOLICATION_CACHE,
            SYSTEM_CACHES,
            CUPS_CACHE,
            CUPS_TMP,
            CORES_DIR,
        ];
        for command in all_commands() {
            assert_eq!(command.program, FIND_PATH);
            assert!(allowed_roots.contains(&command.args[0].as_str()));
            let mindepth = command.args.iter().position(|a| a == "-mindepth").unwrap();
            assert_eq!(command.args[mindepth + 1], "1");
            assert_eq!(command.args.last().unwrap(), "-delete");
            assert!(!command.args.iter().any(|a| a == "-L" || a == "-H"));
        }
    }

    #[test]
    fn test_all_privileged_actions_require_admin() {
        assert!(SystemServiceCachesAction.requires_admin());
        assert!(CupsSpoolCacheAction.requires_admin());
        assert!(CoreDumpsAction.requires_admin());
    }

    #[test]
    fn test_core_dumps_are_irreversible_and_need_confirmation() {
        assert!(!CoreDumpsAction.is_reversible());
        assert!(CoreDumpsAction.risk().requires_confirmation());
    }

    #[test]
    fn test_core_dump_preflight_matches_presence_of_dumps() {
        assert_eq!(
            CoreDumpsAction.preflight().is_ok(),
            CoreDumpsAction::has_core_dumps()
        );
    }
}
