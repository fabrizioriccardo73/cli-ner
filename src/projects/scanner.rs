use crate::projects::detector::detect_project_in_dir;
use crate::projects::models::{DiscoveredProject, ProjectType};
use crate::utils::fs::expand_tilde;
use anyhow::Result;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;
use walkdir::WalkDir;

/// Standard developer directories checked by default when no path is provided.
const COMMON_DEV_DIRS: &[&str] = &[
    "~/development",
    "~/Projects",
    "~/code",
    "~/src",
    "~/Workspace",
    "~/dev",
    "~/Documents/Projects",
    "~/Documents/GitHub",
    "~/Documents/Development",
];

/// Directories to skip immediately during directory traversal to optimize speed.
const SKIP_DIR_NAMES: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    ".gradle",
    "vendor",
    ".dart_tool",
    ".venv",
    "venv",
    "__pycache__",
    ".pytest_cache",
    ".ruff_cache",
    ".mypy_cache",
    ".Trash",
    "Library",
    "System",
    "Applications",
    "Movies",
    "Music",
    "Pictures",
    ".cache",
    ".npm",
    ".cargo",
    ".rustup",
    ".docker",
    ".local",
];

pub struct ScannerOptions {
    pub base_path: Option<PathBuf>,
    pub days_threshold: u64,
    pub project_type_filter: Option<ProjectType>,
    pub min_size_bytes: u64,
    pub include_all: bool,
}

/// Identifies the initial search root directories.
pub fn get_search_roots(specified_path: Option<PathBuf>) -> Vec<PathBuf> {
    if let Some(path) = specified_path {
        return vec![expand_tilde(path)];
    }

    // Check for common developer workspaces
    let mut detected_roots = Vec::new();
    for &pattern in COMMON_DEV_DIRS {
        let expanded = expand_tilde(pattern);
        if expanded.is_dir() {
            detected_roots.push(expanded);
        }
    }

    if !detected_roots.is_empty() {
        return detected_roots;
    }

    // Fallback: Current working directory
    if let Ok(cwd) = std::env::current_dir() {
        vec![cwd]
    } else {
        vec![dirs::home_dir().unwrap_or_else(|| PathBuf::from("."))]
    }
}

/// Recursively scans search roots for software projects with cleanable artifacts.
pub fn scan_projects(options: &ScannerOptions) -> Result<Vec<DiscoveredProject>> {
    let search_roots = get_search_roots(options.base_path.clone());
    let mut discovered: Vec<DiscoveredProject> = Vec::new();

    let spinner = ProgressBar::new_spinner();
    spinner.set_style(
        ProgressStyle::default_spinner()
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏")
            .template("{spinner:.green} Scanning for software projects in {msg}...")?,
    );
    spinner.enable_steady_tick(std::time::Duration::from_millis(80));

    for root in &search_roots {
        spinner.set_message(root.display().to_string());

        let walker = WalkDir::new(root)
            .follow_links(false)
            .max_depth(7)
            .into_iter()
            .filter_entry(|e| {
                let file_name = e.file_name().to_string_lossy();
                if e.file_type().is_dir() {
                    // Do not enter into artifact directories or blacklisted folders
                    if SKIP_DIR_NAMES.contains(&file_name.as_ref()) {
                        return false;
                    }
                }
                true
            });

        for entry in walker.flatten() {
            if !entry.file_type().is_dir() {
                continue;
            }

            let path = entry.path();
            if let Some(project) = detect_project_in_dir(path, options.days_threshold) {
                // Apply project type filter if specified
                if let Some(type_filter) = options.project_type_filter {
                    if project.project_type != type_filter {
                        continue;
                    }
                }

                // Apply min size filter
                if project.total_reclaimable_bytes < options.min_size_bytes {
                    continue;
                }

                // Apply dormancy filter unless include_all is requested
                if !options.include_all && !project.is_dormant {
                    continue;
                }

                discovered.push(project);
            }
        }
    }

    spinner.finish_and_clear();

    // Sort by largest reclaimable space first
    discovered.sort_by_key(|p| std::cmp::Reverse(p.total_reclaimable_bytes));

    Ok(discovered)
}
