use crate::cleaner::registry::CleanerRegistry;
use crate::cleaner::traits::{CleanResult, CleanTargetItem, Cleaner};
use crate::cli::{
    BloatArgs, CleanArgs, DashboardArgs, DiffArgs, DockerArgs, DockerSubcommand, DoctorArgs,
    MaintainArgs, OutputFormat, ProjectsArgs, ReportArgs, ScanArgs, SnapshotArgs,
    SnapshotSubcommand,
};
use crate::docker::{DockerClient, DockerInteractive};
use crate::maintenance::registry::MaintenanceRegistry;
use crate::maintenance::traits::{ActionCategory, MaintenanceAction};
use crate::projects::cleaner::ProjectCleanResult;
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
use crate::utils::platform::{available_space_home, get_disk_stats, is_root};
use crate::utils::table::create_styled_table;
use anyhow::{Context, Result};
use colored::*;
use comfy_table::{Cell, Color, Row};
use dialoguer::{Confirm, MultiSelect};
use indicatif::{ProgressBar, ProgressStyle};
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Instant;

/// One cleaner plus the targets it found, and the reason it found none if it failed.
///
/// Carried around as a unit so the preview, the interactive selection, the confirmation
/// and the execution all operate on one identical set of items.
type ScannedTargets<'a> = (&'a Box<dyn Cleaner>, Vec<CleanTargetItem>, Option<String>);

