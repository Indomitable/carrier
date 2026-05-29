use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use toml::Value;

use crate::core::models::{Dependency, DependencySource, Ecosystem};

#[derive(Debug, Clone, PartialEq, Eq)]
struct CargoDependency {
    name: String,
    declared_version: String,
}

/// Parse all direct Cargo dependencies from Cargo.toml files.
///
/// Cargo.lock is optional. When present, it supplies the resolved version for
/// direct dependencies; otherwise the declared Cargo.toml version is used.
pub fn parse_cargo_manifests(
    project_path: &Path,
    manifest_files: &[PathBuf],
) -> Result<Vec<Dependency>> {
    let lock_path = project_path.join("Cargo.lock");
    let lock_versions = if lock_path.exists() {
        parse_lock_file(&lock_path)?
    } else {
        HashMap::new()
    };

    let mut dependencies = Vec::new();
    let mut seen = HashSet::new();

    for manifest_file in manifest_files {
        let content = std::fs::read_to_string(manifest_file)
            .with_context(|| format!("Failed to read '{}'", manifest_file.display()))?;
        let manifest = content
            .parse::<Value>()
            .with_context(|| format!("Failed to parse '{}'", manifest_file.display()))?;

        for cargo_dep in parse_manifest_dependencies(&manifest) {
            if !seen.insert(cargo_dep.name.clone()) {
                continue;
            }

            let resolved_version = lock_versions.get(&cargo_dep.name).cloned();

            dependencies.push(Dependency {
                name: cargo_dep.name,
                declared_version: cargo_dep.declared_version,
                resolved_version,
                source: DependencySource {
                    manifest_file: manifest_file.clone(),
                    lock_file: if lock_path.exists() {
                        Some(lock_path.clone())
                    } else {
                        None
                    },
                    ecosystem: Ecosystem::Cargo,
                },
            });
        }
    }

    Ok(dependencies)
}

fn parse_manifest_dependencies(manifest: &Value) -> Vec<CargoDependency> {
    let mut dependencies = Vec::new();

    collect_dependency_table(manifest.get("dependencies"), &mut dependencies);
    collect_dependency_table(manifest.get("dev-dependencies"), &mut dependencies);
    collect_dependency_table(manifest.get("build-dependencies"), &mut dependencies);

    if let Some(targets) = manifest.get("target").and_then(Value::as_table) {
        for target in targets.values() {
            collect_dependency_table(target.get("dependencies"), &mut dependencies);
            collect_dependency_table(target.get("dev-dependencies"), &mut dependencies);
            collect_dependency_table(target.get("build-dependencies"), &mut dependencies);
        }
    }

    dependencies
}

fn collect_dependency_table(value: Option<&Value>, dependencies: &mut Vec<CargoDependency>) {
    let Some(table) = value.and_then(Value::as_table) else {
        return;
    };

    for (dependency_key, dependency_value) in table {
        if let Some(dep) = parse_dependency_entry(dependency_key, dependency_value) {
            dependencies.push(dep);
        }
    }
}

fn parse_dependency_entry(dependency_key: &str, value: &Value) -> Option<CargoDependency> {
    match value {
        Value::String(version) => Some(CargoDependency {
            name: dependency_key.to_string(),
            declared_version: version.clone(),
        }),
        Value::Table(table) => {
            let declared_version = table.get("version")?.as_str()?.to_string();
            let name = table
                .get("package")
                .and_then(Value::as_str)
                .unwrap_or(dependency_key)
                .to_string();

            Some(CargoDependency {
                name,
                declared_version,
            })
        }
        _ => None,
    }
}

fn parse_lock_file(lock_path: &Path) -> Result<HashMap<String, String>> {
    let content = std::fs::read_to_string(lock_path)
        .with_context(|| format!("Failed to read '{}'", lock_path.display()))?;
    let lock = content
        .parse::<Value>()
        .with_context(|| format!("Failed to parse '{}'", lock_path.display()))?;

    Ok(parse_lock_versions(&lock))
}

