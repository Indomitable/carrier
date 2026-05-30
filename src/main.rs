mod cli;
mod core;
mod output;
mod providers;

use std::process;

use anyhow::Result;
use clap::Parser;

use crate::core::init_provider;
use crate::core::provider::ProviderRegistry;
use cli::{Cli, Commands};
use core::orchestrator;

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Outdated { path } => {
            run_outdated(&path)?;
        }
        Commands::Why { package, path } => {
            run_why(&path, &package)?;
        }
    }

    Ok(())
}

fn run_why(path: &std::path::Path, package: &str) -> Result<()> {
    let mut registry = ProviderRegistry::new();
    let (project_path, providers) = init_provider(path, &mut registry)?;

    orchestrator::run_why(providers, &project_path, package)?;

    Ok(())
}

fn run_outdated(path: &std::path::Path) -> Result<()> {
    let mut registry = ProviderRegistry::new();
    let (project_path, providers) = init_provider(path, &mut registry)?;

    // Run the outdated check.
    let outdated = orchestrator::run_outdated(providers, &project_path)?;

    // Display results.
    output::table::print_outdated_table(&outdated);

    // Exit with code 1 if there are outdated dependencies.
    if !outdated.is_empty() {
        process::exit(1);
    }

    Ok(())
}
