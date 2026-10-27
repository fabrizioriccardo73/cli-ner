use anyhow::{Context, Result};
use rayon::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};
use walkdir::WalkDir;

/// Get actual physical allocated disk space in bytes for a file or directory metadata.
/// On Unix/macOS, this uses `st_blocks * 512` which reflects the real APFS/ext4 disk allocation,
/// properly accounting for sparse files, dataless/cloud stubs (e.g. OneDrive/iCloud Files On-Demand),
/// and inline files.
#[inline]
pub fn get_allocated_size(meta: &fs::Metadata) -> u64 {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        meta.blocks() * 512
    }
    #[cfg(not(unix))]
    {
        meta.len()
    }
}

/// Walk a single subtree serially and accumulate allocated size + file count.
///
/// Never follows symlinks: a symlink contributes only its own (tiny) allocation,
/// so a link pointing outside the tree can never inflate the reported size.
fn walk_subtree(path: &Path) -> (u64, usize) {
    let mut total_size = 0u64;
    let mut total_files = 0usize;

    for entry in WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        if entry.file_type().is_file() || entry.file_type().is_symlink() {
            if let Ok(meta) = entry.metadata() {
                total_size += get_allocated_size(&meta);
                total_files += 1;
            }
        }
    }

    (total_size, total_files)
}

/// Calculate total physical allocated size and file count in a directory or file
/// without following symlinks.
///
/// The measurement is **complete**: no directory is silently skipped, so the number
/// reported to the user always matches what would actually be removed. The top-level
/// entries are walked in parallel, which keeps large trees (`~/Library`, `node_modules`,
/// DerivedData) fast without trading away accuracy.
pub fn calculate_size<P: AsRef<Path>>(path: P) -> Result<(u64, usize)> {
    let path = path.as_ref();

    // symlink_metadata first: a symlink to a missing target must still be measurable.
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(_) => return Ok((0, 0)),
    };

    if !meta.is_dir() {
        // Regular file, symlink, socket, fifo, device: counts as exactly one entry.
        return Ok((get_allocated_size(&meta), 1));
    }

    // Fan the immediate children out across the thread pool; each child subtree is
    // then walked serially. Depth-1 fan-out is enough to saturate cores on the trees
    // this tool cares about, and avoids the overhead of a fully parallel walker.
    let children: Vec<PathBuf> = match fs::read_dir(path) {
        Ok(entries) => entries.flatten().map(|e| e.path()).collect(),
        Err(_) => return Ok((0, 0)),
    };

    let (total_size, total_files) = children
        .par_iter()
        .map(|child| match fs::symlink_metadata(child) {
            Ok(meta) if meta.is_dir() => walk_subtree(child),
            Ok(meta) => (get_allocated_size(&meta), 1),
            Err(_) => (0, 0),
        })
        .reduce(|| (0u64, 0usize), |a, b| (a.0 + b.0, a.1 + b.1));

    Ok((total_size, total_files))
}

/// Check if a file or directory is older than a specified duration based on last modification.
///
/// Returns `false` when the timestamp cannot be read, so an unreadable entry is treated
/// as recent and therefore left in place.
///
/// Only looks at the entry itself: for "is anything inside still in use", see
/// `cleaner::temp_files::is_idle_temp_entry`.
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

    // Try the plain name first, then timestamped variants with a counter. Every
    // attempt is a rename that refuses to replace an existing entry, so two items
    // with the same name (every Electron app's `GPUCache/index`, say) can never
    // overwrite one another in the Trash — even within the same millisecond.
    let timestamp = chrono::Local::now().format("%Y%m%d_%H%M%S_%3f").to_string();
    let mut last_err = None;
    for attempt in 0..1000u32 {
        let candidate = match attempt {
            0 => trash_dir.join(file_name.as_ref()),
            1 => trash_dir.join(format!("{}_{}", file_name, timestamp)),
            n => trash_dir.join(format!("{}_{}_{}", file_name, timestamp, n)),
        };

        match rename_no_replace(path, &candidate) {
            Ok(()) => return Ok(candidate),
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(e) => {
                last_err = Some((e, candidate));
                break;
            }
        }
    }

    let Some((rename_err, dest_path)) = last_err else {
        anyhow::bail!(
            "Could not find a free name in ~/.Trash for {}",
            path.display()
        );
    };

    // Fallback for cross-device filesystems.
    if path.is_dir() {
        // The system Trash picks its own name here (and keeps "Put Back"), so
        // only the Trash folder itself can be reported.
        trash::delete(path).with_context(|| {
            format!(
                "Failed to rename to trash ({}) and fallback failed",
                rename_err
            )
        })?;
        Ok(trash_dir)
    } else {
        // `create_new` refuses to clobber an entry that appeared in the meantime.
        let copy_res = fs::File::open(path).and_then(|mut src| {
            let mut dst = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&dest_path)?;
            std::io::copy(&mut src, &mut dst)?;
            dst.sync_all()
        });
        copy_res
            .and_then(|_| fs::remove_file(path))
            .with_context(|| format!("Failed to move file to trash: {}", rename_err))?;
        Ok(dest_path)
    }
}

