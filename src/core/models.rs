use std::fmt;
use std::path::PathBuf;

/// Supported package ecosystems.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ecosystem {
    NuGet,
    Npm,
    Cargo,
    Pip,
    Go,
}

impl fmt::Display for Ecosystem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ecosystem::NuGet => write!(f, "NuGet"),
            Ecosystem::Npm => write!(f, "npm"),
            Ecosystem::Cargo => write!(f, "Cargo"),
            Ecosystem::Pip => write!(f, "pip"),
            Ecosystem::Go => write!(f, "Go"),
        }
    }
}

/// Where a dependency was declared.
#[derive(Debug, Clone)]
pub struct DependencySource {
    /// The manifest file where this dependency was found (e.g., MyApp.csproj).
    pub manifest_file: PathBuf,
    /// The lock file used to resolve the actual version, if any.
    pub lock_file: Option<PathBuf>,
    /// Which ecosystem this dependency belongs to.
    pub ecosystem: Ecosystem,
}

/// A single dependency found in a project manifest.
#[derive(Debug, Clone)]
pub struct Dependency {
    /// Package name (e.g., "Newtonsoft.Json", "serde").
    pub name: String,
    /// The version declared in the manifest (may be a range, wildcard, etc.)
    /// e.g., "3.*", "^4.18.2", "[13.0,14.0)", "1.0"
    pub declared_version: String,
    /// The actual resolved version from a lock file (if available).
    /// This is the pinned version currently installed.
    /// e.g., "3.2.1", "4.18.2"
    pub resolved_version: Option<String>,
    /// Where this dependency was declared.
    pub source: DependencySource,
}

/// Result of checking a dependency against its registry.
#[derive(Debug, Clone)]
pub struct OutdatedDependency {
    /// Package name.
    pub name: String,
    /// The version currently in use (resolved from lock file if available,
    /// otherwise the declared version from the manifest).
    pub current_version: String,
    /// The latest stable version available in the registry.
    pub latest_version: String,
    /// Which ecosystem this dependency belongs to.
    pub ecosystem: Ecosystem,
    /// The file where this dependency was declared.
    pub source_file: PathBuf,
}
