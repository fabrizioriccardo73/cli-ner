use crate::cleaner::traits::{CleanResult, CleanTargetItem, Cleaner, ExecutionItemResult};
use crate::report::operation_log::{ActionStatus, ActionType};
use crate::safety::allowlist::CleanCategory;
use crate::utils::fs::{calculate_size, expand_tilde};
use anyhow::Result;
use std::process::Command;

pub struct HomebrewCleaner;

impl Cleaner for HomebrewCleaner {
    fn name(&self) -> &'static str {
        "Homebrew Cache & Unused Packages"
    }

    fn category(&self) -> CleanCategory {
        CleanCategory::Homebrew
    }

    fn description(&self) -> &'static str {
        "Runs brew cleanup and brew autoremove to free cached packages and unused dependencies"
    }

    fn is_available(&self) -> bool {
        Command::new("which")
            .arg("brew")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn scan(&self) -> Result<Vec<CleanTargetItem>> {
        if !self.is_available() {
            return Ok(Vec::new());
        }

        let cache_dir = expand_tilde("~/Library/Caches/Homebrew");
        let (size, count) = calculate_size(&cache_dir).unwrap_or((0, 0));

        if size > 0 {
            Ok(vec![CleanTargetItem {
                path: cache_dir,
                size_bytes: size,
                file_count: count,
                description: "Homebrew download cache and stale locks".into(),
            }])
        } else {
            Ok(Vec::new())
        }
    }

    fn clean(&self, dry_run: bool, _force_permanent: bool) -> Result<CleanResult> {
        let targets = self.scan()?;
        let total_size = targets.iter().map(|t| t.size_bytes).sum();
        let mut result = CleanResult::default();

        if dry_run {
            result.total_bytes_freed = total_size;
            result.items_cleaned = targets.len();
            result.details.push(ExecutionItemResult {
                path: "brew cleanup -s && brew autoremove".into(),
                size_bytes: total_size,
                action: ActionType::DryRun,
                status: ActionStatus::Success,
            });
            return Ok(result);
        }

        let cleanup_status = Command::new("brew").args(["cleanup", "-s"]).status();
        let autoremove_status = Command::new("brew").arg("autoremove").status();

        match (cleanup_status, autoremove_status) {
            (Ok(c), Ok(a)) if c.success() && a.success() => {
                result.total_bytes_freed = total_size;
                result.items_cleaned = 1;
                result.details.push(ExecutionItemResult {
                    path: "brew cleanup & autoremove".into(),
                    size_bytes: total_size,
                    action: ActionType::ExternalCommand,
                    status: ActionStatus::Success,
                });
            }
            (Err(e), _) | (_, Err(e)) => {
                result.items_failed = 1;
                result.details.push(ExecutionItemResult {
                    path: "brew cleanup".into(),
                    size_bytes: 0,
                    action: ActionType::ExternalCommand,
                    status: ActionStatus::Failed(e.to_string()),
                });
            }
            _ => {
                result.items_failed = 1;
                result.details.push(ExecutionItemResult {
                    path: "brew cleanup".into(),
                    size_bytes: 0,
                    action: ActionType::ExternalCommand,
                    status: ActionStatus::Failed("brew cleanup returned non-zero exit code".into()),
                });
            }
        }

        Ok(result)
    }
}

pub struct NpmCacheCleaner;

impl Cleaner for NpmCacheCleaner {
    fn name(&self) -> &'static str {
        "NPM Global Cache"
    }

    fn category(&self) -> CleanCategory {
        CleanCategory::Npm
    }

    fn description(&self) -> &'static str {
        "Cleans global npm cache in ~/.npm/_cacache"
    }

    fn is_available(&self) -> bool {
        Command::new("which")
            .arg("npm")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn scan(&self) -> Result<Vec<CleanTargetItem>> {
        let npm_cache = expand_tilde("~/.npm/_cacache");
        let (size, count) = calculate_size(&npm_cache).unwrap_or((0, 0));

        if size > 0 {
            Ok(vec![CleanTargetItem {
                path: npm_cache,
                size_bytes: size,
                file_count: count,
                description: "NPM cached packages".into(),
            }])
        } else {
            Ok(Vec::new())
        }
    }

    fn clean(&self, dry_run: bool, _force_permanent: bool) -> Result<CleanResult> {
        let targets = self.scan()?;
        let total_size = targets.iter().map(|t| t.size_bytes).sum();
        let mut result = CleanResult::default();

        if dry_run {
            result.total_bytes_freed = total_size;
            result.items_cleaned = targets.len();
            result.details.push(ExecutionItemResult {
                path: "npm cache clean --force".into(),
                size_bytes: total_size,
                action: ActionType::DryRun,
                status: ActionStatus::Success,
            });
            return Ok(result);
        }

        match Command::new("npm")
            .args(["cache", "clean", "--force"])
            .status()
        {
            Ok(s) if s.success() => {
                result.total_bytes_freed = total_size;
                result.items_cleaned = 1;
                result.details.push(ExecutionItemResult {
                    path: "npm cache clean --force".into(),
                    size_bytes: total_size,
                    action: ActionType::ExternalCommand,
                    status: ActionStatus::Success,
                });
            }
            Ok(_) => {
                result.items_failed = 1;
                result.details.push(ExecutionItemResult {
                    path: "npm cache clean --force".into(),
                    size_bytes: 0,
                    action: ActionType::ExternalCommand,
                    status: ActionStatus::Failed(
                        "npm cache clean returned non-zero exit code".into(),
                    ),
                });
            }
            Err(e) => {
                result.items_failed = 1;
                result.details.push(ExecutionItemResult {
                    path: "npm cache clean --force".into(),
                    size_bytes: 0,
                    action: ActionType::ExternalCommand,
                    status: ActionStatus::Failed(e.to_string()),
                });
            }
        }

        Ok(result)
    }
}