/// Drop targets that sit inside another cleaner's target.
///
/// Categories deliberately overlap: `user-cache` sweeps all of `~/Library/Caches`, which
/// contains the Homebrew and pip caches that their own cleaners also claim. Left alone,
/// `--category all` would show those bytes twice in the preview, count them twice in the
/// summary, and have the second cleaner fail on a path the first one already removed.
///
/// Registry order decides the winner: the first cleaner to claim a path keeps it. Entries
/// that are not filesystem paths at all (`docker:build-cache`) can never overlap and are
/// always kept. Returns how many targets were folded away.
fn dedupe_overlapping_targets(scanned: &mut [ScannedTargets<'_>]) -> usize {
    let mut claimed: Vec<PathBuf> = Vec::new();
    let mut dropped = 0usize;

    for (_, items, _) in scanned.iter_mut() {
        items.retain(|item| {
            if !item.path.is_absolute() {
                return true;
            }
            if claimed.iter().any(|owner| item.path.starts_with(owner)) {
                dropped += 1;
                return false;
            }
            claimed.push(item.path.clone());
            true
        });
    }

    dropped
}

/// Total bytes and item count still selected across every cleaner.
fn sum_targets<T>(scanned: &[(T, Vec<CleanTargetItem>, Option<String>)]) -> (u64, usize) {
    scanned
        .iter()
        .fold((0u64, 0usize), |(bytes, count), (_, items, _)| {
            (
                bytes + items.iter().map(|i| i.size_bytes).sum::<u64>(),
                count + items.len(),
            )
        })
}

/// Present every scanned target as a tickable checklist.
///
/// Returns, per cleaner (matching the input order), the set of item indices the user kept.
/// `None` means the user aborted the selection entirely.
fn select_clean_targets_interactive(
    scanned: &[ScannedTargets<'_>],
) -> Result<Option<Vec<HashSet<usize>>>> {
    // Flatten to a single checklist while remembering where each row came from.
    let mut origins: Vec<(usize, usize)> = Vec::new();
    let mut labels: Vec<String> = Vec::new();

    for (cleaner_idx, (cleaner, items, _)) in scanned.iter().enumerate() {
        for (item_idx, item) in items.iter().enumerate() {
            origins.push((cleaner_idx, item_idx));
            labels.push(format!(
                "{:>10}  [{}] {}",
                format_bytes(item.size_bytes),
                cleaner.category().identifier(),
                contract_tilde(&item.path)
            ));
        }
    }

    if labels.is_empty() {
        return Ok(Some(vec![HashSet::new(); scanned.len()]));
    }

    println!(
        "{}",
        "🗂️  Select the items to clean (space toggles, ↑/↓ moves, enter confirms, esc aborts):"
            .bold()
            .cyan()
    );

    // Nothing is pre-ticked: an interactive run must be an explicit opt-in per item.
    let defaults = vec![false; labels.len()];
    let selection = MultiSelect::new()
        .with_prompt("Items to clean")
        .items(&labels)
        .defaults(&defaults)
        .interact_opt();

    let chosen = match selection {
        Ok(Some(indices)) => indices,
        // Escape pressed.
        Ok(None) => return Ok(None),
        Err(e) => {
            // No terminal to draw a checklist on (piped input, CI). Refusing here is the
            // safe answer: silently falling back to "clean everything" would delete far
            // more than an interactive run was asking for.
            anyhow::bail!(
                "--interactive needs a terminal to display the checklist ({e}). \
                 Re-run without --interactive, or run it from an interactive shell."
            );
        }
    };

    let mut per_cleaner = vec![HashSet::new(); scanned.len()];
    for flat_idx in chosen {
        let (cleaner_idx, item_idx) = origins[flat_idx];
        per_cleaner[cleaner_idx].insert(item_idx);
    }

    Ok(Some(per_cleaner))
}

/// Print the outcome of a project sweep, keeping "moved to the Trash" and
/// "actually gone" as two distinct numbers.
fn report_project_clean_outcome(result: &ProjectCleanResult) {
    if result.total_bytes_trashed > 0 {
        println!(
            "{}",
            format!(
                "✅ Moved {} artifact(s) ({}) to the Trash.",
                result.items_cleaned,
                format_bytes(result.total_bytes_trashed).bold()
            )
            .green()
        );
        println!(
            "{}",
            format!(
                "   ♻️  Still on disk and recoverable from ~/.Trash — empty it to reclaim the {}.",
                format_bytes(result.total_bytes_trashed)
            )
            .cyan()
        );
    }
    if result.total_bytes_freed > 0 {
        println!(
            "{}",
            format!(
                "✅ Permanently deleted {} of build artifacts.",
                format_bytes(result.total_bytes_freed).bold()
            )
            .green()
        );
    }
    if result.items_failed > 0 {
        println!(
            "{}",
            format!("⚠️ {} artifact(s) failed to clean.", result.items_failed).yellow()
        );
    }
}

pub struct App {
    registry: CleanerRegistry,
    maintenance_registry: MaintenanceRegistry,
}

impl App {
    pub fn new() -> Self {
        Self {
            registry: CleanerRegistry::new(),
            maintenance_registry: MaintenanceRegistry::new(),
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
            CleanCategory::Pnpm,
            CleanCategory::Pip,
            CleanCategory::Gradle,
            CleanCategory::Maven,
            CleanCategory::Cargo,
            CleanCategory::Go,
        ];
        let xcode_categories = vec![
            CleanCategory::XcodeDerivedData,
            CleanCategory::XcodeArchives,
            CleanCategory::XcodeDeviceSupport,
            CleanCategory::XcodeSimulator,
        ];

        let (category_filter, targets_user_cache): (Option<Vec<CleanCategory>>, bool) = match args
            .category
            .to_lowercase()
            .as_str()
        {
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
            "go" | "golang" => (Some(vec![CleanCategory::Go]), false),
            "pnpm" => (Some(vec![CleanCategory::Pnpm]), false),
            "simulator" | "xcode-simulator" => (Some(vec![CleanCategory::XcodeSimulator]), false),
            "docker" => (Some(vec![CleanCategory::Docker]), false),
            "trash" => (Some(vec![CleanCategory::Trash]), false),
            "extended-app-cache" | "extended-cache" => {
                (Some(vec![CleanCategory::ExtendedAppCache]), false)
            }
            "app-caches" | "deep" => (
                Some(vec![
                    CleanCategory::ExtendedAppCache,
                    CleanCategory::AppSupportCache,
                    CleanCategory::ContainerCache,
                    CleanCategory::MetalShaderCache,
                ]),
                false,
            ),
            "app-support-cache" | "electron" => (Some(vec![CleanCategory::AppSupportCache]), false),
            "container-cache" | "containers" => (Some(vec![CleanCategory::ContainerCache]), false),
            "metal-cache" | "metal" => (Some(vec![CleanCategory::MetalShaderCache]), false),
            "biome" => (Some(vec![CleanCategory::BiomeUsageData]), false),
            other => {
                anyhow::bail!(
                        "Unknown category: '{}'. Valid categories: all, dev, xcode, app-caches, user-cache, user-logs, temp-files, brew, npm, pnpm, pip, gradle, maven, cargo, go, xcode-derived-data, xcode-archives, xcode-device-support, xcode-simulator, docker, trash, extended-app-cache, app-support-cache, container-cache, metal-cache, biome (personal data, never included in `all`)",
                        other
                    );
            }
        };

        // Record the volume's real free space up front. The summary reports the
        // measured delta rather than the logical size of the targets, because moving
        // items to the Trash frees nothing until the Trash is emptied.
        let free_before = available_space_home();

        // Scan selected targets. Cleaners run in parallel and the resulting item lists
        // are kept: everything below — preview, selection, confirmation and execution —
        // operates on this exact set, so what the user approves is what gets removed.
        let mut scanned: Vec<ScannedTargets<'_>> = self
            .registry
            .scan_all(category_filter.as_deref())
            .into_iter()
            .map(|(cleaner, res)| match res {
                Ok(items) => (cleaner, items, None),
                Err(e) => (cleaner, Vec::new(), Some(e.to_string())),
            })
            .collect();

        let folded = dedupe_overlapping_targets(&mut scanned);
        if folded > 0 {
            println!(
                "{}",
                format!(
                    "ℹ️  {} target(s) are nested inside a broader category and are counted once, not twice.",
                    folded
                )
                .dimmed()
            );
        }

        let mut preview_table = create_styled_table();
        preview_table.set_header(vec![
            Cell::new("Category").fg(Color::Cyan),
            Cell::new("Target Name").fg(Color::White),
            Cell::new("Items Found").fg(Color::Yellow),
            Cell::new("Reclaimable Size").fg(Color::Green),
            Cell::new("Status").fg(Color::Magenta),
        ]);

        for (cleaner, items, scan_error) in &scanned {
            match scan_error {
                None => {
                    let cat_size: u64 = items.iter().map(|i| i.size_bytes).sum();

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
                Some(e) => {
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

        let (mut total_reclaimable, mut total_items) = sum_targets(&scanned);

        // macOS gates ~/.Trash and a few other locations behind Full Disk Access. Without
        // it the scan quietly returns "Operation not permitted", so say what that means
        // rather than leaving the user with an opaque errno.
        let permission_blocked: Vec<&str> = scanned
            .iter()
            .filter(|(_, _, err)| {
                err.as_deref().is_some_and(|e| {
                    e.contains("Operation not permitted") || e.contains("os error 1")
                })
            })
            .map(|(cleaner, _, _)| cleaner.name())
            .collect();

        if !permission_blocked.is_empty() {
            println!("{}", "🔒 SOME LOCATIONS COULD NOT BE READ:".bold().yellow());
            println!(
                "   {} {}",
                "Blocked by macOS privacy protection:".yellow(),
                permission_blocked.join(", ").bold().white()
            );
            println!(
                "{}",
                "   Their contents are excluded from every number below — the real reclaimable"
                    .yellow()
            );
            println!("{}", "   total is higher than what is shown.".yellow());
            println!(
                "{}\n",
                "   💡 Grant Full Disk Access to your terminal in System Settings › Privacy & Security."
                    .cyan()
            );
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
            .any(|(c, _, _)| c.category() == CleanCategory::Docker);

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
                scanned.retain(|(c, _, _)| c.category() != CleanCategory::Docker);
                (total_reclaimable, total_items) = sum_targets(&scanned);
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

        // Interactive per-item selection: narrow the scanned set down to the exact
        // entries the user ticked. Applies to dry-runs too, so a selection can be
        // previewed before committing to it.
        if args.interactive {
            match select_clean_targets_interactive(&scanned)? {
                Some(selection) => {
                    for (idx, (_, items, _)) in scanned.iter_mut().enumerate() {
                        let keep = &selection[idx];
                        let mut cursor = 0usize;
                        items.retain(|_| {
                            let selected = keep.contains(&cursor);
                            cursor += 1;
                            selected
                        });
                    }
                    (total_reclaimable, total_items) = sum_targets(&scanned);

                    if total_items == 0 {
                        println!("{}", "❌ No items selected. Nothing to do.".yellow());
                        return Ok(());
                    }
                    println!(
                        "\n✅ {} item(s) selected, {} to reclaim.\n",
                        total_items.to_string().bold(),
                        format_bytes(total_reclaimable).bold().green()
                    );
                }
                None => {
                    println!("{}", "❌ Selection cancelled by user.".yellow());
                    return Ok(());
                }
            }
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
                    "Move ~{} of caches/temp files to the Trash? (space is reclaimed once the Trash is emptied)",
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

        for (cleaner, items, scan_error) in &scanned {
            pb.set_message(format!("Processing {}", cleaner.name()));
            if scan_error.is_none() {
                // Deliberately `clean_items` and not `clean`: re-scanning here would let
                // the target set drift from what was previewed, and would make the Trash
                // cleaner permanently destroy whatever this same run had just trashed.
                match cleaner.clean_items(items, is_dry_run, args.force) {
                    Ok(res) => {
                        for detail in &res.details {
                            op_record.add_item(
                                detail.path.clone(),
                                detail.size_bytes,
                                detail.action.clone(),
                                detail.status.clone(),
                                detail.trashed_to.clone(),
                            );
                        }
                        overall_result.merge(res);
                    }
                    Err(e) => {
                        op_record.add_item(
                            cleaner.name().to_string(),
                            0,
                            ActionType::DryRun,
                            ActionStatus::Failed(e.to_string()),
                            None,
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

        if is_dry_run {
            println!(
                "📊 Would reclaim:     {}",
                format_bytes(overall_result.total_bytes_processed())
                    .bold()
                    .green()
            );
        } else {
            if overall_result.total_bytes_trashed > 0 {
                println!(
                    "🗑️  Moved to Trash:    {} {}",
                    format_bytes(overall_result.total_bytes_trashed)
                        .bold()
                        .yellow(),
                    "(still on disk — empty the Trash to reclaim it)".dimmed()
                );
            }
            if overall_result.total_bytes_freed > 0 {
                println!(
                    "💥 Deleted for good:  {}",
                    format_bytes(overall_result.total_bytes_freed)
                        .bold()
                        .green()
                );
            }

            // Ground truth: what the volume actually reports now.
            if let (Some(before), Some(after)) = (free_before, available_space_home()) {
                let delta = after.saturating_sub(before);
                println!(
                    "💽 Disk space gained: {} {}",
                    format_bytes(delta).bold().green(),
                    format!(
                        "({} free → {} free, measured)",
                        format_bytes(before),
                        format_bytes(after)
                    )
                    .dimmed()
                );
            }
        }

        println!("📁 Items Processed:   {}", overall_result.items_cleaned);
        if overall_result.items_skipped > 0 {
            println!(
                "⏭️  Items Skipped:     {} {}",
                overall_result.items_skipped.to_string().yellow(),
                "(failed a safety check — see the audit log)".dimmed()
            );
        }
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
        } else if overall_result.total_bytes_trashed > 0 {
            println!(
                "\n♻️  Everything above is recoverable from {} until you empty it.",
                "~/.Trash".cyan()
            );
            println!(
                "   Run `{}` to see where each item came from and where it now sits.",
                "cli-ner report --last".cyan()
            );
            println!(
                "   Run `{}` when you are ready to actually reclaim the space.",
                "cli-ner clean --category trash --execute".cyan()
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
                    "Deleted:       {}",
                    format_bytes(last_op.total_bytes_freed).green()
                );
                println!(
                    "Moved to Trash:{} {}",
                    format!(" {}", format_bytes(last_op.total_bytes_trashed)).yellow(),
                    "(recoverable until the Trash is emptied)".dimmed()
                );
                println!("Items:         {}", last_op.total_items_count);

                let mut table = create_styled_table();
                table.set_header(vec![
                    Cell::new("Original Path").fg(Color::Cyan),
                    Cell::new("Size").fg(Color::Green),
                    Cell::new("Action").fg(Color::Yellow),
                    Cell::new("Status").fg(Color::Magenta),
                    Cell::new("Recover From").fg(Color::Blue),
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
                        Cell::new(
                            item.trashed_to
                                .as_ref()
                                .map(contract_tilde)
                                .unwrap_or_else(|| "—".to_string()),
                        )
                        .fg(Color::Blue),
                    ]));
                }

                println!("\nDetailed Items:\n{}", table);
                if last_op.total_bytes_trashed > 0 {
                    println!(
                        "\n♻️  To restore an item, move it back from the \"Recover From\" path to its original path."
                    );
                }
            }
            return Ok(());
        }

        let mut table = create_styled_table();
        table.set_header(vec![
            Cell::new("Timestamp (UTC)").fg(Color::Cyan),
            Cell::new("Command").fg(Color::White),
            Cell::new("Category").fg(Color::Yellow),
            Cell::new("Mode").fg(Color::Blue),
            Cell::new("Deleted").fg(Color::Green),
            Cell::new("To Trash").fg(Color::DarkYellow),
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
                Cell::new(format_bytes(op.total_bytes_trashed)).fg(Color::DarkYellow),
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

        // 3. Filesystem access checks
        println!("{}", "🔐 Filesystem Access (Full Disk Access):".bold());
        let access_targets = [
            ("Trash", "~/.Trash"),
            ("User Caches", "~/Library/Caches"),
            ("User Logs", "~/Library/Logs"),
            ("Developer / Xcode", "~/Library/Developer"),
        ];
        let mut access_table = create_styled_table();
        access_table.set_header(vec![
            Cell::new("Location").fg(Color::Cyan),
            Cell::new("Readable").fg(Color::Green),
        ]);
        let mut any_blocked = false;
        for (label, path) in access_targets {
            let expanded = expand_tilde(path);
            let status = if !expanded.exists() {
                "— not present".dimmed().to_string()
            } else {
                match std::fs::read_dir(&expanded) {
                    Ok(_) => "✅ Yes".green().to_string(),
                    Err(e) => {
                        any_blocked = true;
                        format!("❌ No ({})", e).red().to_string()
                    }
                }
            };
            access_table.add_row(Row::from(vec![Cell::new(label), Cell::new(status)]));
        }
        println!("{}", access_table);
        if any_blocked {
            println!(
                "{}",
                " ⚠️  Blocked locations are silently excluded from scans, so reported totals are lower than reality."
                    .yellow()
            );
            println!(
                "{}\n",
                " 💡 Fix: System Settings › Privacy & Security › Full Disk Access → add your terminal app."
                    .cyan()
            );
        } else {
            println!();
        }

        // 4. Browser Status
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

        // 5. Security & Safety Reminder
        println!("{}", "🛡️ Safety Guarantees & Protection:".bold());
        println!(" • System Integrity Protection (SIP) respected: protected system paths are never touched.");
        println!(" • Blocklist active: personal files (Documents, Desktop, SSH keys, Mail, Keychain) are blocked.");
        println!(" • Browser Protection active: running browser caches are excluded to prevent tab/extension corruption.");
        println!(
            " • Reversible by default: cleaned items are moved to ~/.Trash, so disk space is only"
        );
        println!(
            "   actually reclaimed once the Trash is emptied — cli-ner reports the two separately."
        );
        println!(" • Restore info: the audit log records each item's original path and where it now sits.");
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
                        format_bytes(result.total_bytes_processed()).bold(),
                        result.items_cleaned
                    )
                    .green()
                );
            } else {
                report_project_clean_outcome(&result);
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

        report_project_clean_outcome(&result);

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
                "🔬 macOS Phantom Disk Space Bloat Analysis\n".bold().cyan()
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
                let depth = create_args.depth.unwrap_or(args.depth);
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

    /// Handle `maintain` command: system maintenance actions (index rebuilds, disk
    /// housekeeping, …).
    ///
    /// Deliberately separate from `clean`: a `MaintenanceAction` runs system commands,
    /// not path-based cache/trash cleanup, so it has its own preview/confirm/execute
    /// flow and its own "no root by default" gate. See docs/MAINTENANCE_ROADMAP.md.
    pub fn handle_maintain(&self, args: MaintainArgs) -> Result<()> {
        let is_dry_run = !args.execute;

        if is_dry_run {
            println!(
                "{}",
                "ℹ️ DRY-RUN MODE: No command will be executed. Use --execute to apply changes."
                    .bold()
                    .yellow()
            );
        } else {
            println!(
                "{}",
                "🚀 EXECUTION MODE: Preparing maintenance actions."
                    .bold()
                    .green()
            );
        }

        let category_filter: Option<ActionCategory> = match args.category.to_lowercase().as_str() {
            "all" => None,
            "system-index" => Some(ActionCategory::SystemIndex),
            "extended-cache" => Some(ActionCategory::ExtendedCache),
            "disk-maintenance" => Some(ActionCategory::DiskMaintenance),
            "system-state" => Some(ActionCategory::SystemState),
            other => {
                anyhow::bail!(
                    "Unknown category: '{}'. Valid categories: all, system-index, extended-cache, disk-maintenance, system-state",
                    other
                );
            }
        };

        let in_category: Vec<&dyn MaintenanceAction> = self
            .maintenance_registry
            .all()
            .iter()
            .map(|a| a.as_ref())
            .filter(|a| category_filter.map(|c| a.category() == c).unwrap_or(true))
            .collect();

        if in_category.is_empty() {
            println!(
                "{}",
                "✨ No maintenance actions are available yet in this build.".green()
            );
            println!(
                "{}",
                "   They are being ported in phases — see docs/MAINTENANCE_ROADMAP.md for the plan."
                    .dimmed()
            );
            return Ok(());
        }

        // Actions whose underlying binary/service isn't present on this machine are
        // reported, never silently dropped.
        let (available, unavailable): (Vec<_>, Vec<_>) =
            in_category.into_iter().partition(|a| a.is_available());

        if !unavailable.is_empty() {
            let names = unavailable
                .iter()
                .map(|a| a.name())
                .collect::<Vec<_>>()
                .join(", ");
            println!(
                "{}",
                format!("⏭️  Not available on this machine: {}", names).dimmed()
            );
        }

        if available.is_empty() {
            println!(
                "{}",
                "✨ Nothing to do: every matching action is unavailable on this machine.".green()
            );
            return Ok(());
        }

        let mut table = create_styled_table();
        table.set_header(vec![
            Cell::new("Category").fg(Color::Cyan),
            Cell::new("Action").fg(Color::White),
            Cell::new("Risk").fg(Color::Yellow),
            Cell::new("Admin?").fg(Color::Red),
            Cell::new("Reversible?").fg(Color::Blue),
            Cell::new("Status").fg(Color::Green),
            Cell::new("Commands").fg(Color::Magenta),
        ]);

        for action in &available {
            let commands = action
                .commands()
                .iter()
                .map(|c| c.display())
                .collect::<Vec<_>>()
                .join(" && ");

            let status = match action.preflight() {
                Ok(()) => "ready".green().to_string(),
                Err(reason) => format!("⚠️ {}", reason).yellow().to_string(),
            };

            table.add_row(Row::from(vec![
                Cell::new(action.category().identifier()),
                Cell::new(action.name()),
                Cell::new(action.risk().label()),
                Cell::new(if action.requires_admin() { "yes" } else { "no" }),
                Cell::new(if action.is_reversible() { "yes" } else { "no" }),
                Cell::new(status),
                Cell::new(commands),
            ]));
        }

        println!("\n📋 Maintenance Actions:\n{}", table);

        // "No root by default" (same rule `clean` follows): an action needing elevated
        // privileges never runs unless the caller explicitly opted in.
        let (runnable, admin_blocked): (Vec<_>, Vec<_>) = available
            .into_iter()
            .partition(|a| !a.requires_admin() || args.allow_admin);

        if !admin_blocked.is_empty() {
            let names = admin_blocked
                .iter()
                .map(|a| a.name())
                .collect::<Vec<_>>()
                .join(", ");
            println!(
                "{}",
                "🔒 SKIPPED (require elevated privileges):".bold().yellow()
            );
            println!("   {}", names.white());
            println!(
                "{}",
                "   Re-run with --allow-admin to allow these to prompt for your password.\n".cyan()
            );
        }

        if runnable.is_empty() {
            println!("{}", "Nothing left to do without --allow-admin.".yellow());
            return Ok(());
        }

        if is_dry_run {
            println!(
                "\n💡 To actually run these actions, run:\n   {}",
                format!("cli-ner maintain --category {} --execute", args.category).cyan()
            );
            return Ok(());
        }

        let risky_count = runnable
            .iter()
            .filter(|a| a.risk().requires_confirmation())
            .count();

        if !args.yes {
            let prompt = if risky_count == 0 {
                format!("Run {} maintenance action(s)?", runnable.len())
            } else {
                format!(
                    "Run {} maintenance action(s), including {} medium/high-risk one(s) listed above?",
                    runnable.len(),
                    risky_count
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

        // One native `sudo` prompt up front; each admin command then reuses those
        // credentials non-interactively (`sudo -n`). If the user declines, or there
        // is no terminal to prompt on, admin actions are skipped — not failed one
        // by one with a confusing permission error each.
        let needs_sudo = runnable.iter().any(|a| a.requires_admin()) && !is_root();
        let admin_granted = if needs_sudo {
            println!(
                "{}",
                "🔐 Some actions need administrator rights — macOS will ask for your password."
                    .cyan()
            );
            std::process::Command::new("/usr/bin/sudo")
                .arg("-v")
                .status()
                .map(|s| s.success())
                .unwrap_or(false)
        } else {
            true
        };

        let start_time = Instant::now();
        let mut op_record = OperationRecord::new("maintain", &args.category, false);
        let mut succeeded = 0usize;
        let mut failed = 0usize;
        let mut skipped = 0usize;

        for action in &runnable {
            if action.requires_admin() && !admin_granted {
                skipped += 1;
                op_record.add_item(
                    action.name().to_string(),
                    0,
                    ActionType::ExternalCommand,
                    ActionStatus::Skipped("administrator authentication was not granted".into()),
                    None,
                );
                continue;
            }

            match action.execute() {
                Ok(outcome) => {
                    let commands_label = outcome.commands_run.join(" && ");
                    match &outcome.status {
                        ActionStatus::Success => succeeded += 1,
                        ActionStatus::Skipped(_) => skipped += 1,
                        ActionStatus::Failed(_) => failed += 1,
                    }

                    // Diagnostic actions (SMART status, APFS verify, …) are only
                    // useful if their captured stdout actually reaches the user,
                    // not just a "succeeded" tally.
                    if let Some(output) = &outcome.output {
                        println!("   ℹ️  {}: {}", outcome.action_name.bold(), output);
                    }

                    let logged_label = match &outcome.output {
                        Some(output) => {
                            format!("{}: {} -> {}", outcome.action_name, commands_label, output)
                        }
                        None => format!("{}: {}", outcome.action_name, commands_label),
                    };
                    op_record.add_item(
                        logged_label,
                        0,
                        ActionType::ExternalCommand,
                        outcome.status,
                        None,
                    );
                }
                Err(e) => {
                    failed += 1;
                    op_record.add_item(
                        action.name().to_string(),
                        0,
                        ActionType::ExternalCommand,
                        ActionStatus::Failed(e.to_string()),
                        None,
                    );
                }
            }
        }

        op_record.set_duration(start_time.elapsed());
        let log_path = save_operation_log(&op_record)?;

        println!("\n{}", "🎉 Maintenance run completed.".bold().green());
        println!("--------------------------------------------------");
        println!("✅ Succeeded:        {}", succeeded.to_string().green());
        if skipped > 0 {
            println!(
                "⏭️  Skipped:          {} {}",
                skipped.to_string().yellow(),
                "(a safety precondition wasn't met — see Status above)".dimmed()
            );
        }
        if failed > 0 {
            println!("⚠️  Failed:           {}", failed.to_string().red());
        }
        println!(
            "📝 Audit Log Saved:  {}",
            log_path.display().to_string().dimmed()
        );
        println!("--------------------------------------------------");

        Ok(())
    }
}
