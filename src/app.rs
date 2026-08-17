use crate::cleaner::registry::CleanerRegistry;
use crate::cleaner::traits::CleanResult;
use crate::cli::{CleanArgs, DashboardArgs, DoctorArgs, OutputFormat, ReportArgs, ScanArgs};
use crate::report::operation_log::{
    read_recent_operations, save_operation_log, ActionStatus, ActionType, OperationRecord,
};
use crate::safety::allowlist::CleanCategory;
use crate::scanner::disk_usage::{format_scanned_table, scan_directory_entries};
use crate::scanner::large_files::{find_large_files, format_large_files_table};
use crate::tui::run_dashboard;
use crate::utils::format::{format_bytes, format_duration, parse_size_to_bytes};
use crate::utils::fs::{contract_tilde, expand_tilde};
use crate::utils::platform::get_disk_stats;
use anyhow::{Context, Result};
use colored::*;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, Row, Table};
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
                    .template("{spinner:.green} Searching for files >= {msg} in {wide_bar:.cyan}")?,
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

        let category_filter = match args.category.to_lowercase().as_str() {
            "all" => None,
            "user-cache" | "cache" => Some(CleanCategory::UserCache),
            "user-logs" | "logs" => Some(CleanCategory::UserLogs),
            "temp" | "temp-files" => Some(CleanCategory::TempFiles),
            "xcode" | "xcode-derived-data" => Some(CleanCategory::XcodeDerivedData),
            "archives" | "xcode-archives" => Some(CleanCategory::XcodeArchives),
            "device-support" | "xcode-device-support" => Some(CleanCategory::XcodeDeviceSupport),
            "brew" | "homebrew" => Some(CleanCategory::Homebrew),
            "npm" => Some(CleanCategory::Npm),
            "pip" => Some(CleanCategory::Pip),
            "docker" => Some(CleanCategory::Docker),
            "trash" => Some(CleanCategory::Trash),
            other => {
                anyhow::bail!(
                    "Unknown category: '{}'. Valid categories: all, user-cache, user-logs, temp-files, xcode, brew, npm, pip, docker, trash",
                    other
                );
            }
        };

        // Scan selected targets
        let mut scanned = self.registry.scan_all(category_filter);
        let mut total_reclaimable = 0u64;
        let mut total_items = 0usize;

        let mut preview_table = Table::new();
        preview_table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_header(vec![
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

        println!("\n📋 Cleaning Targets Summary:\n{}", preview_table);
        println!(
            "\n📊 Total Potential Space to Reclaim: {}\n",
            format_bytes(total_reclaimable).bold().green()
        );

        let has_docker = scanned.iter().any(|(c, _)| c.category() == CleanCategory::Docker);

        // Interactive Docker confirmation when executing
        if args.execute && !args.yes && has_docker {
            println!("{}", "🐳 DOCKER CLEANUP DETAILS & SAFETY WARNING:".bold().yellow());
            println!("{}", "   • Docker BuildKit build cache will be purged.".yellow());
            println!("{}", "   • Dangling / untagged images will be removed.".yellow());
            println!("{}", "   • Stopped containers will be pruned.".yellow());
            println!("{}", "   ⚠️  CRITICAL: Any data stored in container filesystems NOT mounted".yellow().bold());
            println!("{}", "      in persistent Docker volumes will be PERMANENTLY LOST!\n".yellow().bold());

            let include_docker = Confirm::new()
                .with_prompt("Do you want to include Docker prune in the cleanup? (Select 'No' to continue without Docker)")
                .default(false)
                .interact()
                .unwrap_or(false);

            if !include_docker {
                println!("{}", "ℹ️  Docker cleanup skipped. Continuing cleanup WITHOUT Docker.\n".cyan());
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
            println!("{}", "   Docker cleanup will remove stopped containers and build cache.".yellow());
            println!("{}", "   Any uncommitted data inside container filesystems NOT stored in".yellow());
            println!("{}", "   persistent Docker volumes or bind-mounts will be PERMANENTLY LOST!\n".yellow().bold());
        }

        if total_reclaimable == 0 && total_items == 0 {
            println!("{}", "✨ All selected categories are already clean!".bold().green());
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
            format_bytes(overall_result.total_bytes_freed).bold().green()
        );
        println!("📁 Items Processed:   {}", overall_result.items_cleaned);
        if overall_result.items_failed > 0 {
            println!(
                "⚠️  Items Failed:      {}",
                overall_result.items_failed.to_string().red()
            );
        }
        println!("📝 Audit Log Saved:   {}", log_path.display().to_string().dimmed());
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
                println!("Total Freed:   {}", format_bytes(last_op.total_bytes_freed).green());
                println!("Items:         {}", last_op.total_items_count);

                let mut table = Table::new();
                table
                    .load_preset(UTF8_FULL)
                    .apply_modifier(UTF8_ROUND_CORNERS)
                    .set_header(vec![
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

        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_header(vec![
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
        println!("{}", "🩺 Running macOS System & Environment Diagnostics...\n".bold().cyan());

        // 1. Mounted Disks
        println!("{}", "💾 Mounted Disks & Storage:".bold());
        let disks = get_disk_stats();
        let mut disk_table = Table::new();
        disk_table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_header(vec![
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

        let mut tool_table = Table::new();
        tool_table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_header(vec![
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

        // 3. Security & Safety Reminder
        println!("{}", "🛡️ Safety Guarantees & Protection:".bold());
        println!(" • System Integrity Protection (SIP) respected: protected system paths are never touched.");
        println!(" • Blocklist active: personal files (Documents, Desktop, SSH keys, Mail, Keychain) are blocked.");
        println!(" • Reversible by default: Cleaned items are moved to macOS Trash (~/.Trash).");
        println!(" • Audit trail: All executions are logged to ~/.cli-ner/logs/\n");

        Ok(())
    }
}
