use crate::docker::client::DockerClient;
use crate::docker::models::{DockerContainer, DockerImage, DockerSystemDf, DockerVolume};
use crate::utils::format::format_bytes;
use anyhow::Result;
use colored::*;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, Row, Table};
use dialoguer::{Confirm, MultiSelect, Select};

pub struct DockerInteractive;

impl DockerInteractive {
    /// Renders a formatted Docker system disk usage summary table
    pub fn render_df_table(df: &DockerSystemDf) -> String {
        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_header(vec![
                Cell::new("Docker Component").fg(Color::Cyan),
                Cell::new("Total Count").fg(Color::White),
                Cell::new("Active / In-Use").fg(Color::Green),
                Cell::new("Total Size").fg(Color::Yellow),
                Cell::new("Reclaimable Size").fg(Color::Magenta),
            ]);

        table.add_row(Row::from(vec![
            Cell::new("🖼️  Images (Layers & Tags)").fg(Color::Cyan),
            Cell::new(df.images_total.to_string()),
            Cell::new(format!("{} active", df.images_active)).fg(Color::Green),
            Cell::new(&df.images_size_str),
            Cell::new(&df.images_reclaimable_str).fg(Color::Magenta),
        ]));

        table.add_row(Row::from(vec![
            Cell::new("📦 Containers").fg(Color::Cyan),
            Cell::new(df.containers_total.to_string()),
            Cell::new(format!("{} running", df.containers_active)).fg(Color::Green),
            Cell::new(&df.containers_size_str),
            Cell::new(&df.containers_reclaimable_str).fg(Color::Magenta),
        ]));

        table.add_row(Row::from(vec![
            Cell::new("💾 Local Volumes (Persistent Data)").fg(Color::Cyan),
            Cell::new(df.volumes_total.to_string()),
            Cell::new(format!("{} attached", df.volumes_active)).fg(Color::Green),
            Cell::new(&df.volumes_size_str),
            Cell::new(&df.volumes_reclaimable_str).fg(Color::Magenta),
        ]));

        table.add_row(Row::from(vec![
            Cell::new("🔨 Build Cache (BuildKit)").fg(Color::Cyan),
            Cell::new(df.build_cache_total.to_string()),
            Cell::new(format!("{} active", df.build_cache_active)).fg(Color::Green),
            Cell::new(&df.build_cache_size_str),
            Cell::new(&df.build_cache_reclaimable_str).fg(Color::Magenta),
        ]));

        let total_reclaimable_bytes = df.images_reclaimable_bytes
            + df.containers_reclaimable_bytes
            + df.build_cache_reclaimable_bytes;

        format!(
            "{}\n📊 {} {}",
            table,
            "Total Potential Reclaimable Space (excluding volumes):"
                .bold()
                .white(),
            format_bytes(total_reclaimable_bytes).bold().green()
        )
    }

    /// Renders the containers table with safety indicators
    pub fn render_containers_table(containers: &[DockerContainer]) -> String {
        if containers.is_empty() {
            return "No Docker containers found on system.".dimmed().to_string();
        }

        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_header(vec![
                Cell::new("Container ID").fg(Color::Cyan),
                Cell::new("Name").fg(Color::White),
                Cell::new("Image").fg(Color::Yellow),
                Cell::new("State / Status").fg(Color::Green),
                Cell::new("Size (RW)").fg(Color::Magenta),
                Cell::new("Mounted Volumes & Data").fg(Color::DarkYellow),
            ]);

        for c in containers {
            let short_id = if c.id.len() >= 12 { &c.id[..12] } else { &c.id };

            let state_cell = if c.is_running {
                Cell::new(format!("🟢 RUNNING ({})", c.status)).fg(Color::Green)
            } else {
                Cell::new(format!("🟡 STOPPED ({})", c.status)).fg(Color::Yellow)
            };

            let mounts_summary = if c.mounts.is_empty() {
                "No persistent volumes".dimmed().to_string()
            } else {
                let list: Vec<String> = c
                    .mounts
                    .iter()
                    .map(|m| {
                        if let Some(ref name) = m.name {
                            format!("vol:{}", name)
                        } else {
                            format!("bind:{}", m.source)
                        }
                    })
                    .collect();
                list.join(", ")
            };

            table.add_row(Row::from(vec![
                Cell::new(short_id),
                Cell::new(&c.name).fg(if c.is_running {
                    Color::Green
                } else {
                    Color::White
                }),
                Cell::new(&c.image),
                state_cell,
                Cell::new(&c.size_str),
                Cell::new(mounts_summary),
            ]));
        }

        table.to_string()
    }

