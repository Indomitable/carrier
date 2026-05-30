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
pub struct NuGetProvider;

impl NuGetProvider {
    pub fn new() -> Self {
        Self
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
        agent: &ureq::Agent,
        package_name: &str,
    ) -> Result<Option<String>> {
        registry::get_latest_stable_version(agent, package_name)
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
