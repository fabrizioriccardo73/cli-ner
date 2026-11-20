use crate::cleaner::traits::{CleanResult, CleanTargetItem, Cleaner, ExecutionItemResult};
use crate::report::operation_log::{ActionStatus, ActionType};
use crate::safety::allowlist::CleanCategory;
use crate::utils::fs::{calculate_size, expand_tilde};
use anyhow::Result;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::process::Command;
use walkdir::WalkDir;

/// Re-measure a cache directory after an external package manager ran and report the
/// difference that actually landed on disk.
///
/// `brew cleanup`, `npm cache clean` and `pip cache purge` each decide for themselves
/// what to evict and routinely leave part of the cache in place, so assuming the whole
/// directory was reclaimed would overstate the result. Falls back to `before` only if
/// the directory can no longer be measured at all (it was removed outright).
fn measure_reclaimed(path: &Path, before: u64) -> u64 {
    match calculate_size(path) {
        Ok((after, _)) => before.saturating_sub(after),
        Err(_) => before,
    }
}

/// Extract the byte count from a docker prune's own `Total reclaimed space: 1.23GB` line.
fn parse_docker_reclaimed(stdout: &str) -> u64 {
    stdout
        .lines()
        .find_map(|line| {
            line.trim()
                .strip_prefix("Total reclaimed space:")
                .map(|rest| parse_docker_size_str(rest.trim()))
        })
        .unwrap_or(0)
}

/// The `docker … prune` steps to run for the items the user kept.
///
/// Only the selected ones: deselecting "stopped containers" in `--interactive` must
/// not still delete them. Anything that is not one of the three known item ids
/// prunes nothing.
fn docker_prunes_for(targets: &[CleanTargetItem]) -> Vec<(&'static str, [&'static str; 3])> {
    const STEPS: [(&str, &str, [&str; 3]); 3] = [
        (
            "docker:build-cache",
            "docker builder prune -f",
            ["builder", "prune", "-f"],
        ),
        (
            "docker:dangling-images",
            "docker image prune -f",
            ["image", "prune", "-f"],
        ),
        (
            "docker:stopped-containers",
            "docker container prune -f (⚠️ stopped containers removed)",
            ["container", "prune", "-f"],
        ),
    ];

    STEPS
        .into_iter()
        .filter(|(id, _, _)| targets.iter().any(|t| t.path == Path::new(id)))
        .map(|(_, label, args)| (label, args))
        .collect()
}

pub struct HomebrewCleaner;

impl Cleaner for HomebrewCleaner {
    /// Runs an external tool: what it removes never goes through the Trash.
    fn deletes_permanently(&self) -> bool {
        true
    }

    fn name(&self) -> &'static str {
        "Homebrew Cache"
    }

    fn category(&self) -> CleanCategory {
        CleanCategory::Homebrew
    }

    fn description(&self) -> &'static str {
        "Runs `brew cleanup -s`: removes downloaded bottles/sources and old versions of \
         installed formulae. Never uninstalls anything (no `brew autoremove`)"
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

    fn clean_items(
        &self,
        targets: &[CleanTargetItem],
        dry_run: bool,
        _force_permanent: bool,
    ) -> Result<CleanResult> {
        let total_size: u64 = targets.iter().map(|t| t.size_bytes).sum();
        let mut result = CleanResult::default();

        if dry_run {
            result.total_bytes_freed = total_size;
            result.items_cleaned = targets.len();
            result.details.push(ExecutionItemResult::new(
                "brew cleanup -s".into(),
                total_size,
                ActionType::DryRun,
                ActionStatus::Success,
            ));
            return Ok(result);
        }

        let cache_dir = expand_tilde("~/Library/Caches/Homebrew");
        // `brew autoremove` is deliberately not run: it *uninstalls* formulae that
        // were installed as a dependency, including ones the user calls directly
        // (python, node, openssl, …). That is removing software, not a cache.
        match Command::new("brew").args(["cleanup", "-s"]).status() {
            Ok(s) if s.success() => {
                let freed = measure_reclaimed(&cache_dir, total_size);
                result.total_bytes_freed = freed;
                result.items_cleaned = 1;
                result.details.push(ExecutionItemResult::new(
                    "brew cleanup -s".into(),
                    freed,
                    ActionType::ExternalCommand,
                    ActionStatus::Success,
                ));
            }
            Ok(_) => {
                result.items_failed = 1;
                result.details.push(ExecutionItemResult::new(
                    "brew cleanup -s".into(),
                    0,
                    ActionType::ExternalCommand,
                    ActionStatus::Failed("brew cleanup returned non-zero exit code".into()),
                ));
            }
            Err(e) => {
                result.items_failed = 1;
                result.details.push(ExecutionItemResult::new(
                    "brew cleanup -s".into(),
                    0,
                    ActionType::ExternalCommand,
                    ActionStatus::Failed(e.to_string()),
                ));
            }
        }

        Ok(result)
    }
}

