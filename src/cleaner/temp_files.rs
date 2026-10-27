use crate::cleaner::traits::{CleanTargetItem, Cleaner};
use crate::safety::allowlist::CleanCategory;
use crate::utils::fs::calculate_size;
use anyhow::Result;
use std::fs;
use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

/// Temp entries with any activity more recent than this are left alone.
///
/// `/private/tmp` and `/private/var/tmp` hold scratch files, sockets and lock files
/// belonging to processes that are running right now; removing those mid-flight breaks
/// live applications. Anything untouched for a day is safely abandoned work.
const MIN_AGE_BEFORE_CLEANING: Duration = Duration::from_secs(24 * 60 * 60);

/// Whether a top-level temp entry is safe to clean: nothing anywhere inside it
/// changed within `threshold`, and it holds no live endpoint.
///
/// Checking only the entry's own timestamp is not enough: a folder's mtime does
/// not move when a process writes into one of its subfolders, so a build
/// running for days in `/tmp/job/out/…` would look abandoned. Every entry in the
/// subtree is checked instead, using the later of mtime and ctime — ctime cannot
/// be backdated, so a freshly extracted archive with old mtimes still counts as new.
///
/// Sockets and `.lock`/`.pid` files mark a server that may still be running
/// (ssh-agent, tmux, databases, IDE helpers): their timestamps only record when
/// the server started, so the whole entry is kept. Anything unreadable is kept too.
fn is_idle_temp_entry(path: &Path, threshold: Duration, now: SystemTime) -> bool {
    let Some(cutoff) = now
        .checked_sub(threshold)
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
    else {
        return false;
    };
    let cutoff = cutoff.as_secs() as i64;

    for entry in WalkDir::new(path).follow_links(false) {
        let Ok(entry) = entry else {
            return false;
        };
        let Ok(meta) = entry.path().symlink_metadata() else {
            return false;
        };

        if meta.file_type().is_socket() {
            return false;
        }
        let name = entry.file_name().to_string_lossy();
        if name.ends_with(".lock") || name.ends_with(".pid") {
            return false;
        }
        if meta.mtime().max(meta.ctime()) >= cutoff {
            return false;
        }
    }

    true
}

pub struct TempFilesCleaner;

impl Cleaner for TempFilesCleaner {
    fn name(&self) -> &'static str {
        "Temporary System & App Files"
    }

    fn category(&self) -> CleanCategory {
        CleanCategory::TempFiles
    }

    fn description(&self) -> &'static str {
        "Cleans temporary files and folders in /private/tmp and /private/var/tmp in which nothing \
         changed for at least 24 hours; anything holding a socket or a lock/pid file is left alone"
    }

    fn scan(&self) -> Result<Vec<CleanTargetItem>> {
        let temp_dirs = ["/private/tmp", "/private/var/tmp"];
        let mut items = Vec::new();
        let now = SystemTime::now();

        for dir_str in &temp_dirs {
            let dir = Path::new(dir_str);
            if dir.is_dir() {
                if let Ok(entries) = fs::read_dir(dir) {
                    for entry in entries.filter_map(|e| e.ok()) {
                        let path = entry.path();

                        // Leave anything a running process may still be using.
                        if !is_idle_temp_entry(&path, MIN_AGE_BEFORE_CLEANING, now) {
                            continue;
                        }

                        if let Ok((size, count)) = calculate_size(&path) {
                            if size > 0 {
                                let name = entry.file_name().to_string_lossy().to_string();
                                items.push(CleanTargetItem {
                                    path,
                                    size_bytes: size,
                                    file_count: count,
                                    description: format!("Temp file/folder (idle >24h): {}", name),
                                });
                            }
                        }
                    }
                }
            }
        }

        items.sort_by_key(|b| std::cmp::Reverse(b.size_bytes));
        Ok(items)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: Duration = Duration::from_secs(24 * 60 * 60);

    fn scratch(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("cli-ner-tmp-idle-{tag}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// "now" two days ahead makes everything created by the test look old.
    fn in_two_days() -> SystemTime {
        SystemTime::now() + 2 * DAY
    }

    #[test]
    fn test_untouched_tree_is_idle() {
        let dir = scratch("old");
        fs::create_dir_all(dir.join("a/b")).unwrap();
        fs::write(dir.join("a/b/file.bin"), b"x").unwrap();

        assert!(is_idle_temp_entry(&dir, DAY, in_two_days()));
        let _ = fs::remove_dir_all(&dir);
    }

    /// Fresh activity deep inside a folder keeps the whole folder, even though the
    /// folder's own mtime did not change.
    #[test]
    fn test_recent_write_deep_inside_keeps_the_folder() {
        let dir = scratch("recent");
        fs::create_dir_all(dir.join("a/b")).unwrap();
        fs::write(dir.join("a/b/file.bin"), b"x").unwrap();

        assert!(!is_idle_temp_entry(&dir, DAY, SystemTime::now()));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_folder_with_a_socket_is_never_idle() {
        let dir = scratch("socket");
        let _listener = std::os::unix::net::UnixListener::bind(dir.join("agent.sock")).unwrap();

        assert!(!is_idle_temp_entry(&dir, DAY, in_two_days()));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn test_lock_and_pid_files_are_never_idle() {
        for name in ["mysql.sock.lock", "server.pid"] {
            let dir = scratch(name);
            fs::write(dir.join(name), b"12345").unwrap();
            assert!(!is_idle_temp_entry(&dir, DAY, in_two_days()), "{name}");
            let _ = fs::remove_dir_all(&dir);
        }
    }
}
