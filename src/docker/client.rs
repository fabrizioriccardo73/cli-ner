use crate::docker::models::{
    DockerContainer, DockerImage, DockerMount, DockerSystemDf, DockerVolume,
};
use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
use std::process::Command;

pub struct DockerClient;

#[derive(Debug, Deserialize)]
struct RawDockerPsItem {
    #[serde(rename = "ID")]
    id: Option<String>,
    #[serde(rename = "Names")]
    names: Option<String>,
    #[serde(rename = "Image")]
    image: Option<String>,
    #[serde(rename = "State")]
    state: Option<String>,
    #[serde(rename = "Status")]
    status: Option<String>,
    #[serde(rename = "CreatedAt")]
    created_at: Option<String>,
    #[serde(rename = "Size")]
    size: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawDockerImageItem {
    #[serde(rename = "ID")]
    id: Option<String>,
    #[serde(rename = "Repository")]
    repository: Option<String>,
    #[serde(rename = "Tag")]
    tag: Option<String>,
    #[serde(rename = "Size")]
    size: Option<String>,
    #[serde(rename = "CreatedAt")]
    created_at: Option<String>,
    #[serde(rename = "CreatedSince")]
    created_since: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawDockerVolumeItem {
    #[serde(rename = "Name")]
    name: Option<String>,
    #[serde(rename = "Driver")]
    driver: Option<String>,
    #[serde(rename = "Scope")]
    scope: Option<String>,
    #[serde(rename = "Size")]
    size: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawInspectMount {
    #[serde(rename = "Type")]
    mount_type: Option<String>,
    #[serde(rename = "Name")]
    name: Option<String>,
    #[serde(rename = "Source")]
    source: Option<String>,
    #[serde(rename = "Destination")]
    destination: Option<String>,
    #[serde(rename = "RW")]
    rw: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[allow(dead_code)]
struct RawInspectContainer {
    #[serde(rename = "Id")]
    id: Option<String>,
    #[serde(rename = "Mounts")]
    mounts: Option<Vec<RawInspectMount>>,
    #[serde(rename = "Image")]
    image_id: Option<String>,
}

#[derive(Debug, Deserialize)]
struct RawDfItem {
    #[serde(rename = "Type")]
    type_name: Option<String>,
    #[serde(rename = "TotalCount")]
    total_count: Option<String>,
    #[serde(rename = "Active")]
    active_count: Option<String>,
    #[serde(rename = "Size")]
    size: Option<String>,
    #[serde(rename = "Reclaimable")]
    reclaimable: Option<String>,
}

impl DockerClient {
    /// Checks if Docker CLI is present and the Docker daemon is responding.
    pub fn is_available() -> bool {
        Command::new("docker")
            .args(["info"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    /// Fetches overall docker system df statistics
    pub fn get_system_df() -> Result<DockerSystemDf> {
        let output = Command::new("docker")
            .args(["system", "df", "--format", "{{json .}}"])
            .output()
            .context("Failed to execute `docker system df`")?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Docker system df failed: {}", err);
        }

        let mut summary = DockerSystemDf::default();
        let stdout = String::from_utf8_lossy(&output.stdout);

        for line in stdout.lines() {
            if let Ok(item) = serde_json::from_str::<RawDfItem>(line) {
                let t_name = item.type_name.unwrap_or_default();
                let total: usize = item.total_count.unwrap_or_default().parse().unwrap_or(0);
                let active: usize = item.active_count.unwrap_or_default().parse().unwrap_or(0);
                let size_str = item.size.unwrap_or_else(|| "0B".into());
                let rec_str = item.reclaimable.unwrap_or_else(|| "0B".into());
                let rec_bytes = Self::parse_size_str(&rec_str);

                if t_name.contains("Images") {
                    summary.images_total = total;
                    summary.images_active = active;
                    summary.images_size_str = size_str;
                    summary.images_reclaimable_str = rec_str;
                    summary.images_reclaimable_bytes = rec_bytes;
                } else if t_name.contains("Containers") {
                    summary.containers_total = total;
                    summary.containers_active = active;
                    summary.containers_size_str = size_str;
                    summary.containers_reclaimable_str = rec_str;
                    summary.containers_reclaimable_bytes = rec_bytes;
                } else if t_name.contains("Local Volumes") || t_name.contains("Volumes") {
                    summary.volumes_total = total;
                    summary.volumes_active = active;
                    summary.volumes_size_str = size_str;
                    summary.volumes_reclaimable_str = rec_str;
                    summary.volumes_reclaimable_bytes = rec_bytes;
                } else if t_name.contains("Build Cache") {
                    summary.build_cache_total = total;
                    summary.build_cache_active = active;
                    summary.build_cache_size_str = size_str;
                    summary.build_cache_reclaimable_str = rec_str;
                    summary.build_cache_reclaimable_bytes = rec_bytes;
                }
            }
        }

        Ok(summary)
    }

    /// Fetches all containers with their state, size, and attached volumes/mounts
    pub fn list_containers() -> Result<Vec<DockerContainer>> {
        let output = Command::new("docker")
            .args(["ps", "-a", "--size", "--format", "{{json .}}"])
            .output()
            .context("Failed to execute `docker ps -a`")?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Docker ps failed: {}", err);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut raw_containers = Vec::new();
        let mut container_ids = Vec::new();

        for line in stdout.lines() {
            if let Ok(item) = serde_json::from_str::<RawDockerPsItem>(line) {
                if let Some(ref id) = item.id {
                    container_ids.push(id.clone());
                }
                raw_containers.push(item);
            }
        }

        // Fetch detailed inspect data for mounts
        let mut mounts_map: HashMap<String, Vec<DockerMount>> = HashMap::new();

        if !container_ids.is_empty() {
            let mut inspect_cmd = Command::new("docker");
            inspect_cmd.arg("inspect");
            for id in &container_ids {
                inspect_cmd.arg(id);
            }
            if let Ok(inspect_output) = inspect_cmd.output() {
                if inspect_output.status.success() {
                    if let Ok(inspect_list) =
                        serde_json::from_slice::<Vec<RawInspectContainer>>(&inspect_output.stdout)
                    {
                        for c in inspect_list {
                            if let Some(id) = c.id {
                                let mounts = c
                                    .mounts
                                    .unwrap_or_default()
                                    .into_iter()
                                    .map(|m| DockerMount {
                                        mount_type: m.mount_type.unwrap_or_else(|| "volume".into()),
                                        name: m.name,
                                        source: m.source.unwrap_or_default(),
                                        destination: m.destination.unwrap_or_default(),
                                        rw: m.rw.unwrap_or(true),
                                    })
                                    .collect();
                                mounts_map.insert(id, mounts);
                            }
                        }
                    }
                }
            }
        }

        let mut containers = Vec::new();
        for item in raw_containers {
            let id = item.id.unwrap_or_default();
            let name = item.names.unwrap_or_else(|| "unnamed".into());
            let image = item.image.unwrap_or_default();
            let state = item.state.unwrap_or_default().to_lowercase();
            let is_running = state == "running";
            let status = item.status.unwrap_or_default();
            let created_at = item.created_at.unwrap_or_default();
            let size_str = item.size.unwrap_or_else(|| "0B".into());
            let size_bytes = Self::parse_size_str(&size_str);

            // Find matching mounts by ID (check both short and long ID)
            let mounts = mounts_map
                .iter()
                .find(|(k, _)| k.starts_with(&id) || id.starts_with(*k))
                .map(|(_, m)| m.clone())
                .unwrap_or_default();

            containers.push(DockerContainer {
                id,
                name,
                image,
                state,
                status,
                created_at,
                size_bytes,
                size_str,
                mounts,
                is_running,
            });
        }

        Ok(containers)
    }

    /// Fetches all images and cross-references with existing containers to find in-use status
    pub fn list_images() -> Result<Vec<DockerImage>> {
        let containers = Self::list_containers().unwrap_or_default();

        let output = Command::new("docker")
            .args(["images", "-a", "--format", "{{json .}}"])
            .output()
            .context("Failed to execute `docker images`")?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Docker images failed: {}", err);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut images = Vec::new();
        let mut seen_ids = HashSet::new();

        for line in stdout.lines() {
            if let Ok(item) = serde_json::from_str::<RawDockerImageItem>(line) {
                let id = item.id.unwrap_or_default();
                let repository = item.repository.unwrap_or_else(|| "<none>".into());
                let tag = item.tag.unwrap_or_else(|| "<none>".into());
                let size_str = item.size.unwrap_or_else(|| "0B".into());
                let size_bytes = Self::parse_size_str(&size_str);
                let created_at = item.created_at.unwrap_or_default();
                let created_since = item.created_since.unwrap_or_default();
                let is_dangling = repository == "<none>" || tag == "<none>";

                let full_ref = format!("{}:{}", repository, tag);

                // Determine if in use by any container
                let mut in_use_by = Vec::new();
                for c in &containers {
                    let c_img = &c.image;
                    if c_img == &repository
                        || c_img == &full_ref
                        || c_img.starts_with(&id)
                        || id.starts_with(c_img)
                    {
                        in_use_by.push(c.name.clone());
                    }
                }

                let dedupe_key = format!("{}:{}:{}", id, repository, tag);
                if seen_ids.insert(dedupe_key) {
                    images.push(DockerImage {
                        id,
                        repository,
                        tag,
                        size_bytes,
                        size_str,
                        created_at,
                        created_since,
                        is_dangling,
                        in_use_by,
                    });
                }
            }
        }

        Ok(images)
    }

    /// Fetches all Docker volumes and checks which containers are using them
    pub fn list_volumes() -> Result<Vec<DockerVolume>> {
        let containers = Self::list_containers().unwrap_or_default();

        let output = Command::new("docker")
            .args(["volume", "ls", "--format", "{{json .}}"])
            .output()
            .context("Failed to execute `docker volume ls`")?;

        if !output.status.success() {
            let err = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Docker volume ls failed: {}", err);
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut volumes = Vec::new();

        for line in stdout.lines() {
            if let Ok(item) = serde_json::from_str::<RawDockerVolumeItem>(line) {
                let name = item.name.unwrap_or_default();
                let driver = item.driver.unwrap_or_else(|| "local".into());
                let scope = item.scope.unwrap_or_else(|| "local".into());
                let size_str = item.size.unwrap_or_else(|| "N/A".into());
                let size_bytes = Self::parse_size_str(&size_str);

                // Find which containers use this volume
                let mut used_by = Vec::new();
                for c in &containers {
                    for m in &c.mounts {
                        if let Some(ref m_name) = m.name {
                            if m_name == &name {
                                used_by.push(c.name.clone());
                            }
                        } else if m.source.contains(&name) {
                            used_by.push(c.name.clone());
                        }
                    }
                }

                volumes.push(DockerVolume {
                    name,
                    driver,
                    scope,
                    size_str,
                    size_bytes,
                    used_by,
                });
            }
        }

        Ok(volumes)
    }

    /// Removes stopped containers by IDs.
    /// Safety guarantee: will NOT remove running containers.
    pub fn remove_containers(ids: &[String]) -> Result<Vec<String>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut cmd = Command::new("docker");
        cmd.arg("rm");
        for id in ids {
            cmd.arg(id);
        }

        let output = cmd.output().context("Failed to execute `docker rm`")?;
        if output.status.success() {
            let removed = String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            Ok(removed)
        } else {
            let err = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to remove containers: {}", err);
        }
    }

    /// Removes images by IDs or tags.
    pub fn remove_images(ids_or_tags: &[String]) -> Result<Vec<String>> {
        if ids_or_tags.is_empty() {
            return Ok(Vec::new());
        }

        let mut cmd = Command::new("docker");
        cmd.arg("rmi");
        for item in ids_or_tags {
            cmd.arg(item);
        }

        let output = cmd.output().context("Failed to execute `docker rmi`")?;
        if output.status.success() {
            let removed = String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(|s| s.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
            Ok(removed)
        } else {
            let err = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Failed to remove images: {}", err);
        }
    }

    /// Prunes dangling images (`docker image prune -f`)
    pub fn prune_dangling_images() -> Result<String> {
        let output = Command::new("docker")
            .args(["image", "prune", "-f"])
            .output()
            .context("Failed to execute `docker image prune -f`")?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            let err = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Docker image prune failed: {}", err);
        }
    }

    /// Prunes stopped containers (`docker container prune -f`)
    #[allow(dead_code)]
    pub fn prune_stopped_containers() -> Result<String> {
        let output = Command::new("docker")
            .args(["container", "prune", "-f"])
            .output()
            .context("Failed to execute `docker container prune -f`")?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            let err = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Docker container prune failed: {}", err);
        }
    }

    /// Prunes build cache (`docker builder prune -f`)
    pub fn prune_build_cache() -> Result<String> {
        let output = Command::new("docker")
            .args(["builder", "prune", "-f"])
            .output()
            .context("Failed to execute `docker builder prune -f`")?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
        } else {
            let err = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("Docker builder prune failed: {}", err);
        }
    }

    /// Parses docker formatted size strings (e.g. "1.84GB", "257kB", "10.09GB (71%)", etc.)
    pub fn parse_size_str(s: &str) -> u64 {
        let main_part = s.split_whitespace().next().unwrap_or(s).trim();
        if main_part.is_empty() {
            return 0;
        }
        let lower = main_part.to_lowercase();
        let num_str: String = lower
            .chars()
            .filter(|c| c.is_ascii_digit() || *c == '.')
            .collect();
        let num: f64 = num_str.parse().unwrap_or(0.0);
        if lower.ends_with("tb") || lower.ends_with("tib") {
            (num * 1024.0 * 1024.0 * 1024.0 * 1024.0) as u64
        } else if lower.ends_with("gb") || lower.ends_with("gib") {
            (num * 1024.0 * 1024.0 * 1024.0) as u64
        } else if lower.ends_with("mb") || lower.ends_with("mib") {
            (num * 1024.0 * 1024.0) as u64
        } else if lower.ends_with("kb") || lower.ends_with("kib") || lower.ends_with('k') {
            (num * 1024.0) as u64
        } else {
            num as u64
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_docker_size() {
        assert_eq!(DockerClient::parse_size_str("1.84GB"), (1.84 * 1024.0 * 1024.0 * 1024.0) as u64);
        assert_eq!(DockerClient::parse_size_str("257kB"), (257.0 * 1024.0) as u64);
        assert_eq!(DockerClient::parse_size_str("10.09GB (71%)"), (10.09 * 1024.0 * 1024.0 * 1024.0) as u64);
        assert_eq!(DockerClient::parse_size_str("626MB"), (626.0 * 1024.0 * 1024.0) as u64);
        assert_eq!(DockerClient::parse_size_str("0B"), 0);
        assert_eq!(DockerClient::parse_size_str(""), 0);
    }
}

