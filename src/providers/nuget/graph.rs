use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::Value;

use crate::core::models::{DependencyGraph, DependencyNode};

pub fn build_graph_from_assets(assets_path: &Path) -> Result<DependencyGraph> {
    let content = fs::read_to_string(assets_path)
        .with_context(|| format!("Failed to read {}", assets_path.display()))?;
    let json: Value = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse {}", assets_path.display()))?;

    let targets = json.get("targets").and_then(|t| t.as_object());
    if targets.is_none() {
        return Ok(DependencyGraph { nodes: Vec::new() });
    }

    let mut nodes = Vec::new();
    let mut seen = std::collections::HashSet::new();

    // A project can have multiple targets (e.g. net6.0, net8.0)
    for (_target_name, target_obj) in targets.unwrap() {
        let packages = target_obj.as_object();
        if packages.is_none() {
            continue;
        }
        let packages = packages.unwrap();

        for (pkg_id_ver, pkg_info) in packages {
            let parts: Vec<&str> = pkg_id_ver.split('/').collect();
            if parts.len() != 2 {
                continue;
            }
            let name = parts[0].to_string();
            let version = parts[1].to_string();
            let id = format!("{} {}", name, version);
            if !seen.insert(id.clone()) {
                continue;
            }

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
                id,
                name,
                version,
                dependencies: deps,
            });
        }
    }

    Ok(DependencyGraph { nodes })
}
