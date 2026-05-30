use crate::core::provider::{Provider, ProviderRegistry};
use crate::providers;
use anyhow::{Context, Result};
use std::path::{Path, PathBuf};

pub mod models;
pub mod orchestrator;
pub mod provider;

pub fn init_provider<'a>(
    path: &Path,
    registry: &'a mut ProviderRegistry,
) -> Result<(PathBuf, Vec<&'a dyn Provider>)> {
    // Resolve to absolute path.
    let project_path = path
        .canonicalize()
        .with_context(|| format!("Path '{}' does not exist", path.display()))?;

    // Build the provider registry with all available providers.
    providers::register_all_providers(registry);

    let detected = registry.detect_providers(&project_path);

    if detected.is_empty() {
        anyhow::bail!(
            "No supported project files found in '{}'.\n\
             Supported files: *.csproj, Directory.Packages.props, Cargo.toml",
            project_path.display()
        );
    }

    Ok((project_path, detected))
}
