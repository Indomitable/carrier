use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::Path;

use anyhow::{Context, Result};
use serde_json::Value;

pub fn analyze_assets_for_why(assets_path: &Path, package_name: &str) -> Result<bool> {
    let content = fs::read_to_string(assets_path)
        .with_context(|| format!("Failed to read {}", assets_path.display()))?;
    let json: Value = serde_json::from_str(&content)
        .with_context(|| format!("Failed to parse {}", assets_path.display()))?;

    let targets = json.get("targets").and_then(|t| t.as_object());
    if targets.is_none() {
        return Ok(false);
    }

    let mut found = false;

    // A project can have multiple targets (e.g. net6.0, net8.0)
    for (target_name, target_obj) in targets.unwrap() {
        let packages = target_obj.as_object();
        if packages.is_none() {
            continue;
        }
        let packages = packages.unwrap();

        // Build a graph
        // node_id (package name without version) -> (resolved_version, dependencies)
        let mut graph: HashMap<String, (String, HashMap<String, String>)> = HashMap::new();

        let mut target_resolved_version = None;
        let mut target_node_key = None;

        for (pkg_id_ver, pkg_info) in packages {
            let parts: Vec<&str> = pkg_id_ver.split('/').collect();
            if parts.len() != 2 {
                continue;
            }
            let name = parts[0].to_string();
            let version = parts[1].to_string();

            if name.eq_ignore_ascii_case(package_name) {
                target_resolved_version = Some(version.clone());
                target_node_key = Some(name.clone());
            }

            let mut deps = HashMap::new();
            if let Some(deps_obj) = pkg_info.get("dependencies").and_then(|d| d.as_object()) {
                for (dep_name, dep_ver) in deps_obj {
                    deps.insert(
                        dep_name.to_string(),
                        dep_ver.as_str().unwrap_or("").to_string(),
                    );
                }
            }

            graph.insert(name, (version, deps));
        }

        // Only proceed if the target package is in this framework
        if let (Some(target_version), Some(target_node)) =
            (target_resolved_version, target_node_key)
        {
            found = true;

            // Try to find the project name from the path (e.g. TestProject/obj/project.assets.json)
            let project_name = assets_path
                .parent()
                .and_then(|p| p.parent())
                .and_then(|p| p.file_name())
                .map(|s| s.to_string_lossy().to_string())
                .unwrap_or_else(|| "Unknown".to_string());

            println!("Project: {}", project_name);
            println!("Target: [{}]", target_name);
            println!("{}: {}", package_name, target_version);

            // Backtracking from target package.
            let mut reverse_graph: HashMap<String, Vec<(String, String)>> = HashMap::new();
            for (parent, (_, deps)) in &graph {
                for (dep_name, dep_req_ver) in deps {
                    // project.assets.json dependency keys might have different casing
                    let mut matched_dep = None;
                    for k in graph.keys() {
                        if k.eq_ignore_ascii_case(dep_name) {
                            matched_dep = Some(k.clone());
                            break;
                        }
                    }
                    if let Some(matched) = matched_dep {
                        reverse_graph
                            .entry(matched)
                            .or_default()
                            .push((parent.clone(), dep_req_ver.clone()));
                    }
                }
            }

            let mut paths = Vec::new();

            fn dfs(
                current: &str,
                graph: &HashMap<String, (String, HashMap<String, String>)>,
                reverse_graph: &HashMap<String, Vec<(String, String)>>,
                current_sequence: &mut VecDeque<String>,
                paths: &mut Vec<Vec<String>>,
                visited: &mut HashSet<String>,
            ) {
                if visited.contains(current) {
                    return;
                }
                visited.insert(current.to_string());

                if let Some(parents) = reverse_graph.get(current) {
                    if parents.is_empty() {
                        let resolved_version = &graph[current].0;
                        current_sequence.push_front(format!("{} ({})", current, resolved_version));
                        paths.push(current_sequence.iter().cloned().collect());
                        current_sequence.pop_front();
                    } else {
                        for (parent, req_ver) in parents {
                            current_sequence.push_front(format!("{} ({})", current, req_ver));
                            dfs(
                                parent,
                                graph,
                                reverse_graph,
                                current_sequence,
                                paths,
                                visited,
                            );
                            current_sequence.pop_front();
                        }
                    }
                } else {
                    // Root
                    let resolved_version = &graph[current].0;
                    current_sequence.push_front(format!("{} ({})", current, resolved_version));
                    paths.push(current_sequence.iter().cloned().collect());
                    current_sequence.pop_front();
                }

                visited.remove(current);
            }

            let mut visited = HashSet::new();

            if let Some(parents) = reverse_graph.get(&target_node) {
                if parents.is_empty() {
                    paths.push(vec![format!("{} ({})", target_node, target_version)]);
                } else {
                    for (parent, req_ver) in parents {
                        let mut seq = VecDeque::new();
                        seq.push_front(format!("{} ({})", target_node, req_ver));
                        dfs(
                            parent,
                            &graph,
                            &reverse_graph,
                            &mut seq,
                            &mut paths,
                            &mut visited,
                        );
                    }
                }
            } else {
                // Node has no parents in graph (direct root dependency with no other dependents)
                paths.push(vec![format!("{} ({})", target_node, target_version)]);
            }

            for path in paths {
                println!("{}", path.join(" -> "));
            }
            println!();
        }
    }

    Ok(found)
}