pub struct NpmCacheCleaner;

impl Cleaner for NpmCacheCleaner {
    /// Runs an external tool: what it removes never goes through the Trash.
    fn deletes_permanently(&self) -> bool {
        true
    }

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

    fn clean_items(
        &self,
        targets: &[CleanTargetItem],
        dry_run: bool,
        _force_permanent: bool,
    ) -> Result<CleanResult> {
        let total_size: u64 = targets.iter().map(|t| t.size_bytes).sum();
        let mut result = CleanResult::default();

        if dry_run {
            result.total_bytes_freed = total_size;
            result.items_cleaned = targets.len();
            result.details.push(ExecutionItemResult::new(
                "npm cache clean --force".into(),
                total_size,
                ActionType::DryRun,
                ActionStatus::Success,
            ));
            return Ok(result);
        }

        let npm_cache = expand_tilde("~/.npm/_cacache");
        match Command::new("npm")
            .args(["cache", "clean", "--force"])
            .status()
        {
            Ok(s) if s.success() => {
                let freed = measure_reclaimed(&npm_cache, total_size);
                result.total_bytes_freed = freed;
                result.items_cleaned = 1;
                result.details.push(ExecutionItemResult::new(
                    "npm cache clean --force".into(),
                    freed,
                    ActionType::ExternalCommand,
                    ActionStatus::Success,
                ));
            }
            Ok(_) => {
                result.items_failed = 1;
                result.details.push(ExecutionItemResult::new(
                    "npm cache clean --force".into(),
                    0,
                    ActionType::ExternalCommand,
                    ActionStatus::Failed("npm cache clean returned non-zero exit code".into()),
                ));
            }
            Err(e) => {
                result.items_failed = 1;
                result.details.push(ExecutionItemResult::new(
                    "npm cache clean --force".into(),
                    0,
                    ActionType::ExternalCommand,
                    ActionStatus::Failed(e.to_string()),
                ));
            }
        }

        Ok(result)
    }
}

pub struct PipCacheCleaner;

impl Cleaner for PipCacheCleaner {
    /// Runs an external tool: what it removes never goes through the Trash.
    fn deletes_permanently(&self) -> bool {
        true
    }

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