fn parse_lock_versions(lock: &Value) -> HashMap<String, String> {
    let mut versions = HashMap::new();

    if let Some(packages) = lock.get("package").and_then(Value::as_array) {
        for package in packages {
            let Some(name) = package.get("name").and_then(Value::as_str) else {
                continue;
            };
            let Some(version) = package.get("version").and_then(Value::as_str) else {
                continue;
            };

            versions.insert(name.to_string(), version.to_string());
        }
    }

    versions
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn parse_manifest(input: &str) -> Vec<CargoDependency> {
        let manifest = input.parse::<Value>().unwrap();
        parse_manifest_dependencies(&manifest)
    }

    #[test]
    fn parses_simple_dependencies() {
        let deps = parse_manifest(
            r#"
[dependencies]
serde = "1"
anyhow = "1.0.0"
"#,
        );

        assert_eq!(
            deps,
            vec![
                CargoDependency {
                    name: "anyhow".to_string(),
                    declared_version: "1.0.0".to_string(),
                },
                CargoDependency {
                    name: "serde".to_string(),
                    declared_version: "1".to_string(),
                },
            ]
        );
    }

    #[test]
    fn parses_inline_table_dependencies() {
        let deps = parse_manifest(
            r#"
[dependencies]
tokio = { version = "1", features = ["full"] }
"#,
        );

        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].name, "tokio");
        assert_eq!(deps[0].declared_version, "1");
    }

    #[test]
    fn parses_renamed_dependencies() {
        let deps = parse_manifest(
            r#"
[dependencies]
serde_renamed = { package = "serde", version = "1" }
"#,
        );

        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].name, "serde");
        assert_eq!(deps[0].declared_version, "1");
    }

    #[test]
    fn parses_dev_build_and_target_dependencies() {
        let deps = parse_manifest(
            r#"
[dev-dependencies]
pretty_assertions = "1"

[build-dependencies]
cc = "1"

[target.'cfg(unix)'.dependencies]
nix = "0.29"

[target.'cfg(windows)'.dev-dependencies]
winapi = "0.3"

[target.'cfg(target_os = "linux")'.build-dependencies]
bindgen = "0.70"
"#,
        );

        let names: HashSet<_> = deps.iter().map(|dep| dep.name.as_str()).collect();
        assert_eq!(names.len(), 5);
        assert!(names.contains("pretty_assertions"));
        assert!(names.contains("cc"));
        assert!(names.contains("nix"));
        assert!(names.contains("winapi"));
        assert!(names.contains("bindgen"));
    }

    #[test]
    fn ignores_dependencies_without_version() {
        let deps = parse_manifest(
            r#"
[dependencies]
local_crate = { path = "../local_crate" }
git_crate = { git = "https://example.com/repo.git" }
serde = { version = "1", path = "../serde" }
"#,
        );

        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].name, "serde");
    }

    #[test]
    fn parses_lock_versions() {
        let lock = r#"
version = 4

[[package]]
name = "serde"
version = "1.0.228"

[[package]]
name = "transitive"
version = "0.1.0"
"#
        .parse::<Value>()
        .unwrap();

        let versions = parse_lock_versions(&lock);

        assert_eq!(versions.get("serde"), Some(&"1.0.228".to_string()));
        assert_eq!(versions.get("transitive"), Some(&"0.1.0".to_string()));
    }

    #[test]
    fn lock_versions_override_manifest_versions_for_direct_dependencies() {
        let project_dir = create_temp_project("lock_override");
        let manifest_path = project_dir.join("Cargo.toml");
        let lock_path = project_dir.join("Cargo.lock");

        fs::write(
            &manifest_path,
            r#"
[package]
name = "example"
version = "0.1.0"

[dependencies]
tokio = "1"
serde_renamed = { package = "serde", version = "1" }
"#,
        )
        .unwrap();
        fs::write(
            &lock_path,
            r#"
version = 4

[[package]]
name = "tokio"
version = "1.48.0"

[[package]]
name = "serde"
version = "1.0.228"

[[package]]
name = "transitive"
version = "0.1.0"
"#,
        )
        .unwrap();

        let deps = parse_cargo_manifests(&project_dir, &[manifest_path.clone()]).unwrap();
        let tokio = deps.iter().find(|dep| dep.name == "tokio").unwrap();
        let serde = deps.iter().find(|dep| dep.name == "serde").unwrap();

        assert_eq!(deps.len(), 2);
        assert_eq!(tokio.declared_version, "1");
        assert_eq!(tokio.resolved_version, Some("1.48.0".to_string()));
        assert_eq!(tokio.source.lock_file, Some(lock_path.clone()));
        assert_eq!(serde.resolved_version, Some("1.0.228".to_string()));

        fs::remove_dir_all(project_dir).unwrap();
    }

    #[test]
    fn missing_lock_file_falls_back_to_manifest_versions() {
        let project_dir = create_temp_project("missing_lock");
        let manifest_path = project_dir.join("Cargo.toml");

        fs::write(
            &manifest_path,
            r#"
[package]
name = "example"
version = "0.1.0"

[dependencies]
tokio = "1"
"#,
        )
        .unwrap();

        let deps = parse_cargo_manifests(&project_dir, &[manifest_path]).unwrap();

        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].name, "tokio");
        assert_eq!(deps[0].declared_version, "1");
        assert_eq!(deps[0].resolved_version, None);
        assert_eq!(deps[0].source.lock_file, None);

        fs::remove_dir_all(project_dir).unwrap();
    }

    fn create_temp_project(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("carrier_cargo_{name}_{nanos}"));
        fs::create_dir(&path).unwrap();
        path
    }
}