    /// Renders the images table with usage protection flags
    pub fn render_images_table(images: &[DockerImage]) -> String {
        if images.is_empty() {
            return "No Docker images found on system.".dimmed().to_string();
        }

        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_header(vec![
                Cell::new("Image ID").fg(Color::Cyan),
                Cell::new("Repository:Tag").fg(Color::White),
                Cell::new("Size").fg(Color::Green),
                Cell::new("Created").fg(Color::Yellow),
                Cell::new("Safety / Usage Status").fg(Color::Magenta),
            ]);

        for img in images {
            let short_id = if img.id.len() >= 12 {
                &img.id[..12]
            } else {
                &img.id
            };

            let status_cell = if img.is_in_use() {
                let containers = img.in_use_by.join(", ");
                Cell::new(format!("🔒 IN-USE (by: {})", containers)).fg(Color::Green)
            } else if img.is_dangling {
                Cell::new("🧹 DANGLING (Safe to prune)").fg(Color::Magenta)
            } else {
                Cell::new("📦 UNUSED (Tagged image)").fg(Color::Yellow)
            };

            table.add_row(Row::from(vec![
                Cell::new(short_id),
                Cell::new(img.display_name()),
                Cell::new(&img.size_str).fg(Color::Green),
                Cell::new(&img.created_since),
                status_cell,
            ]));
        }

        table.to_string()
    }

    /// Renders the volumes table with attachment and data risk notices
    pub fn render_volumes_table(volumes: &[DockerVolume]) -> String {
        if volumes.is_empty() {
            return "No Docker local volumes found on system."
                .dimmed()
                .to_string();
        }

        let mut table = Table::new();
        table
            .load_preset(UTF8_FULL)
            .apply_modifier(UTF8_ROUND_CORNERS)
            .set_header(vec![
                Cell::new("Volume Name").fg(Color::Cyan),
                Cell::new("Driver").fg(Color::White),
                Cell::new("Status").fg(Color::Yellow),
                Cell::new("Attached Container").fg(Color::Green),
                Cell::new("Data Safety Risk").fg(Color::Red),
            ]);

        for v in volumes {
            let in_use = v.is_in_use();
            let status_cell = if in_use {
                Cell::new("🟢 ACTIVE (In-Use)").fg(Color::Green)
            } else {
                Cell::new("🟡 UNATTACHED / ORPHAN").fg(Color::Yellow)
            };

            let attached_str = if in_use {
                v.used_by.join(", ")
            } else {
                "None (orphaned volume)".dimmed().to_string()
            };

            let risk_str = if in_use {
                "🛑 CRITICAL: Contains active DB/app data".to_string()
            } else {
                "⚠️ CAUTION: May contain persistent db backups/data".to_string()
            };

            table.add_row(Row::from(vec![
                Cell::new(&v.name),
                Cell::new(&v.driver),
                status_cell,
                Cell::new(attached_str),
                Cell::new(risk_str).fg(if in_use { Color::Red } else { Color::Yellow }),
            ]));
        }

        table.to_string()
    }