    fn clean_items(
        &self,
        targets: &[CleanTargetItem],
        dry_run: bool,
        _force_permanent: bool,
    ) -> Result<CleanResult> {
        let total_size: u64 = targets.iter().map(|t| t.size_bytes).sum();
        let mut result = CleanResult::default();

        if dry_run {
            result.total_bytes_freed = total_size;
            result.items_cleaned = targets.len();
            result.details.push(ExecutionItemResult::new(
                "pip3 cache purge".into(),
                total_size,
                ActionType::DryRun,
                ActionStatus::Success,
            ));
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

        let pip_cache = expand_tilde("~/Library/Caches/pip");
        match Command::new(cmd).args(["cache", "purge"]).status() {
            Ok(s) if s.success() => {
                let freed = measure_reclaimed(&pip_cache, total_size);
                result.total_bytes_freed = freed;
                result.items_cleaned = 1;
                result.details.push(ExecutionItemResult::new(
                    format!("{} cache purge", cmd),
                    freed,
                    ActionType::ExternalCommand,
                    ActionStatus::Success,
                ));
            }
            Ok(_) => {
                result.items_failed = 1;
                result.details.push(ExecutionItemResult::new(
                    format!("{} cache purge", cmd),
                    0,
                    ActionType::ExternalCommand,
                    ActionStatus::Failed(format!(
                        "{} cache purge returned non-zero exit code",
                        cmd
                    )),
                ));
            }
            Err(e) => {
                result.items_failed = 1;
                result.details.push(ExecutionItemResult::new(
                    format!("{} cache purge", cmd),
                    0,
                    ActionType::ExternalCommand,
                    ActionStatus::Failed(e.to_string()),
                ));
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
    /// Runs an external tool: what it removes never goes through the Trash.
    fn deletes_permanently(&self) -> bool {
        true
    }

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
                                // `docker system df` reports every *unused* image as
                                // reclaimable, but this cleaner only prunes the dangling
                                // ones — so treat this figure as an upper bound.
                                description: "Docker unused image layers (upper bound: only dangling/untagged images are pruned)".into(),
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

        // No breakdown means we don't know what a prune would remove. Fail closed:
        // report it as a scan error (shown as "Skipped") instead of guessing, since
        // guessing here used to mean running every prune, stopped containers included.
        if items.is_empty() {
            anyhow::bail!(
                "could not read `docker system df` — Docker cleanup skipped, nothing pruned"
            );
        }

        Ok(items)
    }

    fn clean_items(
        &self,
        targets: &[CleanTargetItem],
        dry_run: bool,
        _force_permanent: bool,
    ) -> Result<CleanResult> {
        let total_size: u64 = targets.iter().map(|t| t.size_bytes).sum();
        let mut result = CleanResult::default();

        if dry_run {
            result.total_bytes_freed = total_size;
            result.items_cleaned = targets.len();
            for t in targets {
                result.details.push(ExecutionItemResult::new(
                    format!("docker prune ({})", t.path.display()),
                    t.size_bytes,
                    ActionType::DryRun,
                    ActionStatus::Success,
                ));
            }
            return Ok(result);
        }

        // Each prune step reports what it actually removed on its own stdout
        // ("Total reclaimed space: ..."). That figure is the truth; the `docker system df`
        // estimate gathered during the scan is only an upper bound, because `image prune`
        // touches dangling images while df counts every unused one.
        for (label, args) in docker_prunes_for(targets) {
            match Command::new("docker").args(args).output() {
                Ok(output) if output.status.success() => {
                    let reclaimed =
                        parse_docker_reclaimed(&String::from_utf8_lossy(&output.stdout));
                    result.total_bytes_freed += reclaimed;
                    result.items_cleaned += 1;
                    result.details.push(ExecutionItemResult::new(
                        label.to_string(),
                        reclaimed,
                        ActionType::ExternalCommand,
                        ActionStatus::Success,
                    ));
                }
                Ok(output) => {
                    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
                    result.items_failed += 1;
                    result.details.push(ExecutionItemResult::new(
                        label.to_string(),
                        0,
                        ActionType::ExternalCommand,
                        ActionStatus::Failed(if stderr.is_empty() {
                            "docker prune returned a non-zero exit code".into()
                        } else {
                            stderr
                        }),
                    ));
                }
                Err(e) => {
                    result.items_failed += 1;
                    result.details.push(ExecutionItemResult::new(
                        label.to_string(),
                        0,
                        ActionType::ExternalCommand,
                        ActionStatus::Failed(e.to_string()),
                    ));
                }
            }
        }

        Ok(result)
    }
}

pub struct GradleCacheCleaner;

impl Cleaner for GradleCacheCleaner {
    fn name(&self) -> &'static str {
        "Gradle Dependency Cache"
    }

    fn category(&self) -> CleanCategory {
        CleanCategory::Gradle
    }

    fn description(&self) -> &'static str {
        "Cleans cached Gradle dependencies in ~/.gradle/caches and downloaded Gradle distributions in ~/.gradle/wrapper/dists"
    }

    fn scan(&self) -> Result<Vec<CleanTargetItem>> {
        // Each project pins its own Gradle version through the wrapper, so old
        // distributions accumulate in `wrapper/dists` and are re-downloaded on demand.
        let roots = [
            ("~/.gradle/caches", "Gradle dependency cache"),
            ("~/.gradle/wrapper/dists", "Gradle distribution"),
        ];

        let mut items = Vec::new();
        for (root, label) in roots {
            let dir = expand_tilde(root);
            if !dir.is_dir() {
                continue;
            }
            for entry in std::fs::read_dir(&dir)?.flatten() {
                let path = entry.path();
                if let Ok((size, count)) = calculate_size(&path) {
                    if size > 0 {
                        let name = entry.file_name().to_string_lossy().to_string();
                        items.push(CleanTargetItem {
                            path,
                            size_bytes: size,
                            file_count: count,
                            description: format!("{} {}", label, name),
                        });
                    }
                }
            }
        }
        items.sort_by_key(|b| std::cmp::Reverse(b.size_bytes));
        Ok(items)
    }
}

pub struct MavenCacheCleaner;

impl Cleaner for MavenCacheCleaner {
    fn name(&self) -> &'static str {
        "Maven Repository Cache"
    }

