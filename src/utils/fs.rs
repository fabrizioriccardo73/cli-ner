use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use walkdir::WalkDir;

/// Calculate total size and file count in a directory or file without following symlinks.
pub fn calculate_size<P: AsRef<Path>>(path: P) -> Result<(u64, usize)> {
    let path = path.as_ref();
    if !path.exists() {
        return Ok((0, 0));
    }

    if path.is_file() || path.is_symlink() {
        let meta = fs::symlink_metadata(path).context("Failed to read metadata")?;
        return Ok((meta.len(), 1));
    }

    let mut total_size = 0u64;
    let mut total_files = 0usize;

    for entry in WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if let Ok(meta) = entry.metadata() {
            if meta.is_file() {
                total_size += meta.len();
                total_files += 1;
            }
        }
    }

    Ok((total_size, total_files))
}

/// Check if a file or directory is older than a specified duration based on last modification.
#[allow(dead_code)]
pub fn is_older_than<P: AsRef<Path>>(path: P, threshold: Duration) -> bool {
    let path = path.as_ref();
    if let Ok(meta) = fs::symlink_metadata(path) {
        if let Ok(modified) = meta.modified() {
            if let Ok(elapsed) = SystemTime::now().duration_since(modified) {
                return elapsed >= threshold;
            }
        }
    }
    false
}

/// Check if a path is a symbolic link.
pub fn is_symlink<P: AsRef<Path>>(path: P) -> bool {
    fs::symlink_metadata(path.as_ref())
        .map(|m| m.file_type().is_symlink())
        .unwrap_or(false)
}

/// Resolve user home directory relative paths (e.g., `~/Library/Caches` -> `/Users/username/Library/Caches`)
pub fn expand_tilde<P: AsRef<Path>>(path: P) -> PathBuf {
    let path = path.as_ref();
    if let Ok(stripped) = path.strip_prefix("~") {
        if let Some(home) = dirs::home_dir() {
            return home.join(stripped);
        }
    }
    path.to_path_buf()
}

/// Format path by replacing home directory with `~` for user friendly display.
pub fn contract_tilde<P: AsRef<Path>>(path: P) -> String {
    let path = path.as_ref();
    if let Some(home) = dirs::home_dir() {
        if let Ok(stripped) = path.strip_prefix(&home) {
            return format!("~/{}", stripped.display());
        }
    }
    path.display().to_string()
}
