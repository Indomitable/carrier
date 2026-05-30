pub mod graph;
pub mod parser;
pub mod registry;

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::core::models::{DependencyGraph, Ecosystem, Project};
use crate::core::provider::Provider;

/// NuGet package provider.
///
/// Detects .csproj files and Directory.Packages.props in the project directory.
/// Queries the NuGet v3 flat container API for latest versions.
pub struct NuGetProvider {
    agent: ureq::Agent,
}

impl NuGetProvider {
    pub fn new() -> Self {
        let config = ureq::config::Config::builder()
            .user_agent("carrier")
            .timeout_global(Some(std::time::Duration::from_secs(10)))
            .build();
        Self {
            agent: ureq::Agent::new_with_config(config),
        }
    }
}

impl Provider for NuGetProvider {
    fn name(&self) -> &str {
        "NuGet"
    }

    fn ecosystem(&self) -> Ecosystem {
        Ecosystem::NuGet
    }

    fn detect(&self, project_path: &Path) -> bool {
        find_slnx_file(project_path).is_some()
            || find_current_dir_csproj_files(project_path).is_ok_and(|files| !files.is_empty())
    }

    fn get_latest_version(
        &self,
        package_name: &str,
    ) -> Result<Option<String>> {
        registry::get_latest_stable_version(&self.agent, package_name)
    }

    fn is_outdated(&self, declared: &str, resolved: Option<&str>, latest: &str) -> bool {
        let Some(latest_ver) = NugetVersion::parse(latest) else {
            return false;
        };

        if let Some(res) = resolved {
            if let Some(res_ver) = NugetVersion::parse(res) {
                return latest_ver > res_ver;
            }
            return false;
        }

        let declared = declared.trim();
        if (declared.starts_with('[') || declared.starts_with('('))
            && declared.len() >= 2
            && (declared.ends_with(']') || declared.ends_with(')'))
        {
            let inner = &declared[1..declared.len() - 1];
            if let Some(comma_idx) = inner.find(',') {
                let min_str = inner[..comma_idx].trim();
                let max_str = inner[comma_idx + 1..].trim();

                let min_ok = if min_str.is_empty() {
                    true
                } else if let Some(min_ver) = NugetVersion::parse(min_str) {
                    if declared.starts_with('(') {
                        latest_ver > min_ver
                    } else {
                        latest_ver >= min_ver
                    }
                } else {
                    true
                };

                let max_ok = if max_str.is_empty() {
                    true
                } else if let Some(max_ver) = NugetVersion::parse(max_str) {
                    if declared.ends_with(')') {
                        latest_ver < max_ver
                    } else {
                        latest_ver <= max_ver
                    }
                } else {
                    true
                };

                return !(min_ok && max_ok);
            }

            if let Some(exact_ver) = NugetVersion::parse(inner) {
                return latest_ver != exact_ver;
            }
        }

        if let Some(min_ver) = NugetVersion::parse(declared) {
            if declared.split('.').count() == 1 {
                return latest_ver.major > min_ver.major;
            }

            return latest_ver > min_ver;
        }

        false
    }

    fn get_projects(&self, project_path: &Path) -> Result<Vec<Project>> {
        let (stop_dir, csproj_files) = if let Some(slnx_file) = find_slnx_file(project_path) {
            (
                project_path.to_path_buf(),
                parse_slnx_projects(project_path, &slnx_file)?,
            )
        } else {
            (
                project_path.to_path_buf(),
                find_current_dir_csproj_files(project_path)?,
            )
        };

        csproj_files
            .into_iter()
            .map(|csproj| build_project(&csproj, &stop_dir))
            .collect()
    }
}

fn find_slnx_file(project_path: &Path) -> Option<PathBuf> {
    std::fs::read_dir(project_path)
        .ok()?
        .flatten()
        .find_map(|entry| {
            let path = entry.path();
            if path.is_file()
                && path
                    .extension()
                    .is_some_and(|ext| ext.eq_ignore_ascii_case("slnx"))
            {
                Some(path)
            } else {
                None
            }
        })
}

fn find_current_dir_csproj_files(project_path: &Path) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(project_path)? {
        let path = entry?.path();
        if path.is_file()
            && path
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("csproj"))
        {
            files.push(path);
        }
    }
    files.sort();
    Ok(files)
}