pub struct PipCacheCleaner;

impl Cleaner for PipCacheCleaner {
    fn name(&self) -> &'static str {
        "PIP Cache"
    }

    fn category(&self) -> CleanCategory {
        CleanCategory::Pip
    }

    fn description(&self) -> &'static str {
        "Cleans Python pip wheel and HTTP cache in ~/Library/Caches/pip"
    }

    fn is_available(&self) -> bool {
        Command::new("which")
            .arg("pip3")
            .output()
            .or_else(|_| Command::new("which").arg("pip").output())
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn scan(&self) -> Result<Vec<CleanTargetItem>> {
        let pip_cache = expand_tilde("~/Library/Caches/pip");
        let (size, count) = calculate_size(&pip_cache).unwrap_or((0, 0));

        if size > 0 {
            Ok(vec![CleanTargetItem {
                path: pip_cache,
                size_bytes: size,
                file_count: count,
                description: "Python pip package cache".into(),
            }])
        } else {
            Ok(Vec::new())
        }
    }

    fn clean(&self, dry_run: bool, _force_permanent: bool) -> Result<CleanResult> {
        let targets = self.scan()?;
        let total_size = targets.iter().map(|t| t.size_bytes).sum();
        let mut result = CleanResult::default();

        if dry_run {
            result.total_bytes_freed = total_size;
            result.items_cleaned = targets.len();
            result.details.push(ExecutionItemResult {
                path: "pip3 cache purge".into(),
                size_bytes: total_size,
                action: ActionType::DryRun,
                status: ActionStatus::Success,
            });
            return Ok(result);
        }

        let cmd = if Command::new("which")
            .arg("pip3")
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
        {
            "pip3"
        } else {
            "pip"
        };

        match Command::new(cmd).args(["cache", "purge"]).status() {
            Ok(s) if s.success() => {
                result.total_bytes_freed = total_size;
                result.items_cleaned = 1;
                result.details.push(ExecutionItemResult {
                    path: format!("{} cache purge", cmd),
                    size_bytes: total_size,
                    action: ActionType::ExternalCommand,
                    status: ActionStatus::Success,
                });
            }
            Ok(_) => {
                result.items_failed = 1;
                result.details.push(ExecutionItemResult {
                    path: format!("{} cache purge", cmd),
                    size_bytes: 0,
                    action: ActionType::ExternalCommand,
                    status: ActionStatus::Failed(format!(
                        "{} cache purge returned non-zero exit code",
                        cmd
                    )),
                });
            }
            Err(e) => {
                result.items_failed = 1;
                result.details.push(ExecutionItemResult {
                    path: format!("{} cache purge", cmd),
                    size_bytes: 0,
                    action: ActionType::ExternalCommand,
                    status: ActionStatus::Failed(e.to_string()),
                });
            }
        }

        Ok(result)
    }
}

pub struct DockerCleaner;

#[derive(Debug, serde::Deserialize)]
#[allow(dead_code)]
struct DockerDfItem {
    #[serde(rename = "Type")]
    type_name: Option<String>,
    #[serde(rename = "TotalCount")]
    total_count: Option<String>,
    #[serde(rename = "Size")]
    size: Option<String>,
    #[serde(rename = "Reclaimable")]
    reclaimable: Option<String>,
}

fn parse_docker_size_str(s: &str) -> u64 {
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
    } else if lower.ends_with("kb") || lower.ends_with("kib") || lower.ends_with("k") {
        (num * 1024.0) as u64
    } else {
        num as u64
    }
}

