use crate::projects::models::{DiscoveredProject, ProjectArtifact};
use crate::utils::format::format_bytes;
use crate::utils::fs::contract_tilde;
use crate::utils::table::{create_styled_table_with_width, get_terminal_width};
use anyhow::Result;
use colored::*;
use comfy_table::{Cell, Color, Row};
use dialoguer::{Confirm, MultiSelect};

/// Renders a formatted CLI table showing all discovered projects and their cleanable artifacts.
pub fn render_projects_table(projects: &[DiscoveredProject]) -> String {
    render_projects_table_with_width(projects, get_terminal_width())
}

/// Renders projects table with an explicit terminal width constraint.
pub fn render_projects_table_with_width(projects: &[DiscoveredProject], width: u16) -> String {
    if projects.is_empty() {
        return "✨ No software projects with cleanable build artifacts found."
            .green()
            .to_string();
    }

    let mut table = create_styled_table_with_width(width);
    let is_compact = width < 100;

    if is_compact {
        table.set_header(vec![
            Cell::new("Project & Ecosystem").fg(Color::Cyan),
            Cell::new("Status & Activity").fg(Color::Yellow),
            Cell::new("Build Artifacts").fg(Color::Magenta),
            Cell::new("Reclaimable").fg(Color::Green),
        ]);
    } else {
        table.set_header(vec![
            Cell::new("Status").fg(Color::Cyan),
            Cell::new("Ecosystem").fg(Color::Yellow),
            Cell::new("Project & Location").fg(Color::White),
            Cell::new("Build Artifacts").fg(Color::Magenta),
            Cell::new("Reclaimable").fg(Color::Green),
            Cell::new("Last Active").fg(Color::DarkYellow),
        ]);
    }

    let mut total_dormant_bytes = 0u64;
    let mut total_active_bytes = 0u64;
    let mut dormant_count = 0usize;

    for p in projects {
        if p.is_dormant {
            total_dormant_bytes += p.total_reclaimable_bytes;
            dormant_count += 1;
        } else {
            total_active_bytes += p.total_reclaimable_bytes;
        }

        let contracted_path = contract_tilde(&p.root_path);
        let artifacts_summary = p
            .artifacts
            .iter()
            .map(|a| format!("📁 {} ({})", a.name, format_bytes(a.size_bytes)))
            .collect::<Vec<_>>()
            .join("\n");

        if is_compact {
            let project_info = format!(
                "{} {}\n{}",
                p.project_type.icon(),
                p.name.bold(),
                contracted_path.dimmed()
            );

            let status_info = if p.is_dormant {
                let days_str = p
                    .days_inactive
                    .map(|d| format!("💤 Dormant ({}d)", d))
                    .unwrap_or_else(|| "💤 Dormant".into());
                let date_str = p
                    .last_modified
                    .map(|lm| lm.format("%Y-%m-%d").to_string())
                    .unwrap_or_default();
                if date_str.is_empty() {
                    days_str
                } else {
                    format!("{}\n{}", days_str, date_str.dimmed())
                }
            } else {
                let days_str = p
                    .days_inactive
                    .map(|d| format!("⚡ Active ({}d)", d))
                    .unwrap_or_else(|| "⚡ Active".into());
                let date_str = p
                    .last_modified
                    .map(|lm| lm.format("%Y-%m-%d").to_string())
                    .unwrap_or_default();
                if date_str.is_empty() {
                    days_str
                } else {
                    format!("{}\n{}", days_str, date_str.dimmed())
                }
            };

            table.add_row(Row::from(vec![
                Cell::new(project_info),
                Cell::new(status_info).fg(if p.is_dormant {
                    Color::Yellow
                } else {
                    Color::Green
                }),
                Cell::new(artifacts_summary),
                Cell::new(format_bytes(p.total_reclaimable_bytes)).fg(Color::Green),
            ]));
        } else {
            let status_cell = if p.is_dormant {
                let days_str = p
                    .days_inactive
                    .map(|d| format!("💤 Dormant ({}d)", d))
                    .unwrap_or_else(|| "💤 Dormant".into());
                Cell::new(days_str).fg(Color::Yellow)
            } else {
                let days_str = p
                    .days_inactive
                    .map(|d| format!("⚡ Active ({}d)", d))
                    .unwrap_or_else(|| "⚡ Active".into());
                Cell::new(days_str).fg(Color::Green)
            };

            let type_cell =
                Cell::new(format!("{} {}", p.project_type.icon(), p.project_type.name()));

            let project_cell =
                Cell::new(format!("{}\n{}", p.name.bold(), contracted_path.dimmed()));

            let size_cell = Cell::new(format_bytes(p.total_reclaimable_bytes)).fg(Color::Green);

            let last_active_str = match (p.last_modified, p.days_inactive) {
                (Some(lm), Some(days)) => format!("{} ({}d ago)", lm.format("%Y-%m-%d"), days),
                (Some(lm), None) => lm.format("%Y-%m-%d").to_string(),
                _ => "Unknown".to_string(),
            };
            let last_active_cell = Cell::new(last_active_str).fg(Color::DarkYellow);

            table.add_row(Row::from(vec![
                status_cell,
                type_cell,
                project_cell,
                Cell::new(artifacts_summary),
                size_cell,
                last_active_cell,
            ]));
        }
    }

    let summary = format!(
        "\n📊 Summary: {} project(s) found | 💤 Dormant: {} ({}) | ⚡ Active: {} ({})",
        projects.len().to_string().bold(),
        dormant_count.to_string().yellow().bold(),
        format_bytes(total_dormant_bytes).yellow().bold(),
        (projects.len() - dormant_count).to_string().green(),
        format_bytes(total_active_bytes).green()
    );

    format!("{}{}", table, summary)
}


