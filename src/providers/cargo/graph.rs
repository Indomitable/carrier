use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use toml::Value;

use crate::core::models::{DependencyGraph, DependencyNode};

pub fn build_dependency_graph(project_path: &Path) -> Result<DependencyGraph> {
    let lock_path = project_path.join("Cargo.lock");
    if !lock_path.exists() {
        return Ok(DependencyGraph { nodes: Vec::new() });
    }

    let content = fs::read_to_string(&lock_path)
        .with_context(|| format!("Failed to read '{}'", lock_path.display()))?;
    let lock: Value = content
        .parse()
        .with_context(|| format!("Failed to parse '{}'", lock_path.display()))?;

    let packages = lock.get("package").and_then(|p| p.as_array());
    if packages.is_none() {
        return Ok(DependencyGraph { nodes: Vec::new() });
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
                    
                    let target_id = if let Some(ver) = &dep_ver {
                        format!("{} {}", dep_name, ver)
                    } else {
                        dep_name.clone()
                    };
                    
                    deps.push((target_id, dep_ver));
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

    Ok(DependencyGraph { nodes })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn parses_cargo_lock_dependencies_with_correct_target_ids() {
        let dir = tempdir().unwrap();
        let lock_path = dir.path().join("Cargo.lock");
        let mut file = File::create(&lock_path).unwrap();
        
        let toml_content = r#"
[[package]]
name = "my_app"
version = "0.1.0"
dependencies = [
 "serde",
 "serde_json 1.0.100",
]

[[package]]
name = "serde"
version = "1.0.100"

[[package]]
name = "serde_json"
version = "1.0.100"
dependencies = [
 "serde 1.0.100",
]
"#;
        file.write_all(toml_content.as_bytes()).unwrap();

        let graph = build_dependency_graph(dir.path()).unwrap();
        assert_eq!(graph.nodes.len(), 3);
        
        let my_app = graph.nodes.iter().find(|n| n.name == "my_app").unwrap();
        // Since serde dependency doesn't specify a version, target_id should fallback to its name "serde"
        assert_eq!(my_app.dependencies[0].0, "serde");
        assert_eq!(my_app.dependencies[0].1, None);
        // serde_json dependency specifies version, target_id should be "serde_json 1.0.100"
        assert_eq!(my_app.dependencies[1].0, "serde_json 1.0.100");
        assert_eq!(my_app.dependencies[1].1, Some("1.0.100".to_string()));
    }
}
