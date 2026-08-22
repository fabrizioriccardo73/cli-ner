use crate::cleaner::registry::CleanerRegistry;
use crate::cleaner::traits::CleanResult;
use crate::cli::{
    BloatArgs, CleanArgs, DashboardArgs, DiffArgs, DockerArgs, DockerSubcommand, DoctorArgs,
    OutputFormat, ProjectsArgs, ReportArgs, ScanArgs, SnapshotArgs, SnapshotSubcommand,
};
use crate::docker::{DockerClient, DockerInteractive};
use crate::projects::{
    clean_project_artifacts, prompt_confirm_clean, render_projects_table, scan_projects,
    select_artifacts_interactive, ProjectType, ScannerOptions,
};
use crate::report::operation_log::{
    read_recent_operations, save_operation_log, ActionStatus, ActionType, OperationRecord,
};
use crate::safety::allowlist::CleanCategory;
use crate::scanner::bloat::{format_bloat_table, run_full_bloat_analysis};
use crate::scanner::disk_usage::{format_scanned_table, scan_directory_entries};
use crate::scanner::large_files::{find_large_files, format_large_files_table};
use crate::tracker::diff::{compare_snapshots, compare_with_live, format_diff_table};
use crate::tracker::snapshot::{
    create_snapshot, delete_all_snapshots, delete_snapshot, format_snapshots_table,
    get_latest_snapshot, get_snapshots_dir, list_snapshots, load_snapshot,
};
use crate::tui::run_dashboard;
use crate::utils::format::{format_bytes, format_duration, parse_size_to_bytes};
use crate::utils::fs::{contract_tilde, expand_tilde};
use crate::utils::platform::get_disk_stats;
use crate::utils::table::create_styled_table;
use anyhow::{Context, Result};
use colored::*;
use comfy_table::{Cell, Color, Row};
use dialoguer::Confirm;
use indicatif::{ProgressBar, ProgressStyle};
use std::path::PathBuf;
use std::time::Instant;

pub struct App {
    registry: CleanerRegistry,
}

impl App {
    pub fn new() -> Self {
        Self {
            registry: CleanerRegistry::new(),
        }
    }

    /// Handle `scan` command
    pub fn handle_scan(&self, args: ScanArgs) -> Result<()> {
        let target_path = args
            .path
            .map(expand_tilde)
            .unwrap_or_else(|| dirs::home_dir().unwrap_or_else(|| PathBuf::from(".")));

        println!("{}", "🔍 Starting disk scan...".bold().cyan());

        if args.large_files {
            let min_bytes = parse_size_to_bytes(&args.min_size)
                .context("Failed to parse minimum file size threshold")?;

            let spinner = ProgressBar::new_spinner();
            spinner.set_style(
                ProgressStyle::default_spinner()
                    .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏")
                    .template(
                        "{spinner:.green} Searching for files >= {msg} in {wide_bar:.cyan}",
                    )?,
            );
            spinner.set_message(format_bytes(min_bytes));
            spinner.enable_steady_tick(std::time::Duration::from_millis(80));

            let files = find_large_files(&target_path, min_bytes, args.top)?;
            spinner.finish_and_clear();

            if args.format == OutputFormat::Json {
                println!("{}", serde_json::to_string_pretty(&files)?);
            } else {
                println!("{}", format_large_files_table(&files, min_bytes));
            }
        } else {
            let spinner = ProgressBar::new_spinner();
            spinner.set_message(format!("Scanning {}", target_path.display()));
            spinner.enable_steady_tick(std::time::Duration::from_millis(80));

            let (entries, total_size) = scan_directory_entries(&target_path, args.top)?;
            spinner.finish_and_clear();

            if args.format == OutputFormat::Json {
                let json_data = serde_json::json!({
                    "target_path": target_path.display().to_string(),
                    "total_size_bytes": total_size,
                    "entries": entries.iter().map(|e| {
                        serde_json::json!({
                            "path": e.path.display().to_string(),
                            "size_bytes": e.size_bytes,
                            "file_count": e.file_count,
                        })
                    }).collect::<Vec<_>>()
                });
                println!("{}", serde_json::to_string_pretty(&json_data)?);
            } else {
                println!(
                    "{}",
                    format_scanned_table(&target_path, &entries, total_size)
                );
            }
        }

        Ok(())
    }

