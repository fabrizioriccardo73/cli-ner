use crate::safety::allowlist::{find_allowed_target, AllowedTarget};
use crate::safety::blocklist::is_blocked;
use crate::utils::fs::is_symlink;
use anyhow::{bail, Result};
use std::path::{Path, PathBuf};

#[derive(Debug)]
#[allow(dead_code)]
pub struct ValidationResult {
    pub is_safe: bool,
    pub target_info: Option<&'static AllowedTarget>,
    pub canonical_path: PathBuf,
    pub warning: Option<String>,
}

/// Strict validation of any path before deletion or trashing.
/// Ensures no user data or system integrity is ever compromised.
pub fn validate_path_for_cleaning<P: AsRef<Path>>(path: P) -> Result<ValidationResult> {
    let path = path.as_ref();

    // 1. Check if the path exists
    if !path.exists() && !is_symlink(path) {
        bail!("Path does not exist: {}", path.display());
    }

    // 2. Strict blocklist check
    let (blocked, reason) = is_blocked(path);
    if blocked {
        bail!(
            "CRITICAL SECURITY: Path is blocked: {} (Reason: {})",
            path.display(),
            reason.unwrap_or_default()
        );
    }

    // 3. Allowlist check
    let (allowed_target, root_path) = match find_allowed_target(path) {
        Some((target, root)) => (target, root),
        None => {
            bail!(
                "SECURITY: Path is not in the recognized allowlist: {}",
                path.display()
            );
        }
    };

    // 4. If contents_only is true, forbid deleting the parent root folder itself
    let abs_path = if path.is_relative() {
        std::env::current_dir()
            .map(|cwd| cwd.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    } else {
        path.to_path_buf()
    };

    if allowed_target.contents_only && abs_path == root_path {
        bail!(
            "SECURITY: Cannot delete root folder {}. Only its contents can be cleaned.",
            root_path.display()
        );
    }

    // 5. Check if it's a symlink pointing to an unsafe location
    let mut warning = None;
    if is_symlink(path) {
        warning = Some(format!("Item is a symbolic link: {}", path.display()));
    }

    Ok(ValidationResult {
        is_safe: true,
        target_info: Some(allowed_target),
        canonical_path: abs_path,
        warning,
    })
}

/// Strict validation specifically for project build artifacts before deletion or trashing.
pub fn validate_project_artifact_for_cleaning<P1: AsRef<Path>, P2: AsRef<Path>>(
    artifact_path: P1,
    project_root: P2,
) -> Result<()> {
    let artifact_path = artifact_path.as_ref();
    let project_root = project_root.as_ref();

    // 1. Check existence
    if !artifact_path.exists() && !is_symlink(artifact_path) {
        bail!("Artifact path does not exist: {}", artifact_path.display());
    }

    // 2. Strict blocklist check
    let (blocked, reason) = is_blocked(artifact_path);
    if blocked {
        bail!(
            "SECURITY: Path is blocked: {} ({})",
            artifact_path.display(),
            reason.unwrap_or_default()
        );
    }

    // 3. Prevent deleting project root itself
    let abs_artifact = if artifact_path.is_relative() {
        std::env::current_dir()
            .map(|cwd| cwd.join(artifact_path))
            .unwrap_or_else(|_| artifact_path.to_path_buf())
    } else {
        artifact_path.to_path_buf()
    };

    let abs_root = if project_root.is_relative() {
        std::env::current_dir()
            .map(|cwd| cwd.join(project_root))
            .unwrap_or_else(|_| project_root.to_path_buf())
    } else {
        project_root.to_path_buf()
    };

    if abs_artifact == abs_root {
        bail!(
            "SECURITY: Cannot delete project root folder itself: {}",
            project_root.display()
        );
    }

    if !abs_artifact.starts_with(&abs_root) {
        bail!(
            "SECURITY: Artifact {} is not inside project root {}",
            artifact_path.display(),
            project_root.display()
        );
    }

    // 4. Artifact directory name must be an allowed artifact name
    let dir_name = artifact_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();

    if !crate::projects::detector::ALLOWED_ARTIFACT_NAMES.contains(&dir_name) {
        bail!(
            "SECURITY: '{}' is not an authorized project build artifact name",
            dir_name
        );
    }

    // 5. Must NOT be .git or contain .git
    if dir_name == ".git" || abs_artifact.join(".git").exists() {
        bail!(
            "SECURITY: Refusing to delete .git directory at {}",
            artifact_path.display()
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_reject_system_path() {
        let res = validate_path_for_cleaning("/System/Library");
        assert!(res.is_err());
    }

    #[test]
    fn test_reject_user_home_root() {
        if let Some(home) = dirs::home_dir() {
            let res = validate_path_for_cleaning(&home);
            assert!(res.is_err());
        }
    }

    #[test]
    fn test_reject_unlisted_path() {
        if let Some(home) = dirs::home_dir() {
            let doc_file = home.join("Documents");
            let res = validate_path_for_cleaning(&doc_file);
            assert!(res.is_err());
        }
    }

    #[test]
    fn test_reject_root_cache_folder_itself() {
        if let Some(home) = dirs::home_dir() {
            let cache_dir = home.join("Library/Caches");
            if cache_dir.exists() {
                let res = validate_path_for_cleaning(&cache_dir);
                assert!(res.is_err());
            }
        }
    }

    #[test]
    fn test_allow_cache_subfolder() {
        if let Some(home) = dirs::home_dir() {
            let sample_cache = home.join("Library/Caches/test-sample-dir");
            std::fs::create_dir_all(&sample_cache).ok();
            if sample_cache.exists() {
                let res = validate_path_for_cleaning(&sample_cache);
                assert!(res.is_ok());
                let val = res.unwrap();
                assert!(val.is_safe);
                std::fs::remove_dir(&sample_cache).ok();
            }
        }
    }

    #[test]
    fn test_validate_project_artifact_safe() {
        let temp_proj = std::env::temp_dir().join("test_safe_proj");
        let nm = temp_proj.join("node_modules");
        std::fs::create_dir_all(&nm).unwrap();

        let res = validate_project_artifact_for_cleaning(&nm, &temp_proj);
        assert!(res.is_ok());

        std::fs::remove_dir_all(&temp_proj).ok();
    }

    #[test]
    fn test_validate_project_artifact_reject_root_itself() {
        let temp_proj = std::env::temp_dir().join("test_proj_root_reject");
        std::fs::create_dir_all(&temp_proj).unwrap();

        let res = validate_project_artifact_for_cleaning(&temp_proj, &temp_proj);
        assert!(res.is_err());

        std::fs::remove_dir_all(&temp_proj).ok();
    }

    #[test]
    fn test_validate_project_artifact_reject_unauthorized_folder() {
        let temp_proj = std::env::temp_dir().join("test_proj_src_reject");
        let src = temp_proj.join("src");
        std::fs::create_dir_all(&src).unwrap();

        let res = validate_project_artifact_for_cleaning(&src, &temp_proj);
        assert!(res.is_err());

        std::fs::remove_dir_all(&temp_proj).ok();
    }
}
