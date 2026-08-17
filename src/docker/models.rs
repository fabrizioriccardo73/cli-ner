use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerMount {
    pub mount_type: String,       // "volume", "bind"
    pub name: Option<String>,     // Named volume name if applicable
    pub source: String,           // Source path or volume name
    pub destination: String,      // Container destination path
    pub rw: bool,                 // Read/Write
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerContainer {
    pub id: String,
    pub name: String,
    pub image: String,
    pub state: String,            // "running", "exited", "paused", "created"
    pub status: String,           // e.g. "Up 26 hours (healthy)", "Exited (0) 2 days ago"
    pub created_at: String,
    pub size_bytes: u64,
    pub size_str: String,
    pub mounts: Vec<DockerMount>,
    pub is_running: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerImage {
    pub id: String,
    pub repository: String,
    pub tag: String,
    pub size_bytes: u64,
    pub size_str: String,
    pub created_at: String,
    pub created_since: String,
    pub is_dangling: bool,
    pub in_use_by: Vec<String>,   // List of container names referencing this image
}

impl DockerImage {
    pub fn display_name(&self) -> String {
        if self.is_dangling || self.repository == "<none>" {
            format!("<dangling: {}>", &self.id[..self.id.len().min(12)])
        } else {
            format!("{}:{}", self.repository, self.tag)
        }
    }

    pub fn is_in_use(&self) -> bool {
        !self.in_use_by.is_empty()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DockerVolume {
    pub name: String,
    pub driver: String,
    pub scope: String,
    pub size_str: String,
    pub size_bytes: u64,
    pub used_by: Vec<String>,     // Names of containers referencing this volume
}

impl DockerVolume {
    pub fn is_in_use(&self) -> bool {
        !self.used_by.is_empty()
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DockerSystemDf {
    pub images_total: usize,
    pub images_active: usize,
    pub images_size_str: String,
    pub images_reclaimable_str: String,
    pub images_reclaimable_bytes: u64,

    pub containers_total: usize,
    pub containers_active: usize,
    pub containers_size_str: String,
    pub containers_reclaimable_str: String,
    pub containers_reclaimable_bytes: u64,

    pub volumes_total: usize,
    pub volumes_active: usize,
    pub volumes_size_str: String,
    pub volumes_reclaimable_str: String,
    pub volumes_reclaimable_bytes: u64,

    pub build_cache_total: usize,
    pub build_cache_active: usize,
    pub build_cache_size_str: String,
    pub build_cache_reclaimable_str: String,
    pub build_cache_reclaimable_bytes: u64,
}