    /// Handle `clean` command
    pub fn handle_clean(&self, args: CleanArgs) -> Result<()> {
        let is_dry_run = !args.execute;

        if is_dry_run {
            println!(
                "{}",
                "ℹ️ DRY-RUN MODE: No files will be modified. Use --execute to apply changes."
                    .bold()
                    .yellow()
            );
        } else {
            println!(
                "{}",
                "🚀 EXECUTION MODE: Preparing safe clean operation."
                    .bold()
                    .green()
            );
        }

        let dev_categories = vec![
            CleanCategory::Homebrew,
            CleanCategory::Npm,
            CleanCategory::Pip,
            CleanCategory::Gradle,
            CleanCategory::Maven,
            CleanCategory::Cargo,
        ];
        let xcode_categories = vec![
            CleanCategory::XcodeDerivedData,
            CleanCategory::XcodeArchives,
            CleanCategory::XcodeDeviceSupport,
        ];

        let (category_filter, targets_user_cache): (Option<Vec<CleanCategory>>, bool) =
            match args.category.to_lowercase().as_str() {
                "all" => (None, true),
                "user-cache" | "cache" => (Some(vec![CleanCategory::UserCache]), true),
                "user-logs" | "logs" => (Some(vec![CleanCategory::UserLogs]), false),
                "temp" | "temp-files" => (Some(vec![CleanCategory::TempFiles]), false),
                "xcode" => (Some(xcode_categories), false),
                "xcode-derived-data" => (Some(vec![CleanCategory::XcodeDerivedData]), false),
                "archives" | "xcode-archives" => (Some(vec![CleanCategory::XcodeArchives]), false),
                "device-support" | "xcode-device-support" => {
                    (Some(vec![CleanCategory::XcodeDeviceSupport]), false)
                }
                "dev" | "dev-tools" | "developer" => (Some(dev_categories), false),
                "brew" | "homebrew" => (Some(vec![CleanCategory::Homebrew]), false),
                "npm" => (Some(vec![CleanCategory::Npm]), false),
                "pip" => (Some(vec![CleanCategory::Pip]), false),
                "gradle" => (Some(vec![CleanCategory::Gradle]), false),
                "maven" => (Some(vec![CleanCategory::Maven]), false),
                "cargo" => (Some(vec![CleanCategory::Cargo]), false),
                "docker" => (Some(vec![CleanCategory::Docker]), false),
                "trash" => (Some(vec![CleanCategory::Trash]), false),
                other => {
                    anyhow::bail!(
                        "Unknown category: '{}'. Valid categories: all, dev, user-cache, user-logs, temp-files, xcode, brew, npm, pip, gradle, maven, cargo, docker, trash",
                        other
                    );
                }
            };

        // Scan selected targets
        let mut scanned = self.registry.scan_all(category_filter.as_deref());
        let mut total_reclaimable = 0u64;
        let mut total_items = 0usize;

        let mut preview_table = create_styled_table();
        preview_table.set_header(vec![
            Cell::new("Category").fg(Color::Cyan),
            Cell::new("Target Name").fg(Color::White),
            Cell::new("Items Found").fg(Color::Yellow),
            Cell::new("Reclaimable Size").fg(Color::Green),
            Cell::new("Status").fg(Color::Magenta),
        ]);

        for (cleaner, scan_res) in &scanned {
            match scan_res {
                Ok(items) => {
                    let cat_size: u64 = items.iter().map(|i| i.size_bytes).sum();
                    total_reclaimable += cat_size;
                    total_items += items.len();

                    let status = if items.is_empty() {
                        "Clean".dimmed()
                    } else {
                        "Ready to clean".green()
                    };

                    preview_table.add_row(Row::from(vec![
                        Cell::new(cleaner.category().identifier()),
                        Cell::new(cleaner.name()),
                        Cell::new(items.len().to_string()),
                        Cell::new(format_bytes(cat_size)).fg(Color::Green),
                        Cell::new(status.to_string()),
                    ]));
                }
                Err(e) => {
                    preview_table.add_row(Row::from(vec![
                        Cell::new(cleaner.category().identifier()),
                        Cell::new(cleaner.name()),
                        Cell::new("-"),
                        Cell::new("-"),
                        Cell::new(format!("Skipped: {}", e)).fg(Color::Red),
                    ]));
                }
            }
        }

        let running_browsers = crate::safety::browser::get_running_browsers();

        if !running_browsers.is_empty() && targets_user_cache {
            let browser_names = running_browsers
                .iter()
                .map(|b| b.name)
                .collect::<Vec<_>>()
                .join(", ");
            println!("{}", "🌐 ACTIVE BROWSER SAFETY PROTECTION:".bold().yellow());
            println!(
                "   {} {}",
                "Running browser(s) detected:".yellow(),
                browser_names.bold().white()
            );
            println!(
                "{}",
                "   To prevent broken web pages, crashed tabs, and extension state issues,"
                    .yellow()
            );
            println!(
                "{}",
                "   active browser cache folders are AUTOMATICALLY EXCLUDED from cleanup.".yellow()
            );

            println!(
                "{}\n",
                "   💡 Tip: Close your browser(s) and re-run cli-ner if you want to clean their cache safely.".cyan()
            );
        }

        println!("\n📋 Cleaning Targets Summary:\n{}", preview_table);
        println!(
            "\n📊 Total Potential Space to Reclaim: {}\n",
            format_bytes(total_reclaimable).bold().green()
        );

        let has_docker = scanned
            .iter()
            .any(|(c, _)| c.category() == CleanCategory::Docker);

        // Interactive Docker confirmation when executing
        if args.execute && !args.yes && has_docker {
            println!(
                "{}",
                "🐳 DOCKER CLEANUP DETAILS & SAFETY WARNING:"
                    .bold()
                    .yellow()
            );
            println!(
                "{}",
                "   • Docker BuildKit build cache will be purged.".yellow()
            );
            println!(
                "{}",
                "   • Dangling / untagged images will be removed.".yellow()
            );
            println!("{}", "   • Stopped containers will be pruned.".yellow());
            println!(
                "{}",
                "   ⚠️  CRITICAL: Any data stored in container filesystems NOT mounted"
                    .yellow()
                    .bold()
            );
            println!(
                "{}",
                "      in persistent Docker volumes will be PERMANENTLY LOST!\n"
                    .yellow()
                    .bold()
            );

            let include_docker = Confirm::new()
                .with_prompt("Do you want to include Docker prune in the cleanup? (Select 'No' to continue without Docker)")
                .default(false)
                .interact()
                .unwrap_or(false);

            if !include_docker {
                println!(
                    "{}",
                    "ℹ️  Docker cleanup skipped. Continuing cleanup WITHOUT Docker.\n".cyan()
                );
                scanned.retain(|(c, _)| c.category() != CleanCategory::Docker);

                // Recalculate totals after excluding Docker
                total_reclaimable = 0;
                total_items = 0;
                for (_, scan_res) in &scanned {
                    if let Ok(items) = scan_res {
                        total_reclaimable += items.iter().map(|i| i.size_bytes).sum::<u64>();
                        total_items += items.len();
                    }
                }
            } else {
                println!("{}", "✅ Docker cleanup included in target list.\n".green());
            }
        } else if is_dry_run && has_docker {
            // Explicit Docker Warning info in dry-run mode
            println!("{}", "⚠️  DOCKER CONTAINER DATA WARNING:".bold().yellow());
            println!(
                "{}",
                "   Docker cleanup will remove stopped containers and build cache.".yellow()
            );
            println!(
                "{}",
                "   Any uncommitted data inside container filesystems NOT stored in".yellow()
            );
            println!(
                "{}",
                "   persistent Docker volumes or bind-mounts will be PERMANENTLY LOST!\n"
                    .yellow()
                    .bold()
            );
        }

        if total_reclaimable == 0 && total_items == 0 {
            println!(
                "{}",
                "✨ All selected categories are already clean!"
                    .bold()
                    .green()
            );
            return Ok(());
        }

        // Confirmation if executing
        if args.execute && !args.yes {
            let prompt = if args.force {
                format!(
                    "⚠️ DANGER: You selected --force (Permanent deletion). Delete ~{} permanently?",
                    format_bytes(total_reclaimable)
                )
            } else {
                format!(
                    "Proceed to clean ~{} of safe cache/temp files?",
                    format_bytes(total_reclaimable)
                )
            };

            let confirmed = Confirm::new()
                .with_prompt(prompt)
                .default(false)
                .interact()
                .unwrap_or(false);

            if !confirmed {
                println!("{}", "❌ Operation cancelled by user.".yellow());
                return Ok(());
            }
        }

        // Execute cleaning
        let start_time = Instant::now();
        let mut overall_result = CleanResult::default();
        let mut op_record = OperationRecord::new("clean", &args.category, is_dry_run);

        let pb = ProgressBar::new(scanned.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("[{elapsed_precise}] {bar:40.cyan/blue} {pos}/{len} {msg}")?,
        );

        for (cleaner, scan_res) in scanned {
            pb.set_message(format!("Processing {}", cleaner.name()));
            if scan_res.is_ok() {
                match cleaner.clean(is_dry_run, args.force) {
                    Ok(res) => {
                        overall_result.total_bytes_freed += res.total_bytes_freed;
                        overall_result.items_cleaned += res.items_cleaned;
                        overall_result.items_skipped += res.items_skipped;
                        overall_result.items_failed += res.items_failed;

                        for detail in res.details {
                            op_record.add_item(
                                detail.path,
                                detail.size_bytes,
                                detail.action,
                                detail.status,
                            );
                        }
                    }
                    Err(e) => {
                        op_record.add_item(
                            cleaner.name().to_string(),
                            0,
                            ActionType::DryRun,
                            ActionStatus::Failed(e.to_string()),
                        );
                    }
                }
            }
            pb.inc(1);
        }

        pb.finish_and_clear();
        let elapsed = start_time.elapsed();
        op_record.set_duration(elapsed);

        // Save operation log
        let log_path = save_operation_log(&op_record)?;

        // Print final summary
        println!("\n{}", "🎉 Operation Completed!".bold().green());
        println!("--------------------------------------------------");
        println!("⏱️  Duration:          {}", format_duration(elapsed));
        println!(
            "💾 Total Space Freed: {}",
            format_bytes(overall_result.total_bytes_freed)
                .bold()
                .green()
        );
        println!("📁 Items Processed:   {}", overall_result.items_cleaned);
        if overall_result.items_failed > 0 {
            println!(
                "⚠️  Items Failed:      {}",
                overall_result.items_failed.to_string().red()
            );
        }
        println!(
            "📝 Audit Log Saved:   {}",
            log_path.display().to_string().dimmed()
        );
        println!("--------------------------------------------------");

        if is_dry_run {
            println!(
                "\n💡 To actually execute and reclaim this space, run:\n   {}",
                format!("cli-ner clean --category {} --execute", args.category).cyan()
            );
        }

        Ok(())
    }

