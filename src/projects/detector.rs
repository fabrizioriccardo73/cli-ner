use crate::projects::models::{DiscoveredProject, ProjectArtifact, ProjectType};
use crate::utils::fs::calculate_size;
use chrono::{DateTime, Local};
use std::fs;
use std::path::Path;
use std::time::SystemTime;

/// Strict list of recognized project build/dependency artifact folder names.
pub const ALLOWED_ARTIFACT_NAMES: &[&str] = &[
    "node_modules",
    ".next",
    ".nuxt",
    ".turbo",
    "target",
    ".venv",
    "venv",
    "__pycache__",
    ".pytest_cache",
    ".ruff_cache",
    ".mypy_cache",
    ".gradle",
    "build",
    "vendor",
    ".dart_tool",
];

/// Inspects a directory to determine if it is a project root with cleanable build artifacts.
pub fn detect_project_in_dir(dir: &Path, days_threshold: u64) -> Option<DiscoveredProject> {
    if !dir.is_dir() {
        return None;
    }

    let project_type = detect_project_type(dir)?;
    let artifacts = scan_project_artifacts(dir, project_type);

    if artifacts.is_empty() {
        return None;
    }

    let total_reclaimable_bytes: u64 = artifacts.iter().map(|a| a.size_bytes).sum();
    if total_reclaimable_bytes == 0 {
        return None;
    }

    let last_modified = determine_project_activity(dir);
    let days_inactive = last_modified.map(|lm| {
        let now = Local::now();
        let duration = now.signed_duration_since(lm);
        duration.num_days().max(0) as u64
    });

    let is_dormant = days_inactive.map(|d| d >= days_threshold).unwrap_or(false);

    let name = dir
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| dir.display().to_string());

    Some(DiscoveredProject {
        root_path: dir.to_path_buf(),
        name,
        project_type,
        last_modified,
        days_inactive,
        artifacts,
        total_reclaimable_bytes,
        is_dormant,
    })
}

/// Identifies the software ecosystem of a project directory based on manifest files.
pub fn detect_project_type(dir: &Path) -> Option<ProjectType> {
    if dir.join("Cargo.toml").exists() {
        return Some(ProjectType::Rust);
    }
    if dir.join("package.json").exists() {
        return Some(ProjectType::Node);
    }
    if dir.join("pyproject.toml").exists()
        || dir.join("requirements.txt").exists()
        || dir.join("Pipfile").exists()
        || dir.join("setup.py").exists()
        || dir.join("uv.lock").exists()
        || dir.join("poetry.lock").exists()
    {
        return Some(ProjectType::Python);
    }
    if dir.join("build.gradle").exists()
        || dir.join("build.gradle.kts").exists()
        || dir.join("pom.xml").exists()
        || dir.join("settings.gradle").exists()
        || dir.join("settings.gradle.kts").exists()
    {
        return Some(ProjectType::Gradle);
    }
    if dir.join("pubspec.yaml").exists() {
        return Some(ProjectType::Flutter);
    }
    if dir.join("composer.json").exists() {
        return Some(ProjectType::Composer);
    }
    if dir.join("go.mod").exists() {
        return Some(ProjectType::Go);
    }

    None
}

/// Discovers cleanable build and dependency artifact directories inside a project.
fn scan_project_artifacts(dir: &Path, project_type: ProjectType) -> Vec<ProjectArtifact> {
    let candidate_names: &[(&str, &str)] = match project_type {
        ProjectType::Rust => &[("target", "Rust build artifacts & compiler cache")],
        ProjectType::Node => &[
            ("node_modules", "Node.js dependencies"),
            (".next", "Next.js build cache"),
            (".nuxt", "Nuxt.js build cache"),
            (".turbo", "Turborepo build cache"),
        ],
        ProjectType::Python => &[
            (".venv", "Python virtual environment"),
            ("venv", "Python virtual environment"),
            ("__pycache__", "Python bytecode cache"),
            (".pytest_cache", "Pytest test cache"),
            (".ruff_cache", "Ruff linter cache"),
            (".mypy_cache", "Mypy type check cache"),
        ],
        ProjectType::Gradle => &[
            (".gradle", "Gradle build cache & daemons"),
            ("build", "Gradle / Java build output"),
            ("target", "Maven build artifacts"),
        ],
        ProjectType::Composer => &[("vendor", "PHP Composer dependencies")],
        ProjectType::Go => &[("vendor", "Go vendored dependencies")],
        ProjectType::Flutter => &[
            (".dart_tool", "Dart tool build metadata"),
            ("build", "Flutter build output"),
        ],
    };

    let mut artifacts = Vec::new();

    for &(name, desc) in candidate_names {
        let artifact_path = dir.join(name);
        if artifact_path.exists() && artifact_path.is_dir() {
            if let Ok((size, count)) = calculate_size(&artifact_path) {
                if size > 0 {
                    artifacts.push(ProjectArtifact {
                        name: name.to_string(),
                        path: artifact_path,
                        size_bytes: size,
                        file_count: count,
                        description: desc.to_string(),
                    });
                }
            }
        }
    }

    // Sort largest artifacts first
    artifacts.sort_by_key(|a| std::cmp::Reverse(a.size_bytes));
    artifacts
}

