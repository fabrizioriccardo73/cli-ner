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
    Dashboard(DashboardArgs),

    /// 🩺 Run system diagnostics, permissions check, and tool availability
    Doctor(DoctorArgs),
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