    /// Handle interactive `dashboard` command
    pub fn handle_dashboard(&self, args: DashboardArgs) -> Result<()> {
        run_dashboard(args.limit)
    }

    /// Handle `report` command
    pub fn handle_report(&self, args: ReportArgs) -> Result<()> {
        if args.tui {
            return self.handle_dashboard(DashboardArgs { limit: args.limit });
        }

        let operations = read_recent_operations(args.limit)?;

        if operations.is_empty() {
            println!("{}", "No operation logs found in ~/.cli-ner/logs/".yellow());
            return Ok(());
        }

        if args.format == OutputFormat::Json {
            println!("{}", serde_json::to_string_pretty(&operations)?);
            return Ok(());
        }

        if args.last {
            if let Some(last_op) = operations.first() {
                println!(
                    "{}",
                    format!("📄 Last Operation: {} ({})", last_op.id, last_op.timestamp)
                        .bold()
                        .cyan()
                );
                println!("Command:       {}", last_op.command);
                println!("Dry Run:       {}", last_op.dry_run);
                println!("Category:      {}", last_op.category);
                println!(
                    "Total Freed:   {}",
                    format_bytes(last_op.total_bytes_freed).green()
                );
                println!("Items:         {}", last_op.total_items_count);

                let mut table = create_styled_table();
                table.set_header(vec![
                    Cell::new("Item Path").fg(Color::Cyan),
                    Cell::new("Size").fg(Color::Green),
                    Cell::new("Action").fg(Color::Yellow),
                    Cell::new("Status").fg(Color::Magenta),
                ]);

                for item in &last_op.items {
                    let status_str = match &item.status {
                        ActionStatus::Success => "Success".green().to_string(),
                        ActionStatus::Failed(e) => format!("Failed: {}", e).red().to_string(),
                        ActionStatus::Skipped(e) => format!("Skipped: {}", e).yellow().to_string(),
                    };

                    table.add_row(Row::from(vec![
                        Cell::new(contract_tilde(&item.path)),
                        Cell::new(format_bytes(item.size_bytes)).fg(Color::Green),
                        Cell::new(format!("{:?}", item.action)),
                        Cell::new(status_str),
                    ]));
                }

                println!("\nDetailed Items:\n{}", table);
            }
            return Ok(());
        }

        let mut table = create_styled_table();
        table.set_header(vec![
            Cell::new("Timestamp (UTC)").fg(Color::Cyan),
            Cell::new("Command").fg(Color::White),
            Cell::new("Category").fg(Color::Yellow),
            Cell::new("Mode").fg(Color::Blue),
            Cell::new("Freed Space").fg(Color::Green),
            Cell::new("Items").fg(Color::Magenta),
        ]);

        for op in &operations {
            let mode = if op.dry_run {
                "DRY-RUN".yellow()
            } else {
                "EXECUTED".green()
            };

            table.add_row(Row::from(vec![
                Cell::new(op.timestamp.format("%Y-%m-%d %H:%M:%S").to_string()),
                Cell::new(&op.command),
                Cell::new(&op.category),
                Cell::new(mode.to_string()),
                Cell::new(format_bytes(op.total_bytes_freed)).fg(Color::Green),
                Cell::new(op.total_items_count.to_string()),
            ]));
        }

        println!("📊 Past Operations Audit History:\n{}", table);
        Ok(())
    }

