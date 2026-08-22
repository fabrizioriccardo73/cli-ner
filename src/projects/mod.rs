pub mod cleaner;
pub mod detector;
pub mod models;
pub mod scanner;
pub mod ui;

pub use cleaner::clean_project_artifacts;
pub use models::ProjectType;
pub use scanner::{scan_projects, ScannerOptions};
pub use ui::{prompt_confirm_clean, render_projects_table, select_artifacts_interactive};
