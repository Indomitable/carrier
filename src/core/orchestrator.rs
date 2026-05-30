use std::path::Path;

use super::models::OutdatedDependency;
use super::provider::Provider;
use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};

/// Run the `outdated` command: detect providers, parse dependencies,
/// query registries, and return the list of outdated dependencies.
pub fn run_outdated(
    providers: Vec<&dyn Provider>,
    project_path: &Path,
) -> Result<Vec<OutdatedDependency>> {
    let mut all_outdated = Vec::new();

    for provider in providers {
        let projects = provider
            .get_projects(project_path)
            .with_context(|| format!("Failed to get {} projects", provider.name()))?;
        let dependencies: Vec<_> = projects
            .iter()
            .flat_map(|project| project.direct_dependencies.iter())
            .collect();

        if dependencies.is_empty() {
            continue;
        }

        // Extract unique dependency names to avoid duplicate network calls
        let unique_names: std::collections::HashSet<String> = dependencies
            .iter()
            .map(|d| d.name.clone())
            .collect();
        let unique_names: Vec<String> = unique_names.into_iter().collect();

        // Set up a progress spinner.
        let pb = ProgressBar::new(unique_names.len() as u64);
        pb.set_style(
            ProgressStyle::with_template("  {spinner:.cyan} [{pos}/{len}] Checking registry...")?
                .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"),
        );

        use rayon::prelude::*;

        // Fetch latest versions in parallel
        let latest_versions_result: Result<std::collections::HashMap<String, Option<String>>> =
            unique_names
                .into_par_iter()
                .map(|name| {
                    let latest = provider
                        .get_latest_version(&name)
                        .with_context(|| format!("Failed to check latest version for '{}'", name))?;
                    pb.inc(1);
                    Ok((name, latest))
                })
                .collect();

        let latest_versions = latest_versions_result?;

        pb.finish_and_clear();

        for dep in &dependencies {
            if let Some(Some(latest_version)) = latest_versions.get(&dep.name) {
                if provider.is_outdated(
                    &dep.declared_version,
                    dep.resolved_version.as_deref(),
                    latest_version,
                ) {
                    let current_str = dep
                        .resolved_version
                        .as_deref()
                        .unwrap_or(&dep.declared_version);

                    all_outdated.push(OutdatedDependency {
                        name: dep.name.clone(),
                        current_version: current_str.to_string(),
                        latest_version: latest_version.clone(),
                        ecosystem: dep.source.ecosystem,
                        source_file: dep.source.manifest_file.clone(),
                    });
                }
            }
        }
    }

    Ok(all_outdated)
}

/// Run the `why` command: detect providers, ask them for projects, and traverse their graphs to find the package.
pub fn run_why(
    providers: Vec<&dyn Provider>,
    project_path: &Path,
    package_name: &str,
) -> Result<Vec<super::models::WhyProjectResult>> {
    let mut all_results = Vec::new();

    for provider in &providers {
        let projects = provider.get_projects(project_path)?;

        for project in projects {
            let mut name_to_ids: std::collections::HashMap<String, Vec<String>> =
                std::collections::HashMap::new();
            let mut reverse_graph: std::collections::HashMap<
                String,
                Vec<(String, Option<String>)>,
            > = std::collections::HashMap::new();
            let mut id_to_node: std::collections::HashMap<String, &super::models::DependencyNode> =
                std::collections::HashMap::new();

            for node in &project.dependencies_graph.nodes {
                name_to_ids
                    .entry(node.name.clone())
                    .or_default()
                    .push(node.id.clone());
                id_to_node.insert(node.id.clone(), node);
            }

            for node in &project.dependencies_graph.nodes {
                for (target_node_id_or_name, dep_ver) in &node.dependencies {
                    let target_ids = if id_to_node.contains_key(target_node_id_or_name) {
                        vec![target_node_id_or_name.clone()]
                    } else if let Some(ids) = name_to_ids.get(target_node_id_or_name) {
                        ids.clone()
                    } else {
                        // Fallback 1: case-insensitive ID match
                        let mut matched_id = None;
                        for k in id_to_node.keys() {
                            if k.eq_ignore_ascii_case(target_node_id_or_name) {
                                matched_id = Some(k.clone());
                                break;
                            }
                        }
                        if let Some(m) = matched_id {
                            vec![m]
                        } else {
                            // Fallback 2: case-insensitive name match
                            let mut matched_by_name = Vec::new();
                            for (k, ids) in &name_to_ids {
                                if k.eq_ignore_ascii_case(target_node_id_or_name) {
                                    matched_by_name = ids.clone();
                                    break;
                                }
                            }
                            if !matched_by_name.is_empty() {
                                matched_by_name
                            } else {
                                continue;
                            }
                        }
                    };

                    for target_id in target_ids {
                        reverse_graph
                            .entry(target_id)
                            .or_default()
                            .push((node.id.clone(), dep_ver.clone()));
                    }
                }
            }

            // Find all target nodes matching package_name
            let target_nodes: Vec<&super::models::DependencyNode> = project
                .dependencies_graph
                .nodes
                .iter()
                .filter(|n| n.name.eq_ignore_ascii_case(package_name))
                .collect();

            if target_nodes.is_empty() {
                continue;
            }

            let mut project_result = super::models::WhyProjectResult {
                project_name: project.name.clone(),
                targets: Vec::new(),
            };

            for target in target_nodes {
                let mut target_result = super::models::WhyPath {
                    target_name: target.name.clone(),
                    target_version: target.version.clone(),
                    paths: Vec::new(),
                };

                let mut paths = Vec::new();

                fn dfs(
                    current_id: &str,
                    reverse_graph: &std::collections::HashMap<
                        String,
                        Vec<(String, Option<String>)>,
                    >,
                    id_to_node: &std::collections::HashMap<String, &super::models::DependencyNode>,
                    current_sequence: &mut std::collections::VecDeque<String>,
                    paths: &mut Vec<Vec<String>>,
                    visited: &mut std::collections::HashSet<String>,
                ) {
                    if visited.contains(current_id) {
                        return;
                    }
                    visited.insert(current_id.to_string());

                    let node = id_to_node.get(current_id).unwrap();

                    if let Some(parents) = reverse_graph.get(current_id) {
                        if parents.is_empty() {
                            // Root
                            current_sequence
                                .push_front(format!("{} ({})", node.name, node.version));
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

                let mut visited = std::collections::HashSet::new();

                if let Some(parents) = reverse_graph.get(&target.id) {
                    if parents.is_empty() {
                        paths.push(vec![format!("{} ({})", target.name, target.version)]);
                    } else {
                        for (parent_id, req_ver_opt) in parents {
                            let mut seq = std::collections::VecDeque::new();
                            let edge_str = if let Some(req_ver) = req_ver_opt {
                                format!("{} ({})", target.name, req_ver)
                            } else {
                                format!("{} ({})", target.name, target.version)
                            };
                            seq.push_front(edge_str);
                            dfs(
                                parent_id,
                                &reverse_graph,
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
                target_result.paths = paths;
                project_result.targets.push(target_result);
            }

            all_results.push(project_result);
        }
    }

    Ok(all_results)
}


