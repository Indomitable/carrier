mod cli;
mod core;
mod output;
mod providers;

use std::process;

use anyhow::{Context, Result};
use clap::Parser;

use cli::{Cli, Commands};
use core::orchestrator;
use core::provider::ProviderRegistry;

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Outdated { path } => {
            run_outdated(&path)?;
        }
    }

    Ok(())
}

fn run_outdated(path: &std::path::Path) -> Result<()> {
    // Resolve to absolute path.
    let project_path = path
        .canonicalize()
        .with_context(|| format!("Path '{}' does not exist", path.display()))?;

    // Build the provider registry with all available providers.
    let mut registry = ProviderRegistry::new();
    providers::register_all_providers(&mut registry);

    // Run the outdated check.
    let outdated = orchestrator::run_outdated(&registry, &project_path)?;

    // Display results.
    output::table::print_outdated_table(&outdated);

    // Exit with code 1 if there are outdated dependencies.
    if !outdated.is_empty() {
        process::exit(1);
    }

    Ok(())
}
