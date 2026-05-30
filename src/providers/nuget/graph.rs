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

    let mut nodes_map = std::collections::HashMap::new();

    // A project can have multiple targets (e.g. net6.0, net8.0)
    for (_target_name, target_obj) in targets.unwrap() {
        let packages = target_obj.as_object();
        if packages.is_none() {
            continue;
        }
        let packages = packages.unwrap();

        let mut name_to_version = std::collections::HashMap::new();
        for (pkg_id_ver, _) in packages {
            let parts: Vec<&str> = pkg_id_ver.split('/').collect();
            if parts.len() == 2 {
                name_to_version.insert(parts[0].to_string(), parts[1].to_string());
            }
        }

        for (pkg_id_ver, pkg_info) in packages {
            let parts: Vec<&str> = pkg_id_ver.split('/').collect();
            if parts.len() != 2 {
                continue;
            }
            let name = parts[0].to_string();
            let version = parts[1].to_string();
            let id = format!("{} {}", name, version);
            let mut deps = Vec::new();
            if let Some(deps_obj) = pkg_info.get("dependencies").and_then(|d| d.as_object()) {
                for (dep_name, dep_ver) in deps_obj {
                    let req_ver = Some(dep_ver.as_str().unwrap_or("").to_string());
                    let target_id = if let Some(resolved_ver) = name_to_version.get(dep_name) {
                        format!("{} {}", dep_name, resolved_ver)
                    } else {
                        dep_name.to_string()
                    };
                    deps.push((target_id, req_ver));
                }
            }

            let entry = nodes_map.entry(id.clone()).or_insert_with(|| DependencyNode {
                id: id.clone(),
                name,
                version,
                dependencies: Vec::new(),
            });

            for dep in deps {
                if !entry.dependencies.contains(&dep) {
                    entry.dependencies.push(dep);
                }
            }
        }
    }

    Ok(DependencyGraph { nodes: nodes_map.into_values().collect() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn parses_nuget_assets_with_correct_target_ids() {
        let dir = tempdir().unwrap();
        let assets_path = dir.path().join("project.assets.json");
        let mut file = File::create(&assets_path).unwrap();
        
        let json_content = r#"{
  "targets": {
    "net6.0": {
      "Newtonsoft.Json/13.0.1": {
        "type": "package",
        "dependencies": {
          "SomeDep": "1.2.3"
        }
      },
      "SomeDep/1.2.3": {
        "type": "package"
      }
    }
  }
}"#;
        file.write_all(json_content.as_bytes()).unwrap();

        let graph = build_graph_from_assets(&assets_path).unwrap();
        assert_eq!(graph.nodes.len(), 2);
        
        let newtonsoft = graph.nodes.iter().find(|n| n.name == "Newtonsoft.Json").unwrap();
        assert_eq!(newtonsoft.dependencies.len(), 1);
        // Should resolve the requested version range (1.2.3) into the exact target node ID (SomeDep 1.2.3)
        assert_eq!(newtonsoft.dependencies[0].0, "SomeDep 1.2.3");
        assert_eq!(newtonsoft.dependencies[0].1, Some("1.2.3".to_string()));
    }
}
