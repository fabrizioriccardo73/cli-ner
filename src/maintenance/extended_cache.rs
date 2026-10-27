use crate::maintenance::traits::{ActionCategory, MaintenanceAction, RiskLevel, SystemCommand};
use std::path::Path;

const ATSUTIL_PATH: &str = "/usr/bin/atsutil";
const QLMANAGE_PATH: &str = "/usr/bin/qlmanage";

/// Resets the per-user Apple Type Services (ATS) font cache.
///
/// `-removeUser` only touches this account's cache, not the system-wide one
/// (`-remove`, which needs root and is intentionally not offered here). macOS
/// rebuilds it automatically the next time a font is used.
pub struct FontCacheResetAction;

impl MaintenanceAction for FontCacheResetAction {
    fn name(&self) -> &'static str {
        "Reset User Font Cache"
    }

    fn category(&self) -> ActionCategory {
        ActionCategory::ExtendedCache
    }

    fn description(&self) -> &'static str {
        "Clears this user's Apple Type Services (ATS) font cache via `atsutil databases \
         -removeUser`. Fixes font-related glitches (garbled glyphs, missing fonts in menus); \
         macOS rebuilds the cache automatically the next time a font is used."
    }

    fn risk(&self) -> RiskLevel {
        RiskLevel::Low
    }

    fn is_reversible(&self) -> bool {
        true
    }

    fn is_available(&self) -> bool {
        Path::new(ATSUTIL_PATH).is_file()
    }

    fn commands(&self) -> Vec<SystemCommand> {
        vec![SystemCommand::new(
            ATSUTIL_PATH,
            vec!["databases".to_string(), "-removeUser".to_string()],
        )]
    }
}

/// Resets Quick Look's preview-generator cache and restarts its background process.
///
/// Fixes stale or broken preview thumbnails (Space bar in Finder, Gallery view).
/// `qlmanage` regenerates everything on demand, so nothing is permanently lost.
pub struct QuickLookCacheResetAction;

impl MaintenanceAction for QuickLookCacheResetAction {
    fn name(&self) -> &'static str {
        "Reset Quick Look Cache"
    }

    fn category(&self) -> ActionCategory {
        ActionCategory::ExtendedCache
    }

    fn description(&self) -> &'static str {
        "Clears Quick Look's preview-generator cache and restarts `quicklookd` via \
         `qlmanage -r cache` and `qlmanage -r`. Fixes stale or broken file previews; \
         previews are regenerated on demand."
    }

    fn risk(&self) -> RiskLevel {
        RiskLevel::Low
    }

    fn is_reversible(&self) -> bool {
        true
    }

    fn is_available(&self) -> bool {
        Path::new(QLMANAGE_PATH).is_file()
    }

    fn commands(&self) -> Vec<SystemCommand> {
        vec![
            SystemCommand::new(QLMANAGE_PATH, vec!["-r".to_string(), "cache".to_string()]),
            SystemCommand::new(QLMANAGE_PATH, vec!["-r".to_string()]),
        ]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_font_cache_reset_is_user_scoped_and_needs_no_admin() {
        let action = FontCacheResetAction;
        assert!(!action.requires_admin());
        assert_eq!(action.risk(), RiskLevel::Low);

        let rendered = action.commands()[0].display();
        assert!(
            rendered.contains("-removeUser"),
            "must never run the system-wide -remove variant"
        );
    }

    #[test]
    fn test_quick_look_reset_runs_cache_clear_before_restart() {
        let action = QuickLookCacheResetAction;
        let commands = action.commands();
        assert_eq!(commands.len(), 2);
        assert!(commands[0].display().ends_with("-r cache"));
        assert!(commands[1].display().ends_with("-r"));
    }
}