    /// Handle `doctor` command
    pub fn handle_doctor(&self, _args: DoctorArgs) -> Result<()> {
        println!(
            "{}",
            "🩺 Running macOS System & Environment Diagnostics...\n"
                .bold()
                .cyan()
        );

        // 1. Mounted Disks
        println!("{}", "💾 Mounted Disks & Storage:".bold());
        let disks = get_disk_stats();
        let mut disk_table = create_styled_table();
        disk_table.set_header(vec![
            Cell::new("Disk Name").fg(Color::Cyan),
            Cell::new("Mount Point").fg(Color::White),
            Cell::new("Available Space").fg(Color::Green),
            Cell::new("Used Space").fg(Color::Yellow),
            Cell::new("Total Space").fg(Color::Magenta),
        ]);

        for d in &disks {
            disk_table.add_row(Row::from(vec![
                Cell::new(&d.name),
                Cell::new(&d.mount_point),
                Cell::new(format_bytes(d.available_space)).fg(Color::Green),
                Cell::new(format_bytes(d.used_space)).fg(Color::Yellow),
                Cell::new(format_bytes(d.total_space)).fg(Color::Magenta),
            ]));
        }
        println!("{}\n", disk_table);

        // 2. Tool Availability
        println!("{}", "🛠️ External Developer Tools Availability:".bold());
        let tools = [
            ("Homebrew (`brew`)", "brew"),
            ("Node Package Manager (`npm`)", "npm"),
            ("Python Package Manager (`pip3`)", "pip3"),
            ("Docker Engine (`docker`)", "docker"),
            ("Xcode Command Line Tools", "xcode-select"),
        ];

        let mut tool_table = create_styled_table();
        tool_table.set_header(vec![
            Cell::new("Tool").fg(Color::Cyan),
            Cell::new("Status").fg(Color::Green),
        ]);

        for (name, cmd) in &tools {
            let available = std::process::Command::new("which")
                .arg(cmd)
                .output()
                .map(|o| o.status.success())
                .unwrap_or(false);

            let status = if available {
                "✅ Available".green()
            } else {
                "❌ Not Found (Skipped in clean)".dimmed()
            };

            tool_table.add_row(Row::from(vec![
                Cell::new(*name),
                Cell::new(status.to_string()),
            ]));
        }
        println!("{}\n", tool_table);

        // 3. Browser Status
        let running_browsers = crate::safety::browser::get_running_browsers();
        println!("{}", "🌐 Active Web Browsers Status:".bold());
        if running_browsers.is_empty() {
            println!(
                " • No active web browsers detected (all browser caches can be safely cleaned)."
            );
        } else {
            let names = running_browsers
                .iter()
                .map(|b| b.name)
                .collect::<Vec<_>>()
                .join(", ");
            println!(" • Active browsers detected: {}", names.yellow().bold());
            println!("   ↳ Caches for these browsers will be automatically protected & excluded during cleanup.");
        }
        println!();

        // 4. Security & Safety Reminder
        println!("{}", "🛡️ Safety Guarantees & Protection:".bold());
        println!(" • System Integrity Protection (SIP) respected: protected system paths are never touched.");
        println!(" • Blocklist active: personal files (Documents, Desktop, SSH keys, Mail, Keychain) are blocked.");
        println!(" • Browser Protection active: running browser caches are excluded to prevent tab/extension corruption.");
        println!(" • Reversible by default: Cleaned items are moved to macOS Trash (~/.Trash).");
        println!(" • Audit trail: All executions are logged to ~/.cli-ner/logs/\n");

        Ok(())
    }

