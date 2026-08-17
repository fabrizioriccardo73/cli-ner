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

/// Silently moves a file or directory into macOS Trash (~/.Trash) via POSIX rename without invoking GUI sound effects.
pub fn move_to_trash_silent<P: AsRef<Path>>(path: P) -> Result<PathBuf> {
    let path = path.as_ref();
    let trash_dir = expand_tilde("~/.Trash");
    if !trash_dir.exists() {
        fs::create_dir_all(&trash_dir).context("Failed to create ~/.Trash directory")?;
    }

    let file_name = path
        .file_name()
        .context("Invalid target path file name")?
        .to_string_lossy();

    let mut dest_path = trash_dir.join(file_name.as_ref());

    // If destination already exists in Trash, add a timestamp suffix to prevent collisions
    if dest_path.exists() {
        let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S_%3f");
        let unique_name = format!("{}_{}", file_name, timestamp);
        dest_path = trash_dir.join(unique_name);
    }

    // Direct POSIX rename (atomic and 100% silent on the same volume)
    match fs::rename(path, &dest_path) {
        Ok(_) => Ok(dest_path),
        Err(rename_err) => {
            // Fallback for cross-device filesystems: copy + remove
            if path.is_dir() {
                trash::delete(path).with_context(|| {
                    format!(
                        "Failed to rename to trash ({}) and fallback failed",
                        rename_err
                    )
                })?;
                Ok(dest_path)
            } else {
                fs::copy(path, &dest_path)
                    .and_then(|_| fs::remove_file(path))
                    .with_context(|| {
                        format!("Failed to move file to trash: {}", rename_err)
                    })?;
                Ok(dest_path)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;

    #[test]
    fn test_move_to_trash_silent() {
        let temp_dir = expand_tilde("~/.cli-ner/test_scratch");
        let _ = fs::create_dir_all(&temp_dir);
        let test_file = temp_dir.join("test_silent_trash.txt");
        {
            let mut f = File::create(&test_file).unwrap();
            writeln!(f, "test content").unwrap();
        }
        assert!(test_file.exists());

        let moved_path = move_to_trash_silent(&test_file).expect("Silent trash should succeed");
        assert!(!test_file.exists());
        assert!(moved_path.exists());

        // Cleanup test file from trash
        let _ = fs::remove_file(&moved_path);
        let _ = fs::remove_dir_all(&temp_dir);
    }
}
