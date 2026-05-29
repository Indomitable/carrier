pub mod assets;
pub mod parser;
pub mod registry;

use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::core::models::{Dependency, Ecosystem};
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

    fn detect(&self, project_path: &Path) -> Option<Vec<PathBuf>> {
        let mut manifest_files = Vec::new();

        // Check for Directory.Packages.props (Central Package Management)
        let props_path = project_path.join("Directory.Packages.props");
        if props_path.exists() {
            manifest_files.push(props_path);
        }

        let mut slnx_found = false;

        // Check for .slnx files
        if let Ok(entries) = std::fs::read_dir(project_path) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    if let Some(ext) = path.extension() {
                        if ext.eq_ignore_ascii_case("slnx") {
                            slnx_found = true;
                            if let Ok(content) = std::fs::read_to_string(&path) {
                                let mut reader = quick_xml::Reader::from_str(&content);
                                loop {
                                    match reader.read_event() {
                                        Ok(quick_xml::events::Event::Empty(ref e))
                                        | Ok(quick_xml::events::Event::Start(ref e)) => {
                                            let local_name_raw = e.local_name();
                                            let local_name =
                                                std::str::from_utf8(local_name_raw.as_ref())
                                                    .unwrap_or_default();
                                            if local_name.eq_ignore_ascii_case("Project") {
                                                for attr in e.attributes().flatten() {
                                                    let key =
                                                        std::str::from_utf8(attr.key.as_ref())
                                                            .unwrap_or_default();
                                                    if key.eq_ignore_ascii_case("Path") {
                                                        let val = std::str::from_utf8(&attr.value)
                                                            .unwrap_or_default();
                                                        let mut proj_path =
                                                            project_path.to_path_buf();
                                                        for part in
                                                            val.split(|c| c == '/' || c == '\\')
                                                        {
                                                            if !part.is_empty() {
                                                                proj_path.push(part);
                                                            }
                                                        }
                                                        // Always push, let the parser handle missing files or error if needed.
                                                        // Actually, checking exists() might be better. Let's do it.
                                                        if proj_path.exists() {
                                                            manifest_files.push(proj_path);
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                        Ok(quick_xml::events::Event::Eof) => break,
                                        Err(_) => break,
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        if !slnx_found {
            None
        } else {
            Some(manifest_files)
        }
    }

    fn parse_dependencies(
        &self,
        _project_path: &Path,
        manifest_files: &[PathBuf],
    ) -> Result<Vec<Dependency>> {
        parser::parse_nuget_manifests(_project_path, manifest_files)
    }

    fn get_latest_version(
        &self,
        agent: &ureq::Agent,
        package_name: &str,
    ) -> Result<Option<String>> {
        registry::get_latest_stable_version(agent, package_name)
    }

    fn why(&self, project_path: &Path, package_name: &str) -> Result<()> {
        let manifest_files = match self.detect(project_path) {
            Some(files) => files,
            None => anyhow::bail!("No .slnx solution found in '{}'.", project_path.display()),
        };

        let csproj_files: Vec<_> = manifest_files
            .into_iter()
            .filter(|p| {
                p.extension()
                    .map_or(false, |e| e.eq_ignore_ascii_case("csproj"))
            })
            .collect();

        if csproj_files.is_empty() {
            anyhow::bail!("No .csproj files found in the solution.");
        }

        let mut found_in_any = false;

        for csproj in csproj_files {
            if let Some(parent) = csproj.parent() {
                let assets_path = parent.join("obj").join("project.assets.json");
                if assets_path.exists() {
                    let result = assets::analyze_assets_for_why(&assets_path, package_name)?;
                    if result {
                        found_in_any = true;
                    }
                }
            }
        }

        if !found_in_any {
            println!(
                "Package '{}' was not found in any project.assets.json.",
                package_name
            );
        }

        Ok(())
    }
}