    /// Handle `docker` management and cleanup command
    pub fn handle_docker(&self, args: DockerArgs) -> Result<()> {
        if !DockerClient::is_available() {
            println!(
                "{}",
                "❌ Docker is not available or the daemon is not running."
                    .bold()
                    .red()
            );
            println!(
                "{}",
                "Please verify Docker Desktop is running and that `docker` is in your PATH."
                    .yellow()
            );
            return Ok(());
        }

        match args.action {
            None => {
                // Default: interactive menu
                DockerInteractive::run_interactive_menu()?;
            }
            Some(DockerSubcommand::Wizard) => {
                DockerInteractive::run_guided_cleanup_wizard(args.dry_run)?;
            }
            Some(DockerSubcommand::Containers(c_args)) => {
                if c_args.list {
                    let containers = DockerClient::list_containers()?;
                    println!(
                        "\n📦 Docker Containers:\n{}",
                        DockerInteractive::render_containers_table(&containers)
                    );
                } else {
                    DockerInteractive::manage_containers_interactive(args.dry_run)?;
                }
            }
            Some(DockerSubcommand::Images(i_args)) => {
                if i_args.list {
                    let images = DockerClient::list_images()?;
                    println!(
                        "\n🖼️  Docker Images:\n{}",
                        DockerInteractive::render_images_table(&images)
                    );
                } else if i_args.dangling {
                    if args.dry_run {
                        println!(
                            "{}",
                            "ℹ️ [DRY-RUN] Would run `docker image prune -f`".yellow()
                        );
                    } else {
                        println!("{}", "🧹 Pruning dangling images...".cyan());
                        match DockerClient::prune_dangling_images() {
                            Ok(msg) => println!("{} {}", "✅ Success:".green(), msg),
                            Err(e) => println!("{} {}", "❌ Error:".red(), e),
                        }
                    }
                } else {
                    DockerInteractive::manage_images_interactive(args.dry_run)?;
                }
            }
            Some(DockerSubcommand::BuildCache) => {
                DockerInteractive::clean_build_cache_interactive(args.dry_run)?;
            }
            Some(DockerSubcommand::Volumes) => {
                DockerInteractive::audit_volumes_interactive()?;
            }
            Some(DockerSubcommand::Status) => {
                let df = DockerClient::get_system_df()?;
                println!("\n{}", DockerInteractive::render_df_table(&df));
                let containers = DockerClient::list_containers()?;
                println!(
                    "\n📦 Active & Stopped Containers:\n{}",
                    DockerInteractive::render_containers_table(&containers)
                );
            }
        }

        Ok(())
    }

