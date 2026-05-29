use std::path::PathBuf;

use clap::{Parser, Subcommand};

/// Carrier — Universal package outdated checker.
#[derive(Parser)]
#[command(
    name = "carrier",
    version,
    about = "Check for outdated dependencies across multiple package ecosystems"
)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Check for outdated dependencies in the current project.
    Outdated {
        /// Path to the project directory (defaults to current directory).
        #[arg(short, long, default_value = ".")]
        path: PathBuf,
    },
}