    fn category(&self) -> CleanCategory {
        CleanCategory::Maven
    }

    fn description(&self) -> &'static str {
        "Cleans downloaded Maven artifacts in ~/.m2/repository. Artifacts installed locally \
         (`mvn install`, `install:install-file`) are kept: they exist nowhere else"
    }

    fn scan(&self) -> Result<Vec<CleanTargetItem>> {
        let m2_repo = expand_tilde("~/.m2/repository");
        if !m2_repo.is_dir() {
            return Ok(Vec::new());
        }
        let protected = maven_locally_installed_dirs(&m2_repo);
        let mut items = Vec::new();
        collect_maven_targets(&m2_repo, &m2_repo, &protected, &mut items);
        items.sort_by_key(|b| std::cmp::Reverse(b.size_bytes));
        Ok(items)
    }
}

/// Folders of `~/.m2/repository` holding artifacts that were *installed* on this
/// machine rather than downloaded, so deleting them could not be undone by a
/// re-download (an internal library, a jar added with `install:install-file`).
///
/// Recognised by Maven's own bookkeeping:
/// - `maven-metadata-local.xml`, written by `mvn install`;
/// - `_remote.repositories` with an entry for an empty repository id (`foo.jar>=`),
///   which is how Maven Resolver records "installed locally";
/// - a version folder with a `.pom` but no `_remote.repositories` at all: its
///   origin is unknown (legacy local repository), so it is kept too.
fn maven_locally_installed_dirs(repo: &Path) -> HashSet<PathBuf> {
    let mut protected = HashSet::new();
    // dir -> (has a .pom, has _remote.repositories)
    let mut versions: HashMap<PathBuf, (bool, bool)> = HashMap::new();

    for entry in WalkDir::new(repo)
        .follow_links(false)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let Some(parent) = entry.path().parent() else {
            continue;
        };
        let name = entry.file_name().to_string_lossy();

        if name == "maven-metadata-local.xml" {
            protected.insert(parent.to_path_buf());
        } else if name == "_remote.repositories" {
            versions.entry(parent.to_path_buf()).or_default().1 = true;
            let installed_here = std::fs::read_to_string(entry.path())
                .map(|text| {
                    text.lines()
                        .map(str::trim)
                        .any(|l| !l.starts_with('#') && l.ends_with(">="))
                })
                // Unreadable bookkeeping: assume the worst and keep it.
                .unwrap_or(true);
            if installed_here {
                protected.insert(parent.to_path_buf());
            }
        } else if name.ends_with(".pom") {
            versions.entry(parent.to_path_buf()).or_default().0 = true;
        }
    }

    for (dir, (has_pom, has_remote)) in versions {
        if has_pom && !has_remote {
            protected.insert(dir);
        }
    }
    protected
}

