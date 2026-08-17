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

/// Check if a process matching the given name is currently running.
pub fn is_process_running(process_name: &str) -> bool {
    let mut sys = System::new();
    sys.refresh_processes(sysinfo::ProcessesToUpdate::All);
    
    let target = process_name.to_lowercase();
    for (_pid, process) in sys.processes() {
        let name = process.name().to_string_lossy().to_lowercase();
        if name.contains(&target) {
            return true;
        }
    }
    false
}
