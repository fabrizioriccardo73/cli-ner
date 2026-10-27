use crate::maintenance::disk_maintenance::{
    ApfsVerifyAction, SmartStatusCheckAction, ThinLocalSnapshotsAction,
};
use crate::maintenance::extended_cache::{FontCacheResetAction, QuickLookCacheResetAction};
use crate::maintenance::heavy_rebuild::{LocateDatabaseRebuildAction, SpotlightRebuildAction};
use crate::maintenance::privileged_cache::{
    CoreDumpsAction, CupsSpoolCacheAction, SystemServiceCachesAction,
};
use crate::maintenance::system_index::{LaunchServicesRebuildAction, MailIndexVacuumAction};
use crate::maintenance::system_state::{FlushDnsCacheAction, PurgeMemoryAction};
use crate::maintenance::traits::{ActionCategory, MaintenanceAction};

/// Registry of all available maintenance actions.
///
/// Covers Fasi M1-M5 of `docs/MAINTENANCE_ROADMAP.md`. The M5 rebuilds are
/// registered like the rest but are opt-in (`MaintenanceAction::is_opt_in`).
pub struct MaintenanceRegistry {
    actions: Vec<Box<dyn MaintenanceAction>>,
}

impl MaintenanceRegistry {
    pub fn new() -> Self {
        let actions: Vec<Box<dyn MaintenanceAction>> = vec![
            Box::new(LaunchServicesRebuildAction),
            Box::new(MailIndexVacuumAction),
            Box::new(SmartStatusCheckAction),
            Box::new(ApfsVerifyAction),
            Box::new(ThinLocalSnapshotsAction),
            Box::new(FontCacheResetAction),
            Box::new(QuickLookCacheResetAction),
            Box::new(SystemServiceCachesAction),
            Box::new(CupsSpoolCacheAction),
            Box::new(CoreDumpsAction),
            Box::new(FlushDnsCacheAction),
            Box::new(PurgeMemoryAction),
            Box::new(SpotlightRebuildAction),
            Box::new(LocateDatabaseRebuildAction),
        ];

        Self { actions }
    }

    /// Retrieve all registered actions.
    pub fn all(&self) -> &[Box<dyn MaintenanceAction>] {
        &self.actions
    }

    /// Retrieve actions filtered by category.
    #[allow(dead_code, clippy::borrowed_box)]
    pub fn find_by_category(&self, category: ActionCategory) -> Vec<&Box<dyn MaintenanceAction>> {
        self.actions
            .iter()
            .filter(|a| a.category() == category)
            .collect()
    }
}

impl Default for MaintenanceRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_has_the_fase_m1_system_index_actions() {
        let registry = MaintenanceRegistry::new();
        let names: Vec<&str> = registry.all().iter().map(|a| a.name()).collect();
        assert!(names.contains(&"Rebuild LaunchServices Database"));
        assert!(names.contains(&"Vacuum Mail Envelope Index"));
    }

    #[test]
    fn test_registry_has_the_fase_m2_disk_maintenance_actions() {
        let registry = MaintenanceRegistry::new();
        let names: Vec<&str> = registry.all().iter().map(|a| a.name()).collect();
        assert!(names.contains(&"Check Disk SMART Status"));
        assert!(names.contains(&"Verify APFS Volume (read-only)"));
        assert!(names.contains(&"Thin Local Time Machine Snapshots"));
    }

    #[test]
    fn test_registry_has_the_fase_m3_extended_cache_actions() {
        let registry = MaintenanceRegistry::new();
        let names: Vec<&str> = registry.all().iter().map(|a| a.name()).collect();
        assert!(names.contains(&"Reset User Font Cache"));
        assert!(names.contains(&"Reset Quick Look Cache"));
    }

    #[test]
    fn test_registry_has_the_fase_m4_system_state_actions() {
        let registry = MaintenanceRegistry::new();
        let names: Vec<&str> = registry.all().iter().map(|a| a.name()).collect();
        assert!(names.contains(&"Flush DNS Cache"));
        assert!(names.contains(&"Purge Inactive Memory"));
    }

    #[test]
    fn test_only_the_fase_m5_rebuilds_are_opt_in() {
        let registry = MaintenanceRegistry::new();
        let mut opt_in: Vec<&str> = registry
            .all()
            .iter()
            .filter(|a| a.is_opt_in())
            .map(|a| a.name())
            .collect();
        opt_in.sort();
        assert_eq!(
            opt_in,
            vec!["Rebuild Spotlight Index", "Rebuild locate Database"]
        );
    }

    #[test]
    fn test_find_by_category_filters_correctly() {
        let registry = MaintenanceRegistry::new();
        let system_index = registry.find_by_category(ActionCategory::SystemIndex);
        assert_eq!(system_index.len(), 4);

        let disk_maintenance = registry.find_by_category(ActionCategory::DiskMaintenance);
        assert_eq!(disk_maintenance.len(), 4);

        let extended_cache = registry.find_by_category(ActionCategory::ExtendedCache);
        assert_eq!(extended_cache.len(), 4);

        let system_state = registry.find_by_category(ActionCategory::SystemState);
        assert_eq!(system_state.len(), 2);
    }
}
