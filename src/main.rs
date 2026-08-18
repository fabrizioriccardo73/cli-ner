mod app;
mod cleaner;
mod cli;
mod docker;
mod report;
mod safety;
mod scanner;
mod tui;
mod utils;

use anyhow::Result;
use app::App;
use clap::Parser;
use cli::{Cli, Commands};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Initialize tracing / logging
    let filter_level = if cli.verbose { "debug" } else { "warn" };
    let subscriber = tracing_subscriber::registry()
        .with(EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(filter_level)))
        .with(tracing_subscriber::fmt::layer());

    let _ = subscriber.try_init();

    let app = App::new();

    match cli.command {
        Commands::Scan(args) => app.handle_scan(args)?,
        Commands::Clean(args) => app.handle_clean(args)?,
        Commands::Report(args) => app.handle_report(args)?,
        Commands::Dashboard(args) => app.handle_dashboard(args)?,
        Commands::Doctor(args) => app.handle_doctor(args)?,
        Commands::Docker(args) => app.handle_docker(args)?,
    }

    Ok(())
}
