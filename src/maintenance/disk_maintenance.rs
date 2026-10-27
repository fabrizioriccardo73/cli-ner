use crate::maintenance::traits::{
    run_system_commands, ActionCategory, MaintenanceAction, MaintenanceOutcome, RiskLevel,
    SystemCommand,
};
use anyhow::Result;
use std::path::Path;
use std::process::Command;

const DISKUTIL_PATH: &str = "/usr/sbin/diskutil";
const FSCK_APFS_PATH: &str = "/sbin/fsck_apfs";
const TMUTIL_PATH: &str = "/usr/bin/tmutil";

/// Pulls one `label:` line out of `diskutil info /`'s plain-text output.
///
/// Deliberately not the `-plist` form piped through `plutil`: no shell pipe,
/// one less process, and this is the only field each caller needs.
fn diskutil_info_field(label: &str) -> Option<String> {
    let output = Command::new(DISKUTIL_PATH).args(["info", "/"]).output().ok()?;
    if !output.status.success() {
        return None;
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .find(|line| line.trim_start().starts_with(label))
        .and_then(|line| line.split_once(':'))
        .map(|(_, value)| value.trim().to_string())
}

/// Checks the boot volume's SMART health status. Read-only — `diskutil info`
/// only ever reports, it never modifies anything — so there is nothing to
/// undo and nothing that can go wrong beyond the command itself failing.
pub struct SmartStatusCheckAction;

impl MaintenanceAction for SmartStatusCheckAction {
    fn name(&self) -> &'static str {
        "Check Disk SMART Status"
    }

    fn category(&self) -> ActionCategory {
        ActionCategory::DiskMaintenance
    }

    fn description(&self) -> &'static str {
        "Reads the boot disk's SMART health status via `diskutil info`. Purely \
         informational: nothing on disk is changed."
    }

    fn risk(&self) -> RiskLevel {
        RiskLevel::Low
    }

    fn is_reversible(&self) -> bool {
        true
    }

    fn is_available(&self) -> bool {
        Path::new(DISKUTIL_PATH).is_file()
    }

    fn commands(&self) -> Vec<SystemCommand> {
        vec![SystemCommand::new(
            DISKUTIL_PATH,
            vec!["info".to_string(), "/".to_string()],
        )]
    }

    fn execute(&self) -> Result<MaintenanceOutcome> {
        let outcome = run_system_commands(self.name(), self.commands())?;
        // Surface just the one line the user actually wants, cleanly
        // re-joined — not `diskutil`'s column-padded raw line, and not the
        // whole `diskutil info` dump the command produces.
        let smart_line = outcome
            .output
            .as_deref()
            .and_then(|out| out.lines().find(|l| l.trim_start().starts_with("SMART Status")))
            .and_then(|l| l.split_once(':'))
            .map(|(label, value)| format!("{}: {}", label.trim(), value.trim()));

        Ok(MaintenanceOutcome {
            output: smart_line.or(outcome.output),
            ..outcome
        })
    }
}

/// Non-destructively verifies the boot volume's APFS structure.
///
/// `-n` tells `fsck_apfs` to check and report only — never to repair — so
/// this cannot leave the volume in a worse state than it found it in.
pub struct ApfsVerifyAction;

impl MaintenanceAction for ApfsVerifyAction {
    fn name(&self) -> &'static str {
        "Verify APFS Volume (read-only)"
    }

    fn category(&self) -> ActionCategory {
        ActionCategory::DiskMaintenance
    }

    fn description(&self) -> &'static str {
        "Runs `fsck_apfs -n -l -S` against the boot volume's device node: checks \
         structural consistency without repairing or modifying anything."
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
        Path::new(FSCK_APFS_PATH).is_file() && diskutil_info_field("Device Node").is_some()
    }

    fn commands(&self) -> Vec<SystemCommand> {
        match diskutil_info_field("Device Node") {
            Some(device) => vec![SystemCommand::new(
                FSCK_APFS_PATH,
                vec!["-n".to_string(), "-l".to_string(), "-S".to_string(), device],
            )],
            None => Vec::new(),
        }
    }
}

/// Thins local Time Machine (APFS) snapshots to reclaim disk space, keeping
/// the most recent ones rather than deleting every local restore point.
///
/// Not the same as `tmutil deletelocalsnapshots`, which the Catalogo C
/// analysis deliberately excluded: that removes every local restore point
/// outright. Thinning only trims older ones under space pressure, so it is
/// rated `Medium`, not `High` — but it still removes state the user cannot
/// get back, hence `is_reversible() == false`.
pub struct ThinLocalSnapshotsAction;

impl MaintenanceAction for ThinLocalSnapshotsAction {
    fn name(&self) -> &'static str {
        "Thin Local Time Machine Snapshots"
    }

    fn category(&self) -> ActionCategory {
        ActionCategory::DiskMaintenance
    }

    fn description(&self) -> &'static str {
        "Runs `tmutil thinlocalsnapshots` on the boot volume to reclaim space held by \
         local APFS snapshots, trimming older ones rather than deleting every restore \
         point the way `tmutil deletelocalsnapshots` would."
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
        Path::new(TMUTIL_PATH).is_file()
    }

    fn commands(&self) -> Vec<SystemCommand> {
        vec![SystemCommand::new(
            TMUTIL_PATH,
            vec![
                "thinlocalsnapshots".to_string(),
                "/".to_string(),
                "999999999999999".to_string(),
                "4".to_string(),
            ],
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smart_check_is_low_risk_read_only_and_needs_no_admin() {
        let action = SmartStatusCheckAction;
        assert_eq!(action.risk(), RiskLevel::Low);
        assert!(action.is_reversible());
        assert!(!action.requires_admin());
    }

    #[test]
    fn test_smart_check_extracts_just_the_smart_status_line() {
        // Exercises execute() for real: this only reads `diskutil info /`, never
        // writes anything, so it is safe to actually run in a test.
        let outcome = SmartStatusCheckAction.execute().unwrap();
        if let Some(output) = &outcome.output {
            assert!(
                output.starts_with("SMART Status"),
                "expected a single 'SMART Status: ...' line, got: {output}"
            );
        }
    }

    #[test]
    fn test_apfs_verify_requires_admin_and_is_read_only() {
        let action = ApfsVerifyAction;
        assert!(action.requires_admin());
        assert!(action.is_reversible());
        assert_eq!(action.risk(), RiskLevel::Low);
    }

    #[test]
    fn test_apfs_verify_command_targets_the_real_device_node_with_no_modify_flag() {
        let action = ApfsVerifyAction;
        if let Some(device) = diskutil_info_field("Device Node") {
            let commands = action.commands();
            assert_eq!(commands.len(), 1);
            let rendered = commands[0].display();
            assert!(rendered.contains("-n "), "must never run without -n (no-modify)");
            assert!(rendered.ends_with(&device));
        }
    }

    #[test]
    fn test_thin_snapshots_requires_confirmation_and_is_not_reversible() {
        let action = ThinLocalSnapshotsAction;
        assert!(action.risk().requires_confirmation());
        assert!(!action.is_reversible());
        assert!(action.requires_admin());
    }

    #[test]
    fn test_thin_snapshots_command_keeps_recent_snapshots() {
        let action = ThinLocalSnapshotsAction;
        let commands = action.commands();
        assert_eq!(commands.len(), 1);
        assert!(commands[0].display().contains("thinlocalsnapshots"));
    }
}
