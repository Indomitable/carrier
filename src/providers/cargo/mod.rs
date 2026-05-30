pub mod graph;
pub mod parser;
pub mod registry;

use std::path::Path;

use anyhow::Result;

use crate::core::models::{Ecosystem, Project};
use crate::core::provider::Provider;

/// Cargo package provider.
///
/// Detects Cargo.toml files and optionally reads Cargo.lock for resolved versions.
/// Queries the crates.io API for latest stable versions.
pub struct CargoProvider {
    agent: ureq::Agent,
}

impl CargoProvider {
    pub fn new() -> Self {
        let config = ureq::config::Config::builder()
            .user_agent("carrier")
            .timeout_global(Some(std::time::Duration::from_secs(10)))
            .build();
        Self {
            agent: ureq::Agent::new_with_config(config),
        }
    }
}

impl Provider for CargoProvider {
    fn name(&self) -> &str {
        "Cargo"
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::Cargo
    }

    fn detect(&self, project_path: &Path) -> bool {
        project_path.join("Cargo.toml").exists()
    }

    fn get_latest_version(
        &self,
        package_name: &str,
    ) -> Result<Option<String>> {
        registry::get_latest_stable_version(&self.agent, package_name)
    }

    fn is_outdated(&self, declared: &str, resolved: Option<&str>, latest: &str) -> bool {
        let Ok(latest_ver) = semver::Version::parse(latest) else {
            return false;
        };

        if let Some(res) = resolved {
            if let Ok(res_ver) = semver::Version::parse(res) {
                return latest_ver > res_ver;
            }
            return false;
        }

        if let Ok(req) = semver::VersionReq::parse(declared) {
            return !req.matches(&latest_ver);
        }

        false
    }

    fn get_projects(&self, project_path: &Path) -> Result<Vec<Project>> {
        let manifest_file = project_path.join("Cargo.toml");
        if !manifest_file.exists() {
            return Ok(Vec::new());
        }

        let direct_dependencies =
            parser::parse_cargo_manifests(project_path, &[manifest_file.clone()])?;
        let dependencies_graph = graph::build_dependency_graph(project_path)?;
        let name = project_path
            .file_name()
            .map(|s| s.to_string_lossy().to_string())
            .unwrap_or_else(|| "Cargo Project".to_string());

        Ok(vec![Project {
            name,
            manifest_file,
            direct_dependencies,
            dependencies_graph,
        }])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::provider::Provider;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_project_dir() -> std::path::PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("carrier-cargo-test-{suffix}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn get_projects_returns_direct_dependencies_without_lock_file() {
        let project_dir = temp_project_dir();
        fs::write(
            project_dir.join("Cargo.toml"),
            r#"
[package]
name = "sample"
version = "0.1.0"
edition = "2021"

[dependencies]
serde = "1"
"#,
        )
        .unwrap();

        let projects = CargoProvider::new().get_projects(&project_dir).unwrap();

        assert_eq!(projects.len(), 1);
        assert_eq!(projects[0].manifest_file, project_dir.join("Cargo.toml"));
        assert_eq!(projects[0].direct_dependencies.len(), 1);
        assert_eq!(projects[0].direct_dependencies[0].name, "serde");
        assert!(projects[0].dependencies_graph.nodes.is_empty());

        fs::remove_dir_all(project_dir).unwrap();
    }

    #[test]
    fn test_is_outdated() {
        let provider = CargoProvider::new();

        // With resolved version, latest controls the result.
        assert!(!provider.is_outdated("2", Some("2.1.0"), "2.1.0"));
        assert!(provider.is_outdated("2", Some("2.1.0"), "2.2.0"));
        assert!(provider.is_outdated("^1.0.0", Some("1.0.0"), "1.0.5")); // Outdated because lockfile is old
        assert!(provider.is_outdated("^1.0.0", Some("1.0.0"), "2.0.0")); // Outdated because lockfile is old
        assert!(!provider.is_outdated("^1.0.0", Some("1.0.5"), "1.0.5")); // Not outdated, lockfile is current

        // Without resolved version, manifest requirement controls the result.
        assert!(!provider.is_outdated("2", None, "2.9.9"));
        assert!(provider.is_outdated("2", None, "3.0.0"));
        assert!(!provider.is_outdated("~2.0", None, "2.0.9"));
        assert!(provider.is_outdated("~2.0", None, "2.1.0"));
        assert!(!provider.is_outdated("^1.0.0", None, "1.0.5")); // Not outdated, manifest allows latest
        assert!(provider.is_outdated("^1.0.0", None, "2.0.0")); // Outdated, manifest strictly excludes latest
        assert!(!provider.is_outdated(">= 1.0.0", None, "2.0.0")); // Not outdated, manifest allows latest
    }
}
