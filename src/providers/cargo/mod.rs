pub mod parser;
pub mod registry;

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::core::models::{Dependency, Ecosystem};
use crate::core::provider::Provider;

/// Cargo package provider.
///
/// Detects Cargo.toml files and optionally reads Cargo.lock for resolved versions.
/// Queries the crates.io API for latest stable versions.
pub struct CargoProvider;

impl CargoProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Provider for CargoProvider {
    fn name(&self) -> &str {
        "Cargo"
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::Cargo
    }

    fn detect(&self, project_path: &Path) -> Option<Vec<PathBuf>> {
        let manifest_path = project_path.join("Cargo.toml");
        if manifest_path.exists() {
            Some(vec![manifest_path])
        } else {
            None
        }
    }

    fn parse_dependencies(
        &self,
        project_path: &Path,
        manifest_files: &[PathBuf],
    ) -> Result<Vec<Dependency>> {
        parser::parse_cargo_manifests(project_path, manifest_files)
    }

    fn get_latest_version(
        &self,
        agent: &ureq::Agent,
        package_name: &str,
    ) -> Result<Option<String>> {
        registry::get_latest_stable_version(agent, package_name)
    }
}
