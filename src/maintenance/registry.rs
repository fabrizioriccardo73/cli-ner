use crate::maintenance::traits::{ActionCategory, MaintenanceAction};

/// Registry of all available maintenance actions.
///
/// Empty as of Fase M0 (infrastructure only — see `docs/MAINTENANCE_ROADMAP.md`).
/// Concrete actions land category by category starting with Fase M1; this
/// registry, the `maintain` command, and the safety gating around it are meant
/// to need no further changes when that happens.
pub struct MaintenanceRegistry {
    actions: Vec<Box<dyn MaintenanceAction>>,
}

impl MaintenanceRegistry {
    pub fn new() -> Self {
        let actions: Vec<Box<dyn MaintenanceAction>> = vec![
            // Fase M1+: rebuild/reindex, extended caches, disk maintenance, and
            // system-state actions are added here one at a time.
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
    fn test_registry_starts_empty_in_fase_m0() {
        let registry = MaintenanceRegistry::new();
        assert!(registry.all().is_empty());
    }
}
