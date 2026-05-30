use std::path::Path;

use super::models::OutdatedDependency;
use super::provider::Provider;
use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use semver::Version;
use ureq::config::Config;

/// Run the `outdated` command: detect providers, parse dependencies,
/// query registries, and return the list of outdated dependencies.
pub fn run_outdated(
    providers: Vec<&dyn Provider>,
    project_path: &Path,
) -> Result<Vec<OutdatedDependency>> {
    // Create a shared ureq agent for connection keep-alive.
    let config = Config::builder().user_agent("carrier").build();
    let agent = ureq::Agent::new_with_config(config);

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

        // Set up a progress spinner.
        let pb = ProgressBar::new(dependencies.len() as u64);
        pb.set_style(
            ProgressStyle::with_template("  {spinner:.cyan} [{pos}/{len}] Checking {msg}...")?
                .tick_chars("⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"),
        );

        for dep in &dependencies {
            pb.set_message(dep.name.clone());

            let latest = provider
                .get_latest_version(&agent, &dep.name)
                .with_context(|| format!("Failed to check latest version for '{}'", dep.name))?;

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

/// Run the `why` command: detect providers, ask them for projects, and traverse their graphs to find the package.
pub fn run_why(
    providers: Vec<&dyn Provider>,
    project_path: &Path,
    package_name: &str,
) -> Result<()> {
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
                for (dep_name, dep_ver) in &node.dependencies {
                    let target_ids = if let Some(ids) = name_to_ids.get(dep_name) {
                        ids.clone()
                    } else {
                        // Could be a missing dependency, or a mismatch in casing. Try case-insensitive.
                        let mut matched = None;
                        for k in name_to_ids.keys() {
                            if k.eq_ignore_ascii_case(dep_name) {
                                matched = Some(name_to_ids.get(k).unwrap().clone());
                                break;
                            }
                        }
                        if let Some(m) = matched {
                            m
                        } else {
                            continue;
                        }
                    };

                    // For simplicity, add reverse edges to all matching target IDs
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

            println!("Project: {}", project.name);

            for target in target_nodes {
                println!("{}: {}", target.name, target.version);

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

                for path in paths {
                    println!("{}", path.join(" -> "));
                }
                println!();
            }
        }
    }

    Ok(())
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