/// Offer every entry of `dir` that contains nothing protected; descend into the
/// ones that do, so a single locally installed artifact keeps only itself (and
/// the folders above it) instead of blocking its whole group.
fn collect_maven_targets(
    repo: &Path,
    dir: &Path,
    protected: &HashSet<PathBuf>,
    items: &mut Vec<CleanTargetItem>,
) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if protected.contains(&path) {
            continue;
        }
        if protected.iter().any(|p| p.starts_with(&path)) {
            let is_real_dir = entry.file_type().map(|t| t.is_dir()).unwrap_or(false);
            if is_real_dir {
                collect_maven_targets(repo, &path, protected, items);
            }
            continue;
        }
        if let Ok((size, count)) = calculate_size(&path) {
            if size > 0 {
                let name = path
                    .strip_prefix(repo)
                    .unwrap_or(&path)
                    .display()
                    .to_string();
                items.push(CleanTargetItem {
                    path,
                    size_bytes: size,
                    file_count: count,
                    description: format!("Maven repository {}", name),
                });
            }
        }
    }
}

pub struct CargoCacheCleaner;

impl Cleaner for CargoCacheCleaner {
    fn name(&self) -> &'static str {
        "Cargo Package Cache"
    }

    fn category(&self) -> CleanCategory {
        CleanCategory::Cargo
    }

    fn description(&self) -> &'static str {
        "Cleans the Cargo registry download cache, the extracted crate sources, and cached git dependency checkouts under ~/.cargo"
    }

    fn scan(&self) -> Result<Vec<CleanTargetItem>> {
        // `registry/src` (extracted crates) is usually larger than `registry/cache`
        // (the .crate archives) and was previously left behind entirely. Both are
        // rebuilt automatically on the next `cargo build`.
        let roots = [
            ("~/.cargo/registry/cache", "Cargo registry archives"),
            ("~/.cargo/registry/src", "Cargo extracted crate sources"),
            ("~/.cargo/git/checkouts", "Cargo git dependency checkout"),
            ("~/.cargo/git/db", "Cargo git dependency clone"),
        ];

        let mut items = Vec::new();
        for (root, label) in roots {
            let dir = expand_tilde(root);
            if !dir.is_dir() {
                continue;
            }
            for entry in std::fs::read_dir(&dir)?.flatten() {
                let path = entry.path();
                if let Ok((size, count)) = calculate_size(&path) {
                    if size > 0 {
                        let name = entry.file_name().to_string_lossy().to_string();
                        items.push(CleanTargetItem {
                            path,
                            size_bytes: size,
                            file_count: count,
                            description: format!("{} {}", label, name),
                        });
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

    fn write(path: &Path, text: &str) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, text).unwrap();
    }

    /// Downloaded artifacts are offered; locally installed ones (and only those)
    /// are kept, without blocking the rest of their group.
    #[test]
    fn test_maven_keeps_locally_installed_artifacts() {
        let repo = std::env::temp_dir().join("cli-ner-m2-test");
        let _ = std::fs::remove_dir_all(&repo);

        let downloaded = repo.join("org/apache/commons-lang3/3.14.0");
        write(&downloaded.join("commons-lang3-3.14.0.jar"), "jar");
        write(&downloaded.join("commons-lang3-3.14.0.pom"), "pom");
        write(
            &downloaded.join("_remote.repositories"),
            "#NOTE: internal file\ncommons-lang3-3.14.0.jar>central=\ncommons-lang3-3.14.0.pom>central=\n",
        );

        let installed = repo.join("com/acme/internal-lib/1.0");
        write(&installed.join("internal-lib-1.0.jar"), "jar");
        write(&installed.join("internal-lib-1.0.pom"), "pom");
        write(
            &installed.join("_remote.repositories"),
            "internal-lib-1.0.jar>=\n",
        );

        let installed_file = repo.join("com/vendor/driver");
        write(
            &installed_file.join("maven-metadata-local.xml"),
            "<metadata/>",
        );
        write(&installed_file.join("9.0/driver-9.0.jar"), "jar");

        let legacy = repo.join("com/acme/old-lib/0.1");
        write(&legacy.join("old-lib-0.1.pom"), "pom");

        let sibling = repo.join("com/acme/downloaded-too/2.0");
        write(&sibling.join("downloaded-too-2.0.pom"), "pom");
        write(
            &sibling.join("_remote.repositories"),
            "downloaded-too-2.0.pom>central=\n",
        );

        let protected = maven_locally_installed_dirs(&repo);
        let mut items = Vec::new();
        collect_maven_targets(&repo, &repo, &protected, &mut items);
        let paths: Vec<&Path> = items.iter().map(|i| i.path.as_path()).collect();

        assert!(paths.contains(&repo.join("org").as_path()));
        assert!(paths.contains(&repo.join("com/acme/downloaded-too").as_path()));
        for kept in [&installed, &installed_file, &legacy] {
            assert!(
                !paths
                    .iter()
                    .any(|p| kept.starts_with(p) || p.starts_with(kept)),
                "{} must not be offered",
                kept.display()
            );
        }

        let _ = std::fs::remove_dir_all(&repo);
    }

    /// Docker prints what it really removed; that is the number worth reporting,
    /// not the `docker system df` upper bound gathered before the prune.
    #[test]
    fn test_parse_docker_reclaimed() {
        let stdout = "Deleted build cache objects:\nabc123\n\nTotal reclaimed space: 1.23GB\n";
        // Docker reports binary units, so 1.23GB is 1.23 * 1024^3, truncated.
        assert_eq!(parse_docker_reclaimed(stdout), 1_320_702_443);

        assert_eq!(parse_docker_reclaimed("Total reclaimed space: 0B\n"), 0);
        // A prune that printed nothing recognisable must claim nothing.
        assert_eq!(parse_docker_reclaimed("nothing to do\n"), 0);
    }

    fn docker_item(path: &str) -> CleanTargetItem {
        CleanTargetItem {
            path: path.into(),
            size_bytes: 1,
            file_count: 1,
            description: String::new(),
        }
    }

    /// Deselected Docker items must not be pruned anyway.
    #[test]
    fn test_docker_prunes_only_selected_items() {
        let only_cache = docker_prunes_for(&[docker_item("docker:build-cache")]);
        assert_eq!(only_cache.len(), 1);
        assert_eq!(only_cache[0].1, ["builder", "prune", "-f"]);

        assert!(docker_prunes_for(&[]).is_empty());

        // An unknown item (e.g. a real path) must never fan out into every prune.
        let unknown = expand_tilde("~/Library/Containers/com.docker.docker/Data");
        assert!(docker_prunes_for(&[docker_item(unknown.to_str().unwrap())]).is_empty());
    }

    /// External tools decide for themselves what to evict, so the freed figure comes
    /// from re-measuring, and never exceeds what was there to begin with.
    #[test]
    fn test_measure_reclaimed_uses_observed_difference() {
        let dir = std::env::temp_dir().join("test_cli_ner_measure_reclaimed");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let (before, _) = calculate_size(&dir).unwrap();
        // Nothing was removed, so nothing may be claimed.
        assert_eq!(measure_reclaimed(&dir, before), 0);

        // A directory that vanished entirely reclaimed everything it held.
        let gone = dir.join("does-not-exist");
        assert_eq!(measure_reclaimed(&gone, 5_000), 5_000);

        let _ = std::fs::remove_dir_all(&dir);
    }
}
