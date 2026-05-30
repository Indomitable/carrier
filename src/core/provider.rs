use std::path::Path;

use anyhow::Result;

use super::models::{Ecosystem, Project};

/// Trait that every package ecosystem provider must implement.
/// All methods are synchronous — no async runtime needed.
pub trait Provider: Send + Sync {
    /// Human-readable name (e.g., "NuGet", "npm").
    fn name(&self) -> &str;

    /// The ecosystem this provider handles.
    fn ecosystem(&self) -> Ecosystem;

    /// Check if this provider can handle the given project directory.
    fn detect(&self, project_path: &Path) -> bool;

    /// Query the package registry for the latest stable version of a single package.
    /// Returns `Ok(None)` if the package was not found in the registry.
    fn get_latest_version(&self, package_name: &str) -> Result<Option<String>>;

    /// Determine if a dependency is outdated based on ecosystem semantics.
    /// `declared` is the version requirement from the manifest.
    /// `resolved` is the exact version from the lock file (if available).
    /// `latest` is the latest stable version from the registry.
    fn is_outdated(&self, declared: &str, resolved: Option<&str>, latest: &str) -> bool;

    /// Get a list of detected projects and their dependency graphs.
    fn get_projects(&self, _project_path: &Path) -> Result<Vec<Project>> {
        Ok(Vec::new())
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
    pub fn detect_providers(&self, path: &Path) -> Vec<&dyn Provider> {
        let mut detected = Vec::new();
        for provider in &self.providers {
            if provider.detect(path) {
                detected.push(provider.as_ref());
            }
        }
        detected
    }
}
