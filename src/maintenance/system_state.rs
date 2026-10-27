//! Catalog D — system state. None of these free disk space: they reset
//! in-memory state the OS rebuilds on its own (DNS cache, inactive memory).
//!
//! Left out on purpose (see `docs/MAINTENANCE_ROADMAP.md`): NVRAM boot-args
//! (high risk, touches boot settings) and BSD process accounting (off-topic).

use crate::maintenance::traits::{ActionCategory, MaintenanceAction, RiskLevel, SystemCommand};
use std::path::Path;

const DSCACHEUTIL_PATH: &str = "/usr/bin/dscacheutil";
const KILLALL_PATH: &str = "/usr/bin/killall";
const PURGE_PATH: &str = "/usr/sbin/purge";

/// Flushes the Directory Services cache and tells mDNSResponder to drop its
/// DNS cache. Both refill on the next lookup.
pub struct FlushDnsCacheAction;

impl MaintenanceAction for FlushDnsCacheAction {
    fn name(&self) -> &'static str {
        "Flush DNS Cache"
    }

    fn category(&self) -> ActionCategory {
        ActionCategory::SystemState
    }

    fn description(&self) -> &'static str {
        "Clears the DNS cache (dscacheutil + a HUP to mDNSResponder). Useful after changing \
         DNS servers or /etc/hosts, or when a site resolves to a stale address. Frees no disk \
         space; the cache refills on the next lookup."
    }

    fn risk(&self) -> RiskLevel {
        RiskLevel::Low
    }

    /// Signalling mDNSResponder (owned by `_mdnsresponder`) needs root.
    fn requires_admin(&self) -> bool {
        true
    }

    fn is_reversible(&self) -> bool {
        true
    }

    fn is_available(&self) -> bool {
        Path::new(DSCACHEUTIL_PATH).is_file() && Path::new(KILLALL_PATH).is_file()
    }

    fn commands(&self) -> Vec<SystemCommand> {
        vec![
            SystemCommand::new(DSCACHEUTIL_PATH, vec!["-flushcache".to_string()]),
            SystemCommand::new(
                KILLALL_PATH,
                vec!["-HUP".to_string(), "mDNSResponder".to_string()],
            ),
        ]
    }
}

/// Forces the kernel to flush inactive memory and the disk buffer cache.
pub struct PurgeMemoryAction;

impl MaintenanceAction for PurgeMemoryAction {
    fn name(&self) -> &'static str {
        "Purge Inactive Memory"
    }

    fn category(&self) -> ActionCategory {
        ActionCategory::SystemState
    }

    fn description(&self) -> &'static str {
        "Runs /usr/sbin/purge to flush inactive memory and the disk buffer cache. Frees RAM, \
         not disk space, and apps may feel slower for a moment while their caches refill. \
         macOS manages memory well on its own, so this is rarely needed."
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
        Path::new(PURGE_PATH).is_file()
    }

    fn commands(&self) -> Vec<SystemCommand> {
        vec![SystemCommand::new(PURGE_PATH, Vec::new())]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_system_state_actions_need_admin_and_are_low_risk() {
        for action in [
            &FlushDnsCacheAction as &dyn MaintenanceAction,
            &PurgeMemoryAction,
        ] {
            assert_eq!(action.category(), ActionCategory::SystemState);
            assert_eq!(action.risk(), RiskLevel::Low);
            assert!(action.requires_admin());
            assert!(action.is_reversible());
        }
    }

    #[test]
    fn test_flush_dns_flushes_then_signals_mdnsresponder() {
        let rendered: Vec<String> = FlushDnsCacheAction
            .commands()
            .iter()
            .map(|c| c.display())
            .collect();
        assert_eq!(
            rendered,
            vec![
                "/usr/bin/dscacheutil -flushcache",
                "/usr/bin/killall -HUP mDNSResponder",
            ]
        );
    }

    #[test]
    fn test_purge_takes_no_arguments() {
        let commands = PurgeMemoryAction.commands();
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].program, PURGE_PATH);
        assert!(commands[0].args.is_empty());
    }
}
