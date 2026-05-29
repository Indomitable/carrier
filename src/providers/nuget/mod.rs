pub mod parser;
pub mod registry;

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::core::models::{Dependency, Ecosystem};
use crate::core::provider::Provider;

/// NuGet package provider.
///
/// Detects .csproj files and Directory.Packages.props in the project directory.
/// Queries the NuGet v3 flat container API for latest versions.
pub struct NuGetProvider;

impl NuGetProvider {
    pub fn new() -> Self {
        Self
    }
}

impl Provider for NuGetProvider {
    fn name(&self) -> &str {
        "NuGet"
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::NuGet
    }

    fn detect(&self, project_path: &Path) -> Option<Vec<PathBuf>> {
        let mut manifest_files = Vec::new();

        // Check for Directory.Packages.props (Central Package Management)
        let props_path = project_path.join("Directory.Packages.props");
        if props_path.exists() {
            manifest_files.push(props_path);
        }

        // Check for .csproj files
        if let Ok(entries) = std::fs::read_dir(project_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Some(ext) = path.extension() {
                        if ext.eq_ignore_ascii_case("csproj") {
                            manifest_files.push(path);
                        }
                    }
                }
            }
        }

        if manifest_files.is_empty() {
            None
        } else {
            Some(manifest_files)
        }
    }

    fn parse_dependencies(
        &self,
        _project_path: &Path,
        manifest_files: &[PathBuf],
    ) -> Result<Vec<Dependency>> {
        parser::parse_nuget_manifests(_project_path, manifest_files)
    }

    fn get_latest_version(
        &self,
        agent: &ureq::Agent,
        package_name: &str,
    ) -> Result<Option<String>> {
        registry::get_latest_stable_version(agent, package_name)
    }
}