impl Cleaner for DockerCleaner {
    fn name(&self) -> &'static str {
        "Docker (Build Cache, Dangling Images & Stopped Containers)"
    }

    fn category(&self) -> CleanCategory {
        CleanCategory::Docker
    }

    fn description(&self) -> &'static str {
        "Removes build cache, dangling images, and stopped containers (⚠️ Data in unmounted container filesystems will be lost)"
    }

    fn is_available(&self) -> bool {
        Command::new("docker")
            .args(["info"])
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false)
    }

    fn scan(&self) -> Result<Vec<CleanTargetItem>> {
        if !self.is_available() {
            return Ok(Vec::new());
        }

        let mut items = Vec::new();

        // Run `docker system df --format "{{json .}}"`
        if let Ok(output) = Command::new("docker")
            .args(["system", "df", "--format", "{{json .}}"])
            .output()
        {
            if output.status.success() {
                let stdout = String::from_utf8_lossy(&output.stdout);
                for line in stdout.lines() {
                    if let Ok(df_item) = serde_json::from_str::<DockerDfItem>(line) {
                        let t_name = df_item.type_name.unwrap_or_default();
                        let reclaimable_str = df_item.reclaimable.unwrap_or_default();
                        let size_bytes = parse_docker_size_str(&reclaimable_str);
                        let count_str = df_item.total_count.unwrap_or_default();
                        let count: usize = count_str.parse().unwrap_or(0);

                        if t_name.contains("Build Cache") {
                            items.push(CleanTargetItem {
                                path: "docker:build-cache".into(),
                                size_bytes,
                                file_count: count,
                                description: "Docker BuildKit build cache (100% safe to clear)"
                                    .into(),
                            });
                        } else if t_name.contains("Images") {
                            items.push(CleanTargetItem {
                                path: "docker:dangling-images".into(),
                                size_bytes,
                                file_count: count,
                                description: "Docker dangling / untagged image layers".into(),
                            });
                        } else if t_name.contains("Containers") {
                            items.push(CleanTargetItem {
                                path: "docker:stopped-containers".into(),
                                size_bytes,
                                file_count: count,
                                description: "Docker stopped containers (⚠️ Unmounted container data will be lost!)".into(),
                            });
                        }
                    }
                }
            }
        }

        // If docker system df returned no granular items, fallback to general container path
        if items.is_empty() {
            items.push(CleanTargetItem {
                path: expand_tilde("~/Library/Containers/com.docker.docker/Data"),
                size_bytes: 0,
                file_count: 0,
                description:
                    "Docker build cache and dangling containers (⚠️ Unmounted container data lost)"
                        .into(),
            });
        }

        Ok(items)
    }

    fn clean(&self, dry_run: bool, _force_permanent: bool) -> Result<CleanResult> {
        let targets = self.scan()?;
        let total_size: u64 = targets.iter().map(|t| t.size_bytes).sum();
        let mut result = CleanResult::default();

        if dry_run {
            result.total_bytes_freed = total_size;
            result.items_cleaned = targets.len();
            for t in &targets {
                result.details.push(ExecutionItemResult {
                    path: format!("docker prune ({})", t.path.display()),
                    size_bytes: t.size_bytes,
                    action: ActionType::DryRun,
                    status: ActionStatus::Success,
                });
            }
            return Ok(result);
        }

        // Execute docker builder prune -f, docker image prune -f, docker container prune -f
        let mut total_freed = 0u64;

        // 1. Build cache
        let b_res = Command::new("docker")
            .args(["builder", "prune", "-f"])
            .status();
        let b_size = targets
            .iter()
            .find(|t| t.path.to_string_lossy().contains("build-cache"))
            .map(|t| t.size_bytes)
            .unwrap_or(0);
        if let Ok(s) = b_res {
            if s.success() {
                total_freed += b_size;
                result.items_cleaned += 1;
                result.details.push(ExecutionItemResult {
                    path: "docker builder prune -f".into(),
                    size_bytes: b_size,
                    action: ActionType::ExternalCommand,
                    status: ActionStatus::Success,
                });
            }
        }

        // 2. Images prune
        let i_res = Command::new("docker")
            .args(["image", "prune", "-f"])
            .status();
        let i_size = targets
            .iter()
            .find(|t| t.path.to_string_lossy().contains("dangling-images"))
            .map(|t| t.size_bytes)
            .unwrap_or(0);
        if let Ok(s) = i_res {
            if s.success() {
                total_freed += i_size;
                result.items_cleaned += 1;
                result.details.push(ExecutionItemResult {
                    path: "docker image prune -f".into(),
                    size_bytes: i_size,
                    action: ActionType::ExternalCommand,
                    status: ActionStatus::Success,
                });
            }
        }

        // 3. Container prune
        let c_res = Command::new("docker")
            .args(["container", "prune", "-f"])
            .status();
        let c_size = targets
            .iter()
            .find(|t| t.path.to_string_lossy().contains("stopped-containers"))
            .map(|t| t.size_bytes)
            .unwrap_or(0);
        if let Ok(s) = c_res {
            if s.success() {
                total_freed += c_size;
                result.items_cleaned += 1;
                result.details.push(ExecutionItemResult {
                    path: "docker container prune -f (⚠️ stopped containers removed)".into(),
                    size_bytes: c_size,
                    action: ActionType::ExternalCommand,
                    status: ActionStatus::Success,
                });
            }
        }

        result.total_bytes_freed = total_freed;
        Ok(result)
    }
}