fn parse_slnx_projects(project_path: &Path, slnx_file: &Path) -> Result<Vec<PathBuf>> {
    let content = std::fs::read_to_string(slnx_file)?;
    let mut reader = quick_xml::Reader::from_str(&content);
    let mut projects = Vec::new();

    loop {
        match reader.read_event() {
            Ok(quick_xml::events::Event::Empty(ref e))
            | Ok(quick_xml::events::Event::Start(ref e)) => {
                let local_name_raw = e.local_name();
                let local_name = std::str::from_utf8(local_name_raw.as_ref()).unwrap_or_default();
                if local_name.eq_ignore_ascii_case("Project") {
                    for attr in e.attributes().flatten() {
                        let key = std::str::from_utf8(attr.key.as_ref()).unwrap_or_default();
                        if key.eq_ignore_ascii_case("Path") {
                            let val = std::str::from_utf8(&attr.value).unwrap_or_default();
                            let mut proj_path = project_path.to_path_buf();
                            for part in val.split(|c| c == '/' || c == '\\') {
                                if !part.is_empty() {
                                    proj_path.push(part);
                                }
                            }
                            if proj_path.exists() {
                                projects.push(proj_path);
                            }
                        }
                    }
                }
            }
            Ok(quick_xml::events::Event::Eof) => break,
            Err(e) => anyhow::bail!(
                "XML parse error at position {}: {e}",
                reader.error_position()
            ),
            _ => {}
        }
    }

    projects.sort();
    Ok(projects)
}

fn build_project(csproj: &Path, stop_dir: &Path) -> Result<Project> {
    let direct_dependencies = parser::parse_nuget_project(csproj, stop_dir)?;
    let dependencies_graph = csproj
        .parent()
        .map(|parent| parent.join("obj").join("project.assets.json"))
        .filter(|assets_path| assets_path.exists())
        .map(|assets_path| graph::build_graph_from_assets(&assets_path))
        .transpose()?
        .unwrap_or_else(|| DependencyGraph { nodes: Vec::new() });
    let name = csproj
        .file_stem()
        .map(|s| s.to_string_lossy().to_string())
        .unwrap_or_else(|| "NuGet Project".to_string());

    Ok(Project {
        name,
        manifest_file: csproj.to_path_buf(),
        direct_dependencies,
        dependencies_graph,
    })
}

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Debug)]
struct NugetVersion {
    major: u64,
    minor: u64,
    patch: u64,
    revision: u64,
}

impl NugetVersion {
    fn parse(s: &str) -> Option<Self> {
        // Strip out pre-release suffix for base version comparison
        let base = s.split('-').next().unwrap_or(s);
        let parts: Vec<&str> = base.split('.').collect();
        if parts.is_empty() || parts.len() > 4 {
            return None;
        }
        let major = parts.get(0).unwrap_or(&"0").parse().ok()?;
        let minor = parts.get(1).unwrap_or(&"0").parse().ok()?;
        let patch = parts.get(2).unwrap_or(&"0").parse().ok()?;
        let revision = parts.get(3).unwrap_or(&"0").parse().ok()?;
        Some(Self {
            major,
            minor,
            patch,
            revision,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_outdated() {
        let provider = NuGetProvider::new();

        // With resolved version, latest controls the result.
        assert!(provider.is_outdated("1.0.0", Some("1.0.0"), "1.0.5"));
        assert!(!provider.is_outdated("1.0.0", Some("1.0.5"), "1.0.5"));
        assert!(provider.is_outdated("1.0.0", Some("1.0.0"), "1.0.0.1"));

        assert!(!provider.is_outdated("[1, 2)", Some("1.5.1"), "1.5.1"));
        assert!(provider.is_outdated("[1, 2)", Some("1.5.1"), "1.6.0"));

        // Without resolved version, declared range controls the result.
        assert!(provider.is_outdated("1.0.0", None, "2.0.0")); // outdated, nuget restores lowest bound 1.0.0
        assert!(provider.is_outdated("[1.0.0]", None, "2.0.0")); // strictly 1.0.0
        assert!(provider.is_outdated("[1.0.0, 2.0.0)", None, "2.0.0")); // outdated, max is 2.0.0 exclusive
        assert!(!provider.is_outdated("[1.0.0, 2.0.0]", None, "1.5.0")); // not outdated, allow range
        assert!(provider.is_outdated("[1.0.0, 2.0.0]", None, "2.0.1")); // outdated
        
        assert!(!provider.is_outdated("[1, 2)", None, "1.9.9"));
        assert!(provider.is_outdated("[1, 2)", None, "2.0.0"));
        assert!(!provider.is_outdated("2", None, "2.9.9"));
        assert!(provider.is_outdated("2", None, "3.0.0"));
    }
}
