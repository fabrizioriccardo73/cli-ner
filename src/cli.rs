use clap::{Args, Parser, Subcommand, ValueEnum};
use std::path::PathBuf;

#[derive(Parser, Debug)]
#[command(
    name = "cli-ner",
    about = "🧹 Safe & Documented macOS Disk Space Management CLI",
    version,
    long_about = "CLI-NER is a high-performance, safety-first tool for macOS to analyze disk usage and safely reclaim space from caches, logs, developer artifacts, and temp files with complete audit logs and trash-first safety guarantees."
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    /// Verbose output (diagnostic debug logs)
    #[arg(short, long, global = true)]
    pub verbose: bool,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// 🔍 Analyze disk space usage and locate large files
    Scan(ScanArgs),

    /// 🧹 Safely clean caches, logs, and temporary developer files
    Clean(CleanArgs),

    /// 📊 View audit history and logs of past operations
    Report(ReportArgs),

    /// 🖥️ Interactive Terminal UI (TUI) Dashboard for logs & audit analytics
    #[command(alias = "tui")]
    Dashboard(DashboardArgs),

    /// 🩺 Run system diagnostics, permissions check, and tool availability
    Doctor(DoctorArgs),

    /// 🐳 Safe interactive Docker manager & cleanup wizard
    Docker(DockerArgs),

    /// 📦 Scan and safely clean dormant software project build artifacts (node_modules, target, .venv, etc.)
    #[command(alias = "sweep")]
    Projects(ProjectsArgs),

    /// 🔬 Analyze phantom disk space bloat (Time Machine snapshots, sleep image, Docker disk, caches)
    #[command(alias = "phantom")]
    Bloat(BloatArgs),

    /// 📸 Take, list, and manage point-in-time disk space snapshots
    Snapshot(SnapshotArgs),

    /// 📊 Compare disk usage between snapshots or live state to track what consumed space
    #[command(alias = "compare")]
    Diff(DiffArgs),
}

#[derive(Args, Debug)]
pub struct ScanArgs {
    /// Target path to scan (defaults to user home directory)
    #[arg(short, long)]
    pub path: Option<PathBuf>,

    /// Search specifically for large files exceeding a minimum size
    #[arg(short = 'l', long)]
    pub large_files: bool,

    /// Minimum file size threshold for large file search (e.g. 100MB, 1GB)
    #[arg(short = 'm', long, default_value = "100MB")]
    pub min_size: String,

    /// Number of top items to display
    #[arg(short = 'n', long, default_value_t = 15)]
    pub top: usize,

    /// Output format
    #[arg(long, default_value = "table")]
    pub format: OutputFormat,
}

#[derive(Args, Debug)]
pub struct CleanArgs {
    /// Execute actual cleaning (default is DRY-RUN simulation)
    #[arg(long)]
    pub execute: bool,

    /// Specific category to target (e.g., user-cache, dev, temp, trash, docker, all)
    #[arg(short, long, default_value = "all")]
    pub category: String,

    /// Permanently delete files instead of moving them to macOS Trash (requires confirmation)
    #[arg(long)]
    pub force: bool,

    /// Interactive selection mode to review and choose items
    #[arg(short, long)]
    pub interactive: bool,

    /// Non-interactive mode (proceed without confirmation prompt when --execute is passed)
    #[arg(short = 'y', long = "yes")]
    pub yes: bool,
}

#[derive(Args, Debug)]
pub struct ReportArgs {
    /// Maximum number of past operations to display
    #[arg(short, long, default_value_t = 10)]
    pub limit: usize,

    /// Output format
    #[arg(long, default_value = "table")]
    pub format: OutputFormat,

    /// Show detailed item breakdown for the last operation
    #[arg(long)]
    pub last: bool,

    /// Open interactive TUI Dashboard
    #[arg(long)]
    pub tui: bool,
}

#[derive(Args, Debug)]
pub struct DashboardArgs {
    /// Maximum number of operations to load into Dashboard
    #[arg(short, long, default_value_t = 100)]
    pub limit: usize,
}

#[derive(Args, Debug)]
pub struct DoctorArgs {
    /// Check details for specific component (all, disks, permissions, tools)
    #[arg(default_value = "all")]
    pub target: String,
}

#[derive(ValueEnum, Clone, Copy, Debug, PartialEq, Eq)]
pub enum OutputFormat {
    Table,
    Json,
}

#[derive(Args, Debug)]
pub struct DockerArgs {
    #[command(subcommand)]
    pub action: Option<DockerSubcommand>,

    /// Dry run mode (simulate actions without executing deletion)
    #[arg(long)]
    pub dry_run: bool,
}

#[derive(Subcommand, Debug)]
pub enum DockerSubcommand {
    /// 🧙 Launch guided interactive cleanup wizard
    Wizard,

    /// 📦 Inspect and selectively clean stopped containers
    Containers(DockerContainersArgs),

    /// 🖼️ Inspect and clean dangling or unused images
    Images(DockerImagesArgs),

    /// 🔨 Purge BuildKit build cache
    BuildCache,

    /// 💾 Safety audit of persistent Docker volumes
    Volumes,

    /// 📊 Overview of Docker storage usage (docker system df)
    Status,
}