    /// Main interactive Docker dashboard and management menu
    pub fn run_interactive_menu() -> Result<()> {
        if !DockerClient::is_available() {
            println!(
                "{}",
                "❌ Docker daemon is not running or Docker CLI is not accessible."
                    .bold()
                    .red()
            );
            println!(
                "{}",
                "Please start Docker Desktop or the Docker daemon and try again.".yellow()
            );
            return Ok(());
        }

        loop {
            let df = DockerClient::get_system_df()?;
            println!(
                "\n{}",
                "🐳 CLI-NER DOCKER MANAGEMENT & SAFETY WIZARD".bold().cyan()
            );
            println!("{}\n", "=".repeat(60).dimmed());
            println!("{}", Self::render_df_table(&df));
            println!("\n{}", "Select an action:".bold().white());

            let options = vec![
                "🚀 1. Safe Guided Cleanup Wizard (Step-by-step recommendation)",
                "📦 2. Manage & Clean Containers (Inspect mounts & stopped containers)",
                "🖼️  3. Manage & Clean Images (Dangling & unused images)",
                "🔨 4. Purge BuildKit Build Cache",
                "💾 5. Volume Safety Audit & Persistent Data Inspector",
                "📊 6. Show Full Storage Breakdown (docker system df)",
                "❌ 0. Exit",
            ];

            let selection = Select::new()
                .items(&options)
                .default(0)
                .interact()
                .unwrap_or(6);

            match selection {
                0 => {
                    Self::run_guided_cleanup_wizard(false)?;
                }
                1 => {
                    Self::manage_containers_interactive(false)?;
                }
                2 => {
                    Self::manage_images_interactive(false)?;
                }
                3 => {
                    Self::clean_build_cache_interactive(false)?;
                }
                4 => {
                    Self::audit_volumes_interactive()?;
                }
                5 => {
                    let df = DockerClient::get_system_df()?;
                    println!("\n{}", Self::render_df_table(&df));
                    let containers = DockerClient::list_containers()?;
                    println!(
                        "\n📦 Containers Overview:\n{}",
                        Self::render_containers_table(&containers)
                    );
                }
                _ => {
                    println!("{}", "\n👋 Exiting Docker Manager.".cyan());
                    break;
                }
            }
        }

        Ok(())
    }

