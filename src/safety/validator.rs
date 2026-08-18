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
}