    /// Handle `projects` (and alias `sweep`) command
    pub fn handle_projects(&self, args: ProjectsArgs) -> Result<()> {
        let min_size_bytes = parse_size_to_bytes(&args.min_size)
            .context("Failed to parse minimum artifact size threshold")?;

        let project_type_filter = if args.project_type.to_lowercase() != "all" {
            match ProjectType::from_str(&args.project_type) {
                Some(pt) => Some(pt),
                None => {
                    anyhow::bail!(
                        "Unknown project type: '{}'. Valid types: all, rust, node, python, gradle, composer, go, flutter",
                        args.project_type
                    );
                }
            }
        } else {
            None
        };

        let options = ScannerOptions {
            base_path: args.path,
            days_threshold: args.days,
            project_type_filter,
            min_size_bytes,
            include_all: args.all,
        };

        println!("{}", "🔍 Scanning for software projects...".bold().cyan());
        let projects = scan_projects(&options)?;

        if args.format == OutputFormat::Json {
            println!("{}", serde_json::to_string_pretty(&projects)?);
            return Ok(());
        }

        if projects.is_empty() {
            println!(
                "{}",
                "✨ No software projects with cleanable build artifacts found matching criteria."
                    .green()
            );
            return Ok(());
        }

        println!("\n{}", render_projects_table(&projects));

        // Interactive mode
        if args.interactive {
            let selected = select_artifacts_interactive(&projects)?;
            if selected.is_empty() {
                println!("{}", "No build artifacts selected for cleaning.".cyan());
                return Ok(());
            }

            // Always request user confirmation before cleaning unless --yes was passed
            if !args.yes {
                let confirmed = prompt_confirm_clean(&selected, args.force)?;
                if !confirmed {
                    println!("{}", "Operation cancelled by user.".yellow());
                    return Ok(());
                }
            }

            let dry_run = !args.execute;
            if dry_run {
                println!(
                    "{}",
                    "ℹ️ DRY-RUN MODE: Simulating project artifact cleaning. Use --execute to apply changes."
                        .bold()
                        .yellow()
                );
            } else {
                println!("{}", "🧹 Cleaning selected project artifacts...".cyan());
            }

            let result = clean_project_artifacts(&selected, dry_run, args.force)?;

            if dry_run {
                println!(
                    "{}",
                    format!(
                        "✅ [DRY-RUN] Would reclaim {} across {} artifact(s).",
                        format_bytes(result.total_bytes_freed).bold(),
                        result.items_cleaned
                    )
                    .green()
                );
            } else {
                println!(
                    "{}",
                    format!(
                        "✅ Successfully cleaned {} artifact(s) and reclaimed {}!",
                        result.items_cleaned,
                        format_bytes(result.total_bytes_freed).bold()
                    )
                    .green()
                );
                if result.items_failed > 0 {
                    println!(
                        "{}",
                        format!("⚠️ {} artifact(s) failed to clean.", result.items_failed).yellow()
                    );
                }
            }

            return Ok(());
        }

        // Non-interactive mode
        if !args.execute {
            println!(
                "\n{}",
                "💡 Tip: Run with `--interactive` (`-i`) to select and clean specific project folders,"
                    .cyan()
            );
            println!(
                "{}",
                "   or add `--execute` to automatically clean all dormant project artifacts."
                    .cyan()
            );
            return Ok(());
        }

        // Execute mode (without interactive flag) -> targets dormant projects (or all if --all)
        let mut targets = Vec::new();
        for p in &projects {
            if args.all || p.is_dormant {
                for a in &p.artifacts {
                    targets.push((p, a));
                }
            }
        }

        if targets.is_empty() {
            println!(
                "{}",
                "No eligible dormant project artifacts found to clean.".yellow()
            );
            return Ok(());
        }

        // Always request user confirmation before cleaning unless --yes was passed
        if !args.yes {
            let confirmed = prompt_confirm_clean(&targets, args.force)?;
            if !confirmed {
                println!("{}", "Operation cancelled by user.".yellow());
                return Ok(());
            }
        }

        println!("{}", "🧹 Cleaning dormant project artifacts...".cyan());
        let result = clean_project_artifacts(&targets, false, args.force)?;

        println!(
            "{}",
            format!(
                "✅ Successfully cleaned {} artifact(s) and reclaimed {}!",
                result.items_cleaned,
                format_bytes(result.total_bytes_freed).bold()
            )
            .green()
        );
        if result.items_failed > 0 {
            println!(
                "{}",
                format!("⚠️ {} artifact(s) failed to clean.", result.items_failed).yellow()
            );
        }

        Ok(())
    }