    /// Step-by-step guided cleanup wizard
    pub fn run_guided_cleanup_wizard(dry_run: bool) -> Result<()> {
        println!("\n{}", "🧙 GUIDED DOCKER SAFE CLEANUP WIZARD".bold().cyan());
        println!(
            "{}",
            "We will safely inspect and clean Docker components one step at a time.".white()
        );
        println!(
            "{}",
            "⚠️ Note: Running containers, in-use images, and persistent volumes are STRICTLY PROTECTED.\n"
                .bold()
                .green()
        );

        // Step 1: Stopped Containers
        println!("{}", "--- Step 1/3: Stopped Containers ---".bold().yellow());
        let containers = DockerClient::list_containers()?;
        let stopped_containers: Vec<&DockerContainer> =
            containers.iter().filter(|c| !c.is_running).collect();

        if stopped_containers.is_empty() {
            println!(
                "{}",
                "✅ No stopped containers found. Active containers are safely running.".green()
            );
        } else {
            println!(
                "Found {} stopped container(s).",
                stopped_containers.len().to_string().bold().yellow()
            );
            Self::manage_containers_interactive(dry_run)?;
        }

        // Step 2: Build Cache
        println!(
            "\n{}",
            "--- Step 2/3: BuildKit Build Cache ---".bold().yellow()
        );
        let df = DockerClient::get_system_df()?;
        if df.build_cache_total == 0 || df.build_cache_size_str == "0B" {
            println!("{}", "✅ Build cache is already clean.".green());
        } else {
            println!(
                "Build cache has {} entries occupying {}.",
                df.build_cache_total.to_string().bold().cyan(),
                df.build_cache_size_str.bold().yellow()
            );
            let prune_cache = Confirm::new()
                .with_prompt("Purge BuildKit build cache? (100% safe, only rebuilds when needed)")
                .default(true)
                .interact()
                .unwrap_or(false);

            if prune_cache {
                if dry_run {
                    println!(
                        "{}",
                        "ℹ️ [DRY-RUN] Would run `docker builder prune -f`".yellow()
                    );
                } else {
                    println!("{}", "🧹 Purging build cache...".cyan());
                    match DockerClient::prune_build_cache() {
                        Ok(msg) => println!("{} {}", "✅ Success:".green(), msg),
                        Err(e) => println!("{} {}", "❌ Error:".red(), e),
                    }
                }
            }
        }

        // Step 3: Images (Dangling & Unused)
        println!("\n{}", "--- Step 3/3: Images Cleanup ---".bold().yellow());
        let images = DockerClient::list_images()?;
        let dangling_count = images.iter().filter(|i| i.is_dangling).count();
        let unused_count = images
            .iter()
            .filter(|i| !i.is_dangling && !i.is_in_use())
            .count();

        println!(
            "Images found: {} total (🔒 {} in-use, 🧹 {} dangling, 📦 {} unused tagged).",
            images.len().to_string().bold().white(),
            images
                .iter()
                .filter(|i| i.is_in_use())
                .count()
                .to_string()
                .bold()
                .green(),
            dangling_count.to_string().bold().magenta(),
            unused_count.to_string().bold().yellow()
        );

        if dangling_count > 0 {
            let prune_dangling = Confirm::new()
                .with_prompt(format!(
                    "Prune {} dangling (<none>:<none>) image layers? (100% safe)",
                    dangling_count
                ))
                .default(true)
                .interact()
                .unwrap_or(false);

            if prune_dangling {
                if dry_run {
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
            }
        }

        if unused_count > 0 {
            let manage_unused = Confirm::new()
                .with_prompt(format!(
                    "Would you like to review and selectively remove any of the {} unused tagged images?",
                    unused_count
                ))
                .default(false)
                .interact()
                .unwrap_or(false);

            if manage_unused {
                Self::manage_images_interactive(dry_run)?;
            }
        }

        println!("\n{}", "✨ Guided Docker cleanup completed!".bold().green());
        Ok(())
    }

    /// Interactive container management
    pub fn manage_containers_interactive(dry_run: bool) -> Result<()> {
        let containers = DockerClient::list_containers()?;

        if containers.is_empty() {
            println!("{}", "No containers found on system.".dimmed());
            return Ok(());
        }

        println!("\n{}", "📦 Current Containers:".bold().cyan());
        println!("{}", Self::render_containers_table(&containers));

        let stopped_containers: Vec<&DockerContainer> =
            containers.iter().filter(|c| !c.is_running).collect();

        if stopped_containers.is_empty() {
            println!(
                "\n{}",
                "🟢 All containers are currently RUNNING and protected. No stopped containers to remove."
                    .bold()
                    .green()
            );
            return Ok(());
        }

        println!(
            "\n{}",
            "Select stopped containers to REMOVE (Space to toggle, Enter to confirm):"
                .bold()
                .yellow()
        );

        let items: Vec<String> = stopped_containers
            .iter()
            .map(|c| {
                let short_id = if c.id.len() >= 12 { &c.id[..12] } else { &c.id };
                let mounts_info = if c.mounts.is_empty() {
                    "No volumes".to_string()
                } else {
                    format!("⚠️ Has {} mounts", c.mounts.len())
                };
                format!(
                    "[{}] {} ({}) - Size: {} - {}",
                    short_id, c.name, c.image, c.size_str, mounts_info
                )
            })
            .collect();

        let selections = MultiSelect::new()
            .items(&items)
            .interact()
            .unwrap_or_default();

        if selections.is_empty() {
            println!("{}", "No containers selected for deletion.".cyan());
            return Ok(());
        }

        let selected_containers: Vec<&&DockerContainer> = selections
            .iter()
            .map(|&idx| &stopped_containers[idx])
            .collect();

        // Safety check for attached volumes
        let mut has_attached_mounts = false;
        println!("\n{}", "⚠️ Containers to be removed:".bold().yellow());
        for c in &selected_containers {
            println!(
                "  • {} ({}) [ID: {}]",
                c.name.bold(),
                c.image,
                &c.id[..c.id.len().min(12)]
            );
            if !c.mounts.is_empty() {
                has_attached_mounts = true;
                for m in &c.mounts {
                    println!(
                        "    └─ Mounted {}: {} -> {}",
                        m.mount_type.yellow(),
                        m.source.cyan(),
                        m.destination.dimmed()
                    );
                }
            }
        }

        if has_attached_mounts {
            println!(
                "\n{}",
                "⚠️ DATA NOTICE: Docker persistent volumes will NOT be deleted when removing these containers,".yellow().bold()
            );
            println!(
                "{}",
                "   however any uncommitted files inside the container's own writable layer will be lost.".yellow()
            );
        }

        let confirm_prompt = format!(
            "Permanently remove {} stopped container(s)?",
            selected_containers.len()
        );

        let confirmed = Confirm::new()
            .with_prompt(confirm_prompt)
            .default(false)
            .interact()
            .unwrap_or(false);

        if !confirmed {
            println!("{}", "Operation cancelled.".cyan());
            return Ok(());
        }

        let ids_to_remove: Vec<String> = selected_containers.iter().map(|c| c.id.clone()).collect();

        if dry_run {
            println!(
                "{}",
                format!(
                    "ℹ️ [DRY-RUN] Would remove container IDs: {:?}",
                    ids_to_remove
                )
                .yellow()
            );
        } else {
            println!("{}", "🗑️ Removing selected containers...".cyan());
            match DockerClient::remove_containers(&ids_to_remove) {
                Ok(removed) => {
                    println!(
                        "{}",
                        format!("✅ Successfully removed {} container(s):", removed.len())
                            .bold()
                            .green()
                    );
                    for id in removed {
                        println!("  - {}", id);
                    }
                }
                Err(e) => {
                    println!("{} {}", "❌ Error removing containers:".red(), e);
                }
            }
        }

        Ok(())
    }

    /// Interactive image management
    pub fn manage_images_interactive(dry_run: bool) -> Result<()> {
        let images = DockerClient::list_images()?;

        if images.is_empty() {
            println!("{}", "No images found on system.".dimmed());
            return Ok(());
        }

        println!("\n{}", "🖼️ Current Docker Images:".bold().cyan());
        println!("{}", Self::render_images_table(&images));

        let unused_images: Vec<&DockerImage> = images.iter().filter(|i| !i.is_in_use()).collect();

        if unused_images.is_empty() {
            println!(
                "\n{}",
                "🔒 All images are currently IN-USE by existing containers. No images available to safely delete."
                    .bold()
                    .green()
            );
            return Ok(());
        }

        println!(
            "\n{}",
            "Select UNUSED images to REMOVE (Space to toggle, Enter to confirm):"
                .bold()
                .yellow()
        );
        println!(
            "{}",
            "🔒 Note: Images used by containers are hidden from selection to guarantee safety.\n"
                .bold()
                .green()
        );

        let items: Vec<String> = unused_images
            .iter()
            .map(|img| {
                let tag_type = if img.is_dangling {
                    "🧹 [DANGLING]".magenta()
                } else {
                    "📦 [UNUSED]".yellow()
                };
                let short_id = if img.id.len() >= 12 {
                    &img.id[..12]
                } else {
                    &img.id
                };
                format!(
                    "{} {} ({}) - Size: {} - Created: {}",
                    tag_type,
                    img.display_name().bold(),
                    short_id,
                    img.size_str.green(),
                    img.created_since.dimmed()
                )
            })
            .collect();

        let selections = MultiSelect::new()
            .items(&items)
            .interact()
            .unwrap_or_default();

        if selections.is_empty() {
            println!("{}", "No images selected for deletion.".cyan());
            return Ok(());
        }

        let selected_images: Vec<&&DockerImage> =
            selections.iter().map(|&idx| &unused_images[idx]).collect();

        let total_size: u64 = selected_images.iter().map(|i| i.size_bytes).sum();

        println!("\n{}", "Images to be removed:".bold().yellow());
        for img in &selected_images {
            println!(
                "  • {} [ID: {}] ({})",
                img.display_name().bold(),
                &img.id[..img.id.len().min(12)],
                img.size_str
            );
        }

        println!(
            "\n📊 Estimated space to reclaim: {}\n",
            format_bytes(total_size).bold().green()
        );

        let confirm_prompt = format!("Permanently delete {} image(s)?", selected_images.len());

        let confirmed = Confirm::new()
            .with_prompt(confirm_prompt)
            .default(false)
            .interact()
            .unwrap_or(false);

        if !confirmed {
            println!("{}", "Operation cancelled.".cyan());
            return Ok(());
        }

        let targets_to_remove: Vec<String> = selected_images
            .iter()
            .map(|i| {
                if i.is_dangling {
                    i.id.clone()
                } else {
                    format!("{}:{}", i.repository, i.tag)
                }
            })
            .collect();

        if dry_run {
            println!(
                "{}",
                format!(
                    "ℹ️ [DRY-RUN] Would run `docker rmi` on: {:?}",
                    targets_to_remove
                )
                .yellow()
            );
        } else {
            println!("{}", "🗑️ Removing selected images...".cyan());
            match DockerClient::remove_images(&targets_to_remove) {
                Ok(removed) => {
                    println!(
                        "{}",
                        format!(
                            "✅ Successfully removed {} image tag(s)/layer(s):",
                            removed.len()
                        )
                        .bold()
                        .green()
                    );
                    for line in removed {
                        println!("  - {}", line);
                    }
                }
                Err(e) => {
                    println!("{} {}", "❌ Error removing images:".red(), e);
                }
            }
        }

        Ok(())
    }

    /// Interactive BuildKit cache clean
    pub fn clean_build_cache_interactive(dry_run: bool) -> Result<()> {
        let df = DockerClient::get_system_df()?;
        println!("\n{}", "🔨 BuildKit Build Cache Status:".bold().cyan());
        println!(
            "Total Entries: {} | Size: {}",
            df.build_cache_total.to_string().bold().white(),
            df.build_cache_size_str.bold().yellow()
        );

        let confirmed = Confirm::new()
            .with_prompt(
                "Purge Docker BuildKit build cache? (100% safe, layers are rebuilt as needed)",
            )
            .default(true)
            .interact()
            .unwrap_or(false);

        if !confirmed {
            println!("{}", "Operation cancelled.".cyan());
            return Ok(());
        }

        if dry_run {
            println!(
                "{}",
                "ℹ️ [DRY-RUN] Would execute `docker builder prune -f`".yellow()
            );
        } else {
            println!("{}", "🧹 Purging build cache...".cyan());
            match DockerClient::prune_build_cache() {
                Ok(msg) => println!("{} {}", "✅ Success:".green(), msg),
                Err(e) => println!("{} {}", "❌ Error:".red(), e),
            }
        }

        Ok(())
    }

    /// Volume safety audit
    pub fn audit_volumes_interactive() -> Result<()> {
        let volumes = DockerClient::list_volumes()?;
        println!(
            "\n{}",
            "💾 DOCKER VOLUMES & PERSISTENT DATA SAFETY AUDIT"
                .bold()
                .cyan()
        );
        println!("{}\n", "=".repeat(60).dimmed());

        println!("{}", "🚨 CRITICAL VOLUME SAFETY NOTICE:".bold().red());
        println!(
            "{}",
            "• Docker volumes store databases (PostgreSQL, MongoDB, MySQL, MSSQL) and application state."
                .yellow()
        );
        println!(
            "{}",
            "• Deleting a volume PERMANENTLY DESTROYS the data stored within it.".yellow()
        );
        println!(
            "{}",
            "• CLI-NER NEVER deletes volumes in standard cleanup routines to guarantee 100% data safety.\n"
                .green()
                .bold()
        );

        println!("{}", Self::render_volumes_table(&volumes));

        let active_count = volumes.iter().filter(|v| v.is_in_use()).count();
        let unattached_count = volumes.len() - active_count;

        println!(
            "\n📊 Volume Summary: {} Total | 🟢 {} Attached to containers | 🟡 {} Unattached / Orphaned\n",
            volumes.len().to_string().bold().white(),
            active_count.to_string().bold().green(),
            unattached_count.to_string().bold().yellow()
        );

        Ok(())
    }
}
