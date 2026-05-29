use std::path::{Path, PathBuf};

use anyhow::Result;

use super::models::{Dependency, Ecosystem};

/// Trait that every package ecosystem provider must implement.
/// All methods are synchronous — no async runtime needed.
pub trait Provider {
    /// Human-readable name (e.g., "NuGet", "npm").
    fn name(&self) -> &str;

    /// The ecosystem this provider handles.
    fn ecosystem(&self) -> Ecosystem;

    /// Check if this provider can handle the given project directory
    /// by looking for known manifest files.
    /// Returns the list of detected manifest files, or None if not applicable.
    fn detect(&self, project_path: &Path) -> Option<Vec<PathBuf>>;

    /// Parse dependencies from the detected manifest files.
    /// For providers that support lock files (Cargo, npm, etc.),
    /// this also reads the lock file to populate `resolved_version`.
    fn parse_dependencies(
        &self,
        project_path: &Path,
        manifest_files: &[PathBuf],
    ) -> Result<Vec<Dependency>>;

    /// Query the package registry for the latest stable version of a single package.
    /// Returns `Ok(None)` if the package was not found in the registry.
    fn get_latest_version(&self, agent: &ureq::Agent, package_name: &str)
        -> Result<Option<String>>;

    /// Explain why a package is included in the project dependencies.
    fn why(&self, _project_path: &Path, _package_name: &str) -> Result<()> {
        Ok(()) // Default implementation does nothing
    }
}

/// Registry that holds all available providers.
pub struct ProviderRegistry {
    providers: Vec<Box<dyn Provider>>,
}

impl ProviderRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            providers: Vec::new(),
        }
    }

    /// Register a new provider.
    pub fn register(&mut self, provider: Box<dyn Provider>) {
        self.providers.push(provider);
    }

    /// Detect which providers are applicable for the given path.
    /// Only scans the immediate directory (no recursion).
    /// Returns a list of (provider, manifest_files) tuples.
    pub fn detect_providers(&self, path: &Path) -> Vec<(&dyn Provider, Vec<PathBuf>)> {
        let mut detected = Vec::new();
        for provider in &self.providers {
            if let Some(files) = provider.detect(path) {
                if !files.is_empty() {
                    detected.push((provider.as_ref(), files));
                }
            }
        }
        detected
    }
}