#[derive(Args, Debug, Default)]
pub struct DockerContainersArgs {
    /// Only list containers without prompting for deletion
    #[arg(short, long)]
    pub list: bool,
}

#[derive(Args, Debug, Default)]
pub struct DockerImagesArgs {
    /// Only list images without prompting for deletion
    #[arg(short, long)]
    pub list: bool,

    /// Automatically prune dangling (<none>:<none>) images
    #[arg(short, long)]
    pub dangling: bool,
}

#[derive(Args, Debug, Clone)]
pub struct ProjectsArgs {
    /// Target directory or workspace root to scan for projects (defaults to common dev directories or cwd)
    #[arg(short, long)]
    pub path: Option<PathBuf>,

    /// Days of inactivity threshold to consider a project dormant
    #[arg(short, long, default_value_t = 60)]
    pub days: u64,

    /// Filter by project ecosystem (all, node, rust, python, gradle, composer, go, flutter)
    #[arg(short = 't', long = "type", default_value = "all")]
    pub project_type: String,

    /// Minimum total build artifact size per project to include (e.g. 10MB, 100MB)
    #[arg(short = 'm', long, default_value = "0MB")]
    pub min_size: String,

    /// Include all projects including active ones (do not filter only dormant)
    #[arg(short, long)]
    pub all: bool,

    /// Interactive checklist selection mode
    #[arg(short, long)]
    pub interactive: bool,

    /// Execute cleaning of dormant project artifacts (default is preview/dry-run)
    #[arg(long)]
    pub execute: bool,

    /// Permanently delete files instead of moving to macOS Trash (requires confirmation)
    #[arg(long)]
    pub force: bool,

    /// Non-interactive mode (proceed without confirmation prompt when --execute is passed)
    #[arg(short = 'y', long = "yes")]
    pub yes: bool,

    /// Output format
    #[arg(long, default_value = "table")]
    pub format: OutputFormat,
}

#[derive(Args, Debug, Clone)]
pub struct BloatArgs {
    /// Minimum size threshold to display a bloat source (e.g. 50MB, 500MB, 1GB)
    #[arg(short = 'm', long, default_value = "50MB")]
    pub min_size: String,

    /// Show detailed filesystem paths, file counts, and extended diagnostic hints
    #[arg(short = 'd', long)]
    pub detailed: bool,

    /// Also scan system directories requiring elevated privileges
    #[arg(long)]
    pub system: bool,

    /// Output format
    #[arg(long, default_value = "table")]
    pub format: OutputFormat,
}

#[derive(Args, Debug, Clone)]
pub struct SnapshotArgs {
    #[command(subcommand)]
    pub action: Option<SnapshotSubcommand>,

    /// Target path to scan (defaults to user home directory)
    #[arg(short, long)]
    pub path: Option<PathBuf>,

    /// Optional label or name for the snapshot (e.g. "before-build", "monday-morning")
    #[arg(short, long)]
    pub name: Option<String>,

    /// Max scan tree depth (default: 2)
    #[arg(long, default_value_t = 2)]
    pub depth: usize,
}

#[derive(Subcommand, Debug, Clone)]
pub enum SnapshotSubcommand {
    /// 📸 Take and save a new disk snapshot
    Create(SnapshotCreateArgs),

    /// 📋 List all saved disk snapshots
    List,

    /// 🗑️ Delete a specific snapshot by ID/name or delete all
    Delete(SnapshotDeleteArgs),
}

#[derive(Args, Debug, Clone, Default)]
pub struct SnapshotCreateArgs {
    /// Optional label or name for the snapshot (e.g. "before-build", "monday-morning")
    #[arg(short, long)]
    pub name: Option<String>,

    /// Target path to scan (defaults to user home directory)
    #[arg(short, long)]
    pub path: Option<PathBuf>,

    /// Max scan tree depth (default: 2)
    #[arg(long, default_value_t = 2)]
    pub depth: usize,
}

#[derive(Args, Debug, Clone, Default)]
pub struct SnapshotDeleteArgs {
    /// ID or label of the snapshot to delete (or use --all)
    pub id: Option<String>,

    /// Delete all saved snapshots
    #[arg(long)]
    pub all: bool,
}

#[derive(Args, Debug, Clone)]
pub struct DiffArgs {
    /// First snapshot ID or name (older base). If not provided, compares live state against the latest snapshot.
    pub base: Option<String>,

    /// Second snapshot ID or name (newer target). If omitted and base is provided, compares base against live state.
    pub target: Option<String>,

    /// Number of top growing directories/files to display
    #[arg(short = 'n', long, default_value_t = 15)]
    pub top: usize,

    /// Minimum growth/reduction delta threshold to display (e.g. 10MB, 100MB, 1GB)
    #[arg(short = 'm', long, default_value = "10MB")]
    pub min_delta: String,

    /// Max scan tree depth for live comparison (default: 2)
    #[arg(long, default_value_t = 2)]
    pub depth: usize,

    /// Save a new snapshot of the current state after calculating diff
    #[arg(short, long)]
    pub save: bool,

    /// Output format
    #[arg(long, default_value = "table")]
    pub format: OutputFormat,
}