/// Determines the last time code was actively modified in the project (via Git or source files).
pub fn determine_project_activity(dir: &Path) -> Option<DateTime<Local>> {
    let mut newest_time: Option<SystemTime> = None;

    // 1. Check Git index & HEAD & refs if available
    let git_dir = dir.join(".git");
    if git_dir.is_dir() {
        let git_check_files = [
            git_dir.join("index"),
            git_dir.join("HEAD"),
            git_dir.join("FETCH_HEAD"),
            git_dir.join("refs/heads"),
        ];

        for path in &git_check_files {
            if let Ok(meta) = fs::metadata(path) {
                if let Ok(mtime) = meta.modified() {
                    newest_time = match newest_time {
                        Some(prev) => Some(prev.max(mtime)),
                        None => Some(mtime),
                    };
                }
            }
        }
    }

    // 2. Check root files & source directories (excluding build artifact directories)
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            let file_name = entry.file_name();
            let name_str = file_name.to_string_lossy();

            // Skip known artifact folders and .git for project activity checking
            if ALLOWED_ARTIFACT_NAMES.contains(&name_str.as_ref()) || name_str == ".git" {
                continue;
            }

            if let Ok(meta) = entry.metadata() {
                if let Ok(mtime) = meta.modified() {
                    newest_time = match newest_time {
                        Some(prev) => Some(prev.max(mtime)),
                        None => Some(mtime),
                    };
                }
            }
        }
    }

    newest_time.map(DateTime::from)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn test_detect_rust_project() {
        let temp_dir = std::env::temp_dir().join("test_cli_ner_rust_proj");
        let _ = fs::create_dir_all(&temp_dir);
        let cargo_toml = temp_dir.join("Cargo.toml");
        File::create(&cargo_toml).unwrap();

        let target_dir = temp_dir.join("target");
        let _ = fs::create_dir_all(&target_dir);
        let mut sample = File::create(target_dir.join("test.bin")).unwrap();
        sample.write_all(b"sample build artifact data").unwrap();

        let detected = detect_project_in_dir(&temp_dir, 30);
        assert!(detected.is_some());
        let proj = detected.unwrap();
        assert_eq!(proj.project_type, ProjectType::Rust);
        assert!(!proj.artifacts.is_empty());
        assert_eq!(proj.artifacts[0].name, "target");

        // Clean up
        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_detect_node_project() {
        let temp_dir = std::env::temp_dir().join("test_cli_ner_node_proj");
        let _ = fs::create_dir_all(&temp_dir);
        let pkg_json = temp_dir.join("package.json");
        File::create(&pkg_json).unwrap();

        let nm_dir = temp_dir.join("node_modules");
        let _ = fs::create_dir_all(&nm_dir);
        let mut sample = File::create(nm_dir.join("index.js")).unwrap();
        sample.write_all(b"console.log('test');").unwrap();

        let detected = detect_project_in_dir(&temp_dir, 0);
        assert!(detected.is_some());
        let proj = detected.unwrap();
        assert_eq!(proj.project_type, ProjectType::Node);
        assert_eq!(proj.artifacts[0].name, "node_modules");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_detect_python_project() {
        let temp_dir = std::env::temp_dir().join("test_cli_ner_py_proj");
        let _ = fs::create_dir_all(&temp_dir);
        let pyproject = temp_dir.join("pyproject.toml");
        File::create(&pyproject).unwrap();

        let venv_dir = temp_dir.join(".venv");
        let _ = fs::create_dir_all(&venv_dir);
        let mut sample = File::create(venv_dir.join("pyvenv.cfg")).unwrap();
        sample.write_all(b"home = /usr/bin").unwrap();

        let detected = detect_project_in_dir(&temp_dir, 0);
        assert!(detected.is_some());
        let proj = detected.unwrap();
        assert_eq!(proj.project_type, ProjectType::Python);
        assert_eq!(proj.artifacts[0].name, ".venv");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_detect_gradle_project() {
        let temp_dir = std::env::temp_dir().join("test_cli_ner_gradle_proj");
        let _ = fs::create_dir_all(&temp_dir);
        let build_gradle = temp_dir.join("build.gradle");
        File::create(&build_gradle).unwrap();

        let gradle_dir = temp_dir.join(".gradle");
        let _ = fs::create_dir_all(&gradle_dir);
        let mut sample = File::create(gradle_dir.join("cache.bin")).unwrap();
        sample.write_all(b"gradle-cache").unwrap();

        let detected = detect_project_in_dir(&temp_dir, 0);
        assert!(detected.is_some());
        let proj = detected.unwrap();
        assert_eq!(proj.project_type, ProjectType::Gradle);
        assert_eq!(proj.artifacts[0].name, ".gradle");

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_detect_composer_project() {
        let temp_dir = std::env::temp_dir().join("test_cli_ner_composer_proj");
        let _ = fs::create_dir_all(&temp_dir);
        let composer_json = temp_dir.join("composer.json");
        File::create(&composer_json).unwrap();

        let vendor_dir = temp_dir.join("vendor");
        let _ = fs::create_dir_all(&vendor_dir);
        let mut sample = File::create(vendor_dir.join("autoload.php")).unwrap();
        sample.write_all(b"<?php").unwrap();

        let detected = detect_project_in_dir(&temp_dir, 0);
        assert!(detected.is_some());
        let proj = detected.unwrap();
        assert_eq!(proj.project_type, ProjectType::Composer);
        assert_eq!(proj.artifacts[0].name, "vendor");

        let _ = fs::remove_dir_all(&temp_dir);
    }
}