/// `rename(2)` that fails with `AlreadyExists` instead of replacing `to`.
///
/// On macOS this is `renamex_np(…, RENAME_EXCL)`, which checks and renames in
/// one atomic step. Elsewhere it falls back to check-then-rename.
#[cfg(target_os = "macos")]
fn rename_no_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    use std::ffi::CString;
    use std::os::raw::{c_char, c_int, c_uint};
    use std::os::unix::ffi::OsStrExt;

    extern "C" {
        fn renamex_np(from: *const c_char, to: *const c_char, flags: c_uint) -> c_int;
    }
    const RENAME_EXCL: c_uint = 0x0000_0004;

    let invalid = |_| std::io::Error::from(std::io::ErrorKind::InvalidInput);
    let from_c = CString::new(from.as_os_str().as_bytes()).map_err(invalid)?;
    let to_c = CString::new(to.as_os_str().as_bytes()).map_err(invalid)?;

    // SAFETY: both pointers come from live, NUL-terminated CStrings.
    let rc = unsafe { renamex_np(from_c.as_ptr(), to_c.as_ptr(), RENAME_EXCL) };
    if rc == 0 {
        Ok(())
    } else {
        Err(std::io::Error::last_os_error())
    }
}

#[cfg(not(target_os = "macos"))]
fn rename_no_replace(from: &Path, to: &Path) -> std::io::Result<()> {
    if fs::symlink_metadata(to).is_ok() {
        return Err(std::io::Error::from(std::io::ErrorKind::AlreadyExists));
    }
    fs::rename(from, to)
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

    #[test]
    fn test_calculate_size_and_allocation() {
        let temp_dir = std::env::temp_dir().join("test_cli_ner_calc_size");
        let _ = fs::create_dir_all(&temp_dir);
        let test_file = temp_dir.join("sample.txt");
        {
            let mut f = File::create(&test_file).unwrap();
            f.write_all(b"Hello world from cli-ner").unwrap();
        }

        let meta = fs::symlink_metadata(&test_file).unwrap();
        let alloc_size = get_allocated_size(&meta);
        assert!(alloc_size > 0);

        let (dir_size, file_count) = calculate_size(&temp_dir).unwrap();
        assert_eq!(file_count, 1);
        assert_eq!(dir_size, alloc_size);

        let _ = fs::remove_dir_all(&temp_dir);
    }

    /// `calculate_size` used to prune `.git`, `Containers`, `Group Containers` and
    /// `CoreSimulator` from every walk, so it under-reported the size of anything that
    /// contained them — including targets it would then delete in full.
    #[test]
    fn test_calculate_size_counts_previously_pruned_directories() {
        let root = std::env::temp_dir().join("test_cli_ner_no_pruning");
        let _ = fs::remove_dir_all(&root);

        let mut expected_files = 0usize;
        for hidden in [".git", "Containers", "Group Containers", "CoreSimulator"] {
            let dir = root.join("payload").join(hidden);
            fs::create_dir_all(&dir).unwrap();
            let mut f = File::create(dir.join("blob.bin")).unwrap();
            f.write_all(&vec![b'x'; 4096]).unwrap();
            expected_files += 1;
        }

        let (size, count) = calculate_size(&root).unwrap();
        assert_eq!(
            count, expected_files,
            "every file must be counted, none silently skipped"
        );
        assert!(size > 0, "sizes inside these directories must be measured");

        let _ = fs::remove_dir_all(&root);
    }

    /// Moving into the Trash must never replace an item already there.
    #[test]
    fn test_rename_no_replace_refuses_to_overwrite() {
        let root = std::env::temp_dir().join("test_cli_ner_rename_excl");
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(&root).unwrap();

        let incoming = root.join("index");
        let existing = root.join("index-already-trashed");
        fs::write(&incoming, b"new").unwrap();
        fs::write(&existing, b"keep me").unwrap();

        let err = rename_no_replace(&incoming, &existing).unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(&existing).unwrap(), b"keep me");
        assert!(incoming.exists(), "the source must stay put when refused");

        let free = root.join("index-free");
        rename_no_replace(&incoming, &free).unwrap();
        assert_eq!(fs::read(&free).unwrap(), b"new");

        let _ = fs::remove_dir_all(&root);
    }

    /// A symlink must contribute its own tiny allocation, never the size of its target.
    #[test]
    fn test_calculate_size_does_not_follow_symlinks() {
        let root = std::env::temp_dir().join("test_cli_ner_symlink_size");
        let _ = fs::remove_dir_all(&root);

        let real = root.join("real");
        fs::create_dir_all(&real).unwrap();
        let mut f = File::create(real.join("big.bin")).unwrap();
        f.write_all(&vec![b'x'; 200_000]).unwrap();

        let linked = root.join("linked");
        fs::create_dir_all(&linked).unwrap();
        std::os::unix::fs::symlink(&real, linked.join("pointer")).unwrap();

        let (size, count) = calculate_size(&linked).unwrap();
        assert_eq!(count, 1, "the symlink itself is the only entry");
        assert!(
            size < 100_000,
            "symlink must not be credited with its target's 200 KB, got {size}"
        );

        let _ = fs::remove_dir_all(&root);
    }
}
