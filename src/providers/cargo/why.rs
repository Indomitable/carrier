use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use toml::Value;

pub fn run_why(project_path: &Path, package_name: &str) -> Result<()> {
    let lock_path = project_path.join("Cargo.lock");
    if !lock_path.exists() {
        anyhow::bail!(
            "No Cargo.lock found in '{}'. 'why' command for Cargo requires a lock file.",
            project_path.display()
        );
    }

    let content = fs::read_to_string(&lock_path)
        .with_context(|| format!("Failed to read '{}'", lock_path.display()))?;
    let lock: Value = content
        .parse()
        .with_context(|| format!("Failed to parse '{}'", lock_path.display()))?;

    let packages = lock.get("package").and_then(|p| p.as_array());
    if packages.is_none() {
        anyhow::bail!("Invalid Cargo.lock: missing [[package]] array.");
    }

    struct Node {
        id: String,
        name: String,
        version: String,
        dependencies: Vec<(String, Option<String>)>,
    }

    let mut nodes = Vec::new();
    let mut name_to_ids: HashMap<String, Vec<String>> = HashMap::new();

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
                    // Dependency string is either "name" or "name version" (or more, but version is 2nd)
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

        name_to_ids
            .entry(name.clone())
            .or_default()
            .push(id.clone());
        nodes.push(Node {
            id,
            name,
            version,
            dependencies: deps,
        });
    }

    // Build reverse graph: target_id -> Vec<(parent_id, req_ver_opt)>
    let mut reverse_graph: HashMap<String, Vec<(String, Option<String>)>> = HashMap::new();
    let mut in_degree: HashMap<String, usize> = HashMap::new();

    for node in &nodes {
        in_degree.entry(node.id.clone()).or_insert(0);
        for (dep_name, dep_ver) in &node.dependencies {
            let target_id = if let Some(v) = dep_ver {
                format!("{} {}", dep_name, v)
            } else {
                // Should only be 1 version of this package
                if let Some(ids) = name_to_ids.get(dep_name) {
                    ids[0].clone()
                } else {
                    continue; // missing dependency?
                }
            };

            reverse_graph
                .entry(target_id.clone())
                .or_default()
                .push((node.id.clone(), dep_ver.clone()));
            *in_degree.entry(target_id).or_insert(0) += 1;
        }
    }

    // Find all target nodes matching package_name
    let target_nodes: Vec<&Node> = nodes
        .iter()
        .filter(|n| n.name.eq_ignore_ascii_case(package_name))
        .collect();

    if target_nodes.is_empty() {
        println!("Package '{}' was not found in Cargo.lock.", package_name);
        return Ok(());
    }

    // Helper map to get Node by id
    let id_to_node: HashMap<String, &Node> = nodes.iter().map(|n| (n.id.clone(), n)).collect();

    for target in target_nodes {
        println!("{}: {}", target.name, target.version);

        let mut paths = Vec::new();

        fn dfs(
            current_id: &str,
            reverse_graph: &HashMap<String, Vec<(String, Option<String>)>>,
            in_degree: &HashMap<String, usize>,
            id_to_node: &HashMap<String, &Node>,
            current_sequence: &mut VecDeque<String>,
            paths: &mut Vec<Vec<String>>,
            visited: &mut HashSet<String>,
        ) {
            if visited.contains(current_id) {
                return;
            }
            visited.insert(current_id.to_string());

            let node = id_to_node.get(current_id).unwrap();

            if let Some(parents) = reverse_graph.get(current_id) {
                if parents.is_empty() {
                    // Root
                    current_sequence.push_front(format!("{} ({})", node.name, node.version));
                    paths.push(current_sequence.iter().cloned().collect());
                    current_sequence.pop_front();
                } else {
                    for (parent_id, req_ver_opt) in parents {
                        let edge_str = if let Some(req_ver) = req_ver_opt {
                            format!("{} ({})", node.name, req_ver)
                        } else {
                            format!("{} ({})", node.name, node.version)
                        };
                        current_sequence.push_front(edge_str);
                        dfs(
                            parent_id,
                            reverse_graph,
                            in_degree,
                            id_to_node,
                            current_sequence,
                            paths,
                            visited,
                        );
                        current_sequence.pop_front();
                    }
                }
            } else {
                // Root
                current_sequence.push_front(format!("{} ({})", node.name, node.version));
                paths.push(current_sequence.iter().cloned().collect());
                current_sequence.pop_front();
            }

            visited.remove(current_id);
        }

        let mut visited = HashSet::new();

        if let Some(parents) = reverse_graph.get(&target.id) {
            if parents.is_empty() {
                paths.push(vec![format!("{} ({})", target.name, target.version)]);
            } else {
                for (parent_id, req_ver_opt) in parents {
                    let mut seq = VecDeque::new();
                    let edge_str = if let Some(req_ver) = req_ver_opt {
                        format!("{} ({})", target.name, req_ver)
                    } else {
                        format!("{} ({})", target.name, target.version)
                    };
                    seq.push_front(edge_str);
                    dfs(
                        parent_id,
                        &reverse_graph,
                        &in_degree,
                        &id_to_node,
                        &mut seq,
                        &mut paths,
                        &mut visited,
                    );
                }
            }
        } else {
            paths.push(vec![format!("{} ({})", target.name, target.version)]);
        }

        // Sort paths for stable output
        paths.sort();

        for path in paths {
            println!("{}", path.join(" -> "));
        }
        println!();
    }

    Ok(())
}