/// Flattened item for interactive selection
pub struct SelectableArtifact<'a> {
    pub project: &'a DiscoveredProject,
    pub artifact: &'a ProjectArtifact,
}

/// Prompt interactive checkbox selection for project artifacts to clean.
pub fn select_artifacts_interactive<'a>(
    projects: &'a [DiscoveredProject],
) -> Result<Vec<(&'a DiscoveredProject, &'a ProjectArtifact)>> {
    let mut flat_items: Vec<SelectableArtifact<'a>> = Vec::new();
    let mut item_labels: Vec<String> = Vec::new();
    let mut defaults: Vec<bool> = Vec::new();

    for p in projects {
        for a in &p.artifacts {
            let status_badge = if p.is_dormant {
                format!("[💤 {}d]", p.days_inactive.unwrap_or(0))
            } else {
                format!("[⚡ {}d]", p.days_inactive.unwrap_or(0))
            };

            let label = format!(
                "{} {} {} -> {}/{} ({})",
                p.project_type.icon(),
                status_badge,
                p.name.bold(),
                contract_tilde(&p.root_path),
                a.name.cyan(),
                format_bytes(a.size_bytes).green().bold()
            );

            item_labels.push(label);
            // Default to selected only if dormant
            defaults.push(p.is_dormant);
            flat_items.push(SelectableArtifact {
                project: p,
                artifact: a,
            });
        }
    }

    if flat_items.is_empty() {
        return Ok(Vec::new());
    }

    println!(
        "\n{}",
        "🔘 Space = toggle selection, Enter = confirm, A = all, Esc = cancel"
            .dimmed()
    );

    let selections = MultiSelect::new()
        .with_prompt("Select project build artifacts to clean")
        .items(&item_labels)
        .defaults(&defaults)
        .interact_opt()?;

    let selected_indices = match selections {
        Some(s) => s,
        None => return Ok(Vec::new()),
    };

    let result = selected_indices
        .into_iter()
        .map(|idx| (flat_items[idx].project, flat_items[idx].artifact))
        .collect();

    Ok(result)
}

/// Prompts user with a confirmation dialog before executing cleaning.
pub fn prompt_confirm_clean(
    targets: &[(&DiscoveredProject, &ProjectArtifact)],
    force_permanent: bool,
) -> Result<bool> {
    if targets.is_empty() {
        return Ok(false);
    }

    let total_size: u64 = targets.iter().map(|(_, a)| a.size_bytes).sum();

    println!("\n{}", "⚠️ Target build artifacts to clean:".bold().yellow());
    for (p, a) in targets {
        println!(
            "  • {} {} -> {}/{} ({})",
            p.project_type.icon(),
            p.name.bold(),
            contract_tilde(&p.root_path).dimmed(),
            a.name.cyan(),
            format_bytes(a.size_bytes).green()
        );
    }

    let prompt_msg = if force_permanent {
        format!(
            "⚠️ PERMANENT DELETION: Permanently delete {} folder(s) ({})?",
            targets.len(),
            format_bytes(total_size)
        )
    } else {
        format!(
            "Move {} build artifact folder(s) ({}) to macOS Trash?",
            targets.len(),
            format_bytes(total_size)
        )
    };

    let confirmed = Confirm::new()
        .with_prompt(prompt_msg)
        .default(false)
        .interact()
        .unwrap_or(false);

    Ok(confirmed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::projects::models::ProjectType;
    use std::path::PathBuf;

    #[test]
    fn test_render_projects_table_empty() {
        let output = render_projects_table(&[]);
        assert!(output.contains("No software projects"));
    }

    #[test]
    fn test_render_projects_table_compact_and_wide() {
        let sample = vec![DiscoveredProject {
            name: "website".into(),
            root_path: PathBuf::from("/Users/fabrizio.riccardo/development/plastic-free/website"),
            project_type: ProjectType::Node,
            is_dormant: true,
            days_inactive: Some(937),
            last_modified: Some(chrono::Local::now()),
            artifacts: vec![ProjectArtifact {
                name: ".next".into(),
                path: PathBuf::from(
                    "/Users/fabrizio.riccardo/development/plastic-free/website/.next",
                ),
                size_bytes: 1_660_000_000,
                file_count: 50,
                description: "Next.js build cache".into(),
            }],
            total_reclaimable_bytes: 1_660_000_000,
        }];

        // Compact (< 100 columns)
        let compact_output = render_projects_table_with_width(&sample, 80);
        assert!(compact_output.contains("Project & Ecosystem"));
        assert!(compact_output.contains("website"));
        assert!(compact_output.contains("1.66 GB") || compact_output.contains("1.7"));

        // Wide (>= 100 columns)
        let wide_output = render_projects_table_with_width(&sample, 120);
        assert!(wide_output.contains("Status"));
        assert!(wide_output.contains("Ecosystem"));
        assert!(wide_output.contains("Project & Location"));
        assert!(wide_output.contains("website"));
    }
}

