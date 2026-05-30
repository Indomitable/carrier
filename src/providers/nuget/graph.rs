use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::Value;

use crate::core::models::{DependencyGraph, DependencyNode, Project};

pub fn build_graphs_from_assets(assets_path: &Path) -> Result<Vec<Project>> {
    let content = fs::read_to_string(assets_path)
        .with_context(|| format!("Failed to read {}", assets_path.display()))?;
    let json: Value = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse {}", assets_path.display()))?;

    let targets = json.get("targets").and_then(|t| t.as_object());
    if targets.is_none() {
        return Ok(Vec::new());
    }

    let mut projects = Vec::new();

    // Try to find the project name from the path (e.g. TestProject/obj/project.assets.json)
    let base_project_name = assets_path
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.file_name())
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "Unknown".to_string());

    // A project can have multiple targets (e.g. net6.0, net8.0)
    for (target_name, target_obj) in targets.unwrap() {
        let packages = target_obj.as_object();
        if packages.is_none() {
            continue;
        }
        let packages = packages.unwrap();

        let mut nodes = Vec::new();

        for (pkg_id_ver, pkg_info) in packages {
            let parts: Vec<&str> = pkg_id_ver.split('/').collect();
            if parts.len() != 2 {
                continue;
            }
            let name = parts[0].to_string();
            let version = parts[1].to_string();

            let mut deps = Vec::new();
            if let Some(deps_obj) = pkg_info.get("dependencies").and_then(|d| d.as_object()) {
                for (dep_name, dep_ver) in deps_obj {
                    deps.push((
                        dep_name.to_string(),
                        Some(dep_ver.as_str().unwrap_or("").to_string()),
                    ));
                }
            }

            nodes.push(DependencyNode {
                id: name.clone(), // NuGet flat graph uses package name as id
                name,
                version,
                dependencies: deps,
            });
        }

        let project_name = format!("{} [{}]", base_project_name, target_name);
        projects.push(Project {
            name: project_name,
            graph: DependencyGraph { nodes },
        });
    }

    Ok(projects)
}
