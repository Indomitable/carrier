use std::path::Path;

use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use semver::Version;

use super::models::OutdatedDependency;
use super::provider::ProviderRegistry;

/// Run the `outdated` command: detect providers, parse dependencies,
/// query registries, and return the list of outdated dependencies.
pub fn run_outdated(
    registry: &ProviderRegistry,
    project_path: &Path,
) -> Result<Vec<OutdatedDependency>> {
    let detected = registry.detect_providers(project_path);

    if detected.is_empty() {
        anyhow::bail!(
            "No supported project files found in '{}'.\n\
             Supported files: *.csproj, Directory.Packages.props",
            project_path.display()
        );
    }

    // Create a shared ureq agent for connection keep-alive.
    let agent = ureq::Agent::new_with_defaults();

    let mut all_outdated = Vec::new();

    for (provider, manifest_files) in &detected {
        let dependencies = provider
            .parse_dependencies(project_path, manifest_files)
            .with_context(|| {
                format!(
                    "Failed to parse {} dependencies",
                    provider.name()
                )
            })?;

        if dependencies.is_empty() {
            continue;
        }

        // Set up a progress spinner.
        let pb = ProgressBar::new(dependencies.len() as u64);
        pb.set_style(
            ProgressStyle::with_template(
                "  {spinner:.cyan} [{pos}/{len}] Checking {msg}..."
            )
            .unwrap()
            .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"),
        );

        for dep in &dependencies {
            pb.set_message(dep.name.clone());

            let latest = provider
                .get_latest_version(&agent, &dep.name)
                .with_context(|| {
                    format!("Failed to check latest version for '{}'", dep.name)
                })?;

            if let Some(latest_version) = latest {
                // Determine the "current" version for comparison.
                // Prefer resolved_version (from lock file), fall back to declared_version.
                let current_str = dep
                    .resolved_version
                    .as_deref()
                    .unwrap_or(&dep.declared_version);

                // Try to normalize the current version string for semver parsing.
                let current_normalized = normalize_version(current_str);
                let latest_normalized = normalize_version(&latest_version);

                if let (Some(current_v), Some(latest_v)) = (
                    parse_version(&current_normalized),
                    parse_version(&latest_normalized),
                ) {
                    if latest_v > current_v {
                        all_outdated.push(OutdatedDependency {
                            name: dep.name.clone(),
                            current_version: current_str.to_string(),
                            latest_version,
                            ecosystem: dep.source.ecosystem,
                            source_file: dep.source.manifest_file.clone(),
                        });
                    }
                }
            }

            pb.inc(1);
        }

        pb.finish_and_clear();
    }

    Ok(all_outdated)
}

/// Attempt to parse a version string as a semver::Version.
fn parse_version(version: &str) -> Option<Version> {
    Version::parse(version).ok()
}

/// Normalize a version string for semver comparison.
/// Handles wildcards ("3.*" → "3.0.0"), two-part versions ("3.1" → "3.1.0"),
/// and strips common prefixes/ranges.
fn normalize_version(version: &str) -> String {
    let v = version.trim();

    // Strip leading range characters: [, (, >=, =, ~, ^
    let v = v.trim_start_matches(|c: char| "[(>=~^".contains(c));
    // Strip trailing range characters: ], ), and anything after comma
    let v = if let Some(idx) = v.find(|c: char| ",])".contains(c)) {
        &v[..idx]
    } else {
        v
    };
    let v = v.trim();

    // Replace wildcards: "3.*" → "3.0.0", "3.1.*" → "3.1.0"
    let v = v.replace(".*", "");

    // Ensure at least 3 parts (major.minor.patch)
    let parts: Vec<&str> = v.split('.').collect();
    match parts.len() {
        1 => format!("{}.0.0", parts[0]),
        2 => format!("{}.{}.0", parts[0], parts[1]),
        _ => v.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_normalize_version_full() {
        assert_eq!(normalize_version("13.0.3"), "13.0.3");
    }

    #[test]
    fn test_normalize_version_two_parts() {
        assert_eq!(normalize_version("3.1"), "3.1.0");
    }

    #[test]
    fn test_normalize_version_one_part() {
        assert_eq!(normalize_version("3"), "3.0.0");
    }

    #[test]
    fn test_normalize_version_wildcard() {
        assert_eq!(normalize_version("3.*"), "3.0.0");
        assert_eq!(normalize_version("3.1.*"), "3.1.0");
    }

    #[test]
    fn test_normalize_version_range() {
        assert_eq!(normalize_version("[13.0, 14.0)"), "13.0.0");
    }

    #[test]
    fn test_normalize_version_caret() {
        assert_eq!(normalize_version("^4.18.2"), "4.18.2");
    }

    #[test]
    fn test_normalize_version_tilde() {
        assert_eq!(normalize_version("~1.24.0"), "1.24.0");
    }
}