    /// Handle `bloat` / `phantom` command: analyze hidden/phantom disk space consumers
    pub fn handle_bloat(&self, args: BloatArgs) -> Result<()> {
        let min_bytes = parse_size_to_bytes(&args.min_size)
            .context("Failed to parse minimum size threshold")?;

        let spinner = ProgressBar::new_spinner();
        spinner.set_style(
            ProgressStyle::default_spinner()
                .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏")
                .template("{spinner:.cyan} {msg}")?,
        );
        spinner.set_message("Analyzing phantom disk space consumption across macOS subsystems...");
        spinner.enable_steady_tick(std::time::Duration::from_millis(80));

        let report = run_full_bloat_analysis(args.system)?;
        spinner.finish_and_clear();

        if args.format == OutputFormat::Json {
            println!("{}", serde_json::to_string_pretty(&report)?);
        } else {
            println!(
                "{}",
                "🔬 macOS Phantom Disk Space Bloat Analysis\n"
                    .bold()
                    .cyan()
            );
            println!("{}", format_bloat_table(&report, min_bytes, args.detailed));
        }

        Ok(())
    }

    /// Handle `snapshot` command: create, list, and delete disk space snapshots
    pub fn handle_snapshot(&self, args: SnapshotArgs) -> Result<()> {
        match args.action {
            None => {
                // Default: create snapshot
                self.create_snapshot_interactive(args.path, args.name, args.depth)?;
            }
            Some(SnapshotSubcommand::Create(create_args)) => {
                let target_path = create_args.path.or(args.path);
                let label = create_args.name.or(args.name);
                let depth = if create_args.depth != 3 {
                    create_args.depth
                } else {
                    args.depth
                };
                self.create_snapshot_interactive(target_path, label, depth)?;
            }
            Some(SnapshotSubcommand::List) => {
                let snapshots = list_snapshots()?;
                println!("{}", format_snapshots_table(&snapshots));
            }
            Some(SnapshotSubcommand::Delete(delete_args)) => {
                if delete_args.all {
                    let count = delete_all_snapshots()?;
                    println!("{}", format!("🗑️ Deleted {} snapshot(s).", count).green());
                } else if let Some(id) = delete_args.id {
                    let success = delete_snapshot(&id)?;
                    if success {
                        println!("{}", format!("🗑️ Deleted snapshot '{}'.", id).green());
                    } else {
                        println!("{}", format!("Snapshot '{}' not found.", id).yellow());
                    }
                } else {
                    println!(
                        "{}",
                        "Please specify snapshot ID to delete or use `--all`.".yellow()
                    );
                }
            }
        }
        Ok(())
    }

