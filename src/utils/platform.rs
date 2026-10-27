use sysinfo::{Disks, System};

/// Information about system disks.
#[derive(Debug, Clone)]
pub struct DiskStats {
    pub name: String,
    pub mount_point: String,
    pub total_space: u64,
    pub available_space: u64,
    pub used_space: u64,
}

/// Retrieve disk statistics for all mounted disks.
pub fn get_disk_stats() -> Vec<DiskStats> {
    let disks = Disks::new_with_refreshed_list();
    disks
        .iter()
        .map(|d| {
            let total = d.total_space();
            let available = d.available_space();
            DiskStats {
                name: d.name().to_string_lossy().to_string(),
                mount_point: d.mount_point().to_string_lossy().to_string(),
                total_space: total,
                available_space: available,
                used_space: total.saturating_sub(available),
            }
        })
        .collect()
}

/// Free space (in bytes) actually reported by the volume that holds `path`.
///
/// This is the ground truth used to tell the user how much space a run *really*
/// reclaimed, as opposed to the logical size of the files that were touched.
/// Picks the mount point with the longest match, so `/System/Volumes/Data` wins
/// over `/` for paths inside the data volume.
pub fn available_space_for(path: &std::path::Path) -> Option<u64> {
    let disks = Disks::new_with_refreshed_list();
    disks
        .iter()
        .filter(|d| path.starts_with(d.mount_point()))
        .max_by_key(|d| d.mount_point().as_os_str().len())
        .map(|d| d.available_space())
}

/// Free space on the volume holding the user's home directory.
pub fn available_space_home() -> Option<u64> {
    let home = dirs::home_dir()?;
    available_space_for(&home)
}

/// Check if a process matching the given name is currently running.
pub fn is_process_running(process_name: &str) -> bool {
    let mut sys = System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All);

    let target = process_name.to_lowercase();
    for process in sys.processes().values() {
        let name = process.name().to_string_lossy().to_lowercase();
        if name.contains(&target) {
            return true;
        }
    }
    false
}
