use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use toml::Value;

use crate::core::models::{DependencyGraph, DependencyNode, Project};

pub fn get_projects(project_path: &Path) -> Result<Vec<Project>> {
    let lock_path = project_path.join("Cargo.lock");
    if !lock_path.exists() {
        return Ok(Vec::new());
    }

    let content = fs::read_to_string(&lock_path)
        .with_context(|| format!("Failed to read '{}'", lock_path.display()))?;
    let lock: Value = content
        .parse()
        .with_context(|| format!("Failed to parse '{}'", lock_path.display()))?;

    let packages = lock.get("package").and_then(|p| p.as_array());
    if packages.is_none() {
        return Ok(Vec::new());
    }

    let mut nodes = Vec::new();

    for pkg in packages.unwrap() {
        let name = pkg
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let version = pkg
            .get("version")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if name.is_empty() || version.is_empty() {
            continue;
        }

        let id = format!("{} {}", name, version);
        let mut deps = Vec::new();

        if let Some(deps_array) = pkg.get("dependencies").and_then(|v| v.as_array()) {
            for dep in deps_array {
                if let Some(dep_str) = dep.as_str() {
                    // Dependency string is either "name" or "name version"
                    let parts: Vec<&str> = dep_str.split_whitespace().collect();
                    let dep_name = parts[0].to_string();
                    let dep_ver = if parts.len() > 1 {
                        Some(parts[1].to_string())
                    } else {
                        None
                    };
                    deps.push((dep_name, dep_ver));
                }
            }
        }

        nodes.push(DependencyNode {
            id,
            name,
            version,
            dependencies: deps,
        });
    }

    // Try to determine a project name
    let project_name = project_path
        .file_name()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Cargo Workspace".to_string());

    let project = Project {
        name: project_name,
        graph: DependencyGraph { nodes },
    };

    Ok(vec![project])
}