    fn create_snapshot_interactive(
        &self,
        path: Option<PathBuf>,
        name: Option<String>,
        depth: usize,
    ) -> Result<()> {
        let spinner = ProgressBar::new_spinner();
        spinner.set_style(
            ProgressStyle::default_spinner()
                .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏")
                .template("{spinner:.cyan} {msg}")?,
        );
        spinner.set_message("Scanning filesystem tree and capturing disk snapshot...");
        spinner.enable_steady_tick(std::time::Duration::from_millis(80));

        let snapshot = create_snapshot(path, name, depth, true)?;
        spinner.finish_and_clear();

        let dir = get_snapshots_dir();
        let file_path = dir.join(format!("{}.json", snapshot.id));

        println!(
            "{}",
            "📸 Disk Snapshot Captured Successfully!".bold().green()
        );
        println!("--------------------------------------------------");
        println!("🆔 Snapshot ID:       {}", snapshot.id.bold().cyan());
        if let Some(ref lbl) = snapshot.name {
            println!("🏷️  Label:             {}", lbl.bold().yellow());
        }
        println!(
            "⏱️  Timestamp:         {}",
            snapshot.timestamp.format("%Y-%m-%d %H:%M:%S")
        );
        println!("📂 Root Target:       {}", snapshot.root_path);
        println!("📊 Items Captured:    {}", snapshot.items.len());
        println!(
            "💾 Total Size:        {}",
            format_bytes(snapshot.total_size_bytes).green()
        );
        if let Some(ref disk) = snapshot.disk_stats {
            println!(
                "💽 Available Disk:    {}",
                format_bytes(disk.available_bytes).green()
            );
        }
        println!(
            "📁 Saved File:        {}",
            file_path.display().to_string().dimmed()
        );
        println!("--------------------------------------------------");
        println!(
            "\n💡 Tip: Run `{}` later to see what space has grown since this snapshot.",
            "cli-ner diff".cyan()
        );

        Ok(())
    }

    /// Handle `diff` command: compare two snapshots or compare latest snapshot with live state
    pub fn handle_diff(&self, args: DiffArgs) -> Result<()> {
        let min_delta_bytes = parse_size_to_bytes(&args.min_delta)
            .context("Failed to parse minimum delta threshold")?;

        let spinner = ProgressBar::new_spinner();
        spinner.set_style(
            ProgressStyle::default_spinner()
                .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏")
                .template("{spinner:.cyan} {msg}")?,
        );

        let report = match (args.base, args.target) {
            (Some(base_id), Some(target_id)) => {
                // Compare two specific saved snapshots
                spinner.set_message(format!(
                    "Loading snapshots {} and {}...",
                    base_id, target_id
                ));
                spinner.enable_steady_tick(std::time::Duration::from_millis(80));

                let base_snap = load_snapshot(&base_id)?;
                let target_snap = load_snapshot(&target_id)?;
                compare_snapshots(&base_snap, &target_snap)
            }
            (Some(base_id), None) => {
                // Compare specific base snapshot against live current state
                spinner.set_message(format!(
                    "Comparing snapshot {} against current live filesystem state...",
                    base_id
                ));
                spinner.enable_steady_tick(std::time::Duration::from_millis(80));

                let base_snap = load_snapshot(&base_id)?;
                compare_with_live(&base_snap, args.depth)?
            }
            (None, None) => {
                // Compare latest saved snapshot with live state
                let latest = get_latest_snapshot()?;
                match latest {
                    Some(latest_snap) => {
                        spinner.set_message(
                            "Comparing latest saved snapshot against current live state...",
                        );
                        spinner.enable_steady_tick(std::time::Duration::from_millis(80));
                        compare_with_live(&latest_snap, args.depth)?
                    }
                    None => {
                        spinner.finish_and_clear();
                        println!(
                            "{}",
                            "ℹ️ No previous disk snapshot found. Taking an initial baseline snapshot now...".yellow()
                        );
                        let initial =
                            create_snapshot(None, Some("baseline".into()), args.depth, true)?;
                        println!(
                            "{}",
                            format!(
                                "✅ Baseline snapshot '{}' created. Run `cli-ner diff` again later to see what space has grown.",
                                initial.id
                            )
                            .green()
                        );
                        return Ok(());
                    }
                }
            }
            (None, Some(_)) => unreachable!(),
        };

        spinner.finish_and_clear();

        if args.format == OutputFormat::Json {
            println!("{}", serde_json::to_string_pretty(&report)?);
        } else {
            println!("{}", format_diff_table(&report, args.top, min_delta_bytes));
        }

        if args.save {
            let snap = create_snapshot(None, None, args.depth, true)?;
            println!(
                "\n{}",
                format!("📸 Saved new current snapshot as '{}'.", snap.id).green()
            );
        }

        Ok(())
    }
}
