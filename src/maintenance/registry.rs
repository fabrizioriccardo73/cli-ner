use crate::maintenance::system_index::{LaunchServicesRebuildAction, MailIndexVacuumAction};
use crate::maintenance::traits::{ActionCategory, MaintenanceAction};

/// Registry of all available maintenance actions.
///
/// Fase M1 in progress (see `docs/MAINTENANCE_ROADMAP.md`): the low-risk
/// Catalog A actions (System Index rebuilds) are in; extended caches, disk
/// maintenance, and system-state actions land category by category next.
pub struct MaintenanceRegistry {
    actions: Vec<Box<dyn MaintenanceAction>>,
}

impl MaintenanceRegistry {
    pub fn new() -> Self {
        let actions: Vec<Box<dyn MaintenanceAction>> = vec![
            Box::new(LaunchServicesRebuildAction),
            Box::new(MailIndexVacuumAction),
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
    fn test_find_by_category_filters_correctly() {
        let registry = MaintenanceRegistry::new();
        let system_index = registry.find_by_category(ActionCategory::SystemIndex);
        assert_eq!(system_index.len(), registry.all().len());

        let disk_maintenance = registry.find_by_category(ActionCategory::DiskMaintenance);
        assert!(disk_maintenance.is_empty());
    }
}
