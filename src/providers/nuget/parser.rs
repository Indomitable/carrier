use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use quick_xml::events::Event;
use quick_xml::Reader;

use crate::core::models::{Dependency, DependencySource, Ecosystem};

/// Parsed package reference from a .csproj or Directory.Packages.props file.
#[derive(Debug)]
struct PackageRef {
    name: String,
    version: Option<String>,
    version_override: Option<String>,
}

pub(super) fn parse_nuget_project(csproj_path: &Path, stop_dir: &Path) -> Result<Vec<Dependency>> {
    let content = std::fs::read_to_string(csproj_path)
        .with_context(|| format!("Failed to read '{}'", csproj_path.display()))?;
    let refs = parse_package_references(&content)?;

    let needs_cpm = refs
        .iter()
        .any(|pkg_ref| pkg_ref.version.is_none() && pkg_ref.version_override.is_none());
    let props_file = if needs_cpm {
        Some(find_directory_packages_props(csproj_path, stop_dir)?.ok_or_else(|| {
            anyhow::anyhow!(
                "Project uses central package management but no Directory.Packages.props file is found."
            )
        })?)
    } else {
        find_directory_packages_props(csproj_path, stop_dir)?
    };
    let props_versions = match &props_file {
        Some(path) => parse_directory_packages_props(path)?,
        None => HashMap::new(),
    };

    let mut dependencies = Vec::new();
    for pkg_ref in refs {
        let declared_version = if let Some(version_override) = pkg_ref.version_override {
            version_override
        } else if let Some(version) = pkg_ref.version {
            version
        } else {
            props_versions
                .get(&pkg_ref.name.to_lowercase())
                .cloned()
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "Directory.Packages.props does not contain a version for '{}'.",
                        pkg_ref.name
                    )
                })?
        };

        dependencies.push(Dependency {
            name: pkg_ref.name,
            declared_version,
            resolved_version: None,
            source: DependencySource {
                manifest_file: csproj_path.to_path_buf(),
                lock_file: None,
                ecosystem: Ecosystem::NuGet,
            },
        });
    }

    deduplicate_dependencies(&mut dependencies);

    Ok(dependencies)
}

/// Parse `<PackageReference Include="..." Version="..." />` from a .csproj file.
fn parse_package_references(xml_content: &str) -> Result<Vec<PackageRef>> {
    parse_xml_elements(xml_content, "PackageReference")
}

/// Parse `<PackageVersion Include="..." Version="..." />` from Directory.Packages.props.
fn parse_package_versions(xml_content: &str) -> Result<Vec<PackageRef>> {
    parse_xml_elements(xml_content, "PackageVersion")
}

fn parse_directory_packages_props(props_path: &Path) -> Result<HashMap<String, String>> {
    let content = std::fs::read_to_string(props_path)
        .with_context(|| format!("Failed to read '{}'", props_path.display()))?;
    let mut versions = HashMap::new();

    for package_version in parse_package_versions(&content)? {
        if let Some(version) = package_version.version {
            versions.insert(package_version.name.to_lowercase(), version);
        }
    }

    Ok(versions)
}

fn find_directory_packages_props(csproj_path: &Path, stop_dir: &Path) -> Result<Option<PathBuf>> {
    let mut current = csproj_path.parent().ok_or_else(|| {
        anyhow::anyhow!(
            "Project '{}' has no parent directory.",
            csproj_path.display()
        )
    })?;

    loop {
        let props_path = current.join("Directory.Packages.props");
        if props_path.exists() {
            return Ok(Some(props_path));
        }

        if current == stop_dir {
            return Ok(None);
        }

        let Some(parent) = current.parent() else {
            return Ok(None);
        };
        current = parent;
    }
}

/// Generic XML element parser that extracts Include and Version attributes
/// from the specified element name.
fn parse_xml_elements(xml_content: &str, element_name: &str) -> Result<Vec<PackageRef>> {
    let mut reader = Reader::from_str(xml_content);
    let mut refs = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Empty(ref e)) | Ok(Event::Start(ref e)) => {
                let local_name_raw = e.local_name();
                let local_name = std::str::from_utf8(local_name_raw.as_ref()).unwrap_or_default();

                if local_name.eq_ignore_ascii_case(element_name) {
                    let mut include = None;
                    let mut version = None;
                    let mut version_override = None;

                    for attr in e.attributes().flatten() {
                        let key = std::str::from_utf8(attr.key.as_ref()).unwrap_or_default();
                        let val = std::str::from_utf8(&attr.value)
                            .unwrap_or_default()
                            .to_string();

                        if key.eq_ignore_ascii_case("Include") {
                            include = Some(val);
                        } else if key.eq_ignore_ascii_case("Version") {
                            version = Some(val);
                        } else if key.eq_ignore_ascii_case("VersionOverride") {
                            version_override = Some(val);
                        }
                    }

                    if let Some(name) = include {
                        refs.push(PackageRef {
                            name,
                            version: version.filter(|v| !v.is_empty()),
                            version_override: version_override.filter(|v| !v.is_empty()),
                        });
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                anyhow::bail!(
                    "XML parse error at position {}: {e}",
                    reader.error_position()
                );
            }
            _ => {}
        }
    }

    Ok(refs)
}

/// Remove duplicate package entries by keeping the last declaration.
fn deduplicate_dependencies(deps: &mut Vec<Dependency>) {
    let mut seen: HashMap<String, usize> = HashMap::new();

    for (i, dep) in deps.iter().enumerate() {
        let key = dep.name.to_lowercase();
        seen.insert(key, i);
    }

    let keep_indices: std::collections::HashSet<usize> = seen.values().copied().collect();

    let mut i = 0;
    deps.retain(|_| {
        let keep = keep_indices.contains(&i);
        i += 1;
        keep
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_project_dir() -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("carrier-nuget-test-{suffix}"));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn test_parse_csproj_package_references() {
        let xml = r#"
<Project Sdk="Microsoft.NET.Sdk">
  <PropertyGroup>
    <TargetFramework>net8.0</TargetFramework>
  </PropertyGroup>
  <ItemGroup>
    <PackageReference Include="Newtonsoft.Json" Version="13.0.3" />
    <PackageReference Include="Serilog" Version="3.1.1" />
    <PackageReference Include="Microsoft.Extensions.Logging" Version="8.0.*" />
  </ItemGroup>
</Project>"#;

        let refs = parse_package_references(xml).unwrap();
        assert_eq!(refs.len(), 3);
        assert_eq!(refs[0].name, "Newtonsoft.Json");
        assert_eq!(refs[0].version.as_deref(), Some("13.0.3"));
        assert_eq!(refs[1].name, "Serilog");
        assert_eq!(refs[1].version.as_deref(), Some("3.1.1"));
        assert_eq!(refs[2].name, "Microsoft.Extensions.Logging");
        assert_eq!(refs[2].version.as_deref(), Some("8.0.*"));
    }

    #[test]
    fn test_parse_directory_packages_props() {
        let xml = r#"
<Project>
  <PropertyGroup>
    <ManagePackageVersionsCentrally>true</ManagePackageVersionsCentrally>
  </PropertyGroup>
  <ItemGroup>
    <PackageVersion Include="Newtonsoft.Json" Version="13.0.3" />
    <PackageVersion Include="Serilog" Version="3.1.1" />
  </ItemGroup>
</Project>"#;

        let refs = parse_package_versions(xml).unwrap();
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].name, "Newtonsoft.Json");
        assert_eq!(refs[0].version.as_deref(), Some("13.0.3"));
    }

    #[test]
    fn test_parse_csproj_without_version_kept_for_cpm_resolution() {
        let xml = r#"
<Project Sdk="Microsoft.NET.Sdk">
  <ItemGroup>
    <PackageReference Include="Newtonsoft.Json" />
    <PackageReference Include="Serilog" Version="3.1.1" />
  </ItemGroup>
</Project>"#;

        let refs = parse_package_references(xml).unwrap();
        assert_eq!(refs.len(), 2);
        assert_eq!(refs[0].name, "Newtonsoft.Json");
        assert_eq!(refs[0].version, None);
        assert_eq!(refs[1].name, "Serilog");
        assert_eq!(refs[1].version.as_deref(), Some("3.1.1"));
    }

    #[test]
    fn test_parse_version_override() {
        let xml = r#"
<Project Sdk="Microsoft.NET.Sdk">
  <ItemGroup>
    <PackageReference Include="Newtonsoft.Json" Version="13.0.1" VersionOverride="13.0.3" />
  </ItemGroup>
</Project>"#;

        let refs = parse_package_references(xml).unwrap();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].version.as_deref(), Some("13.0.1"));
        assert_eq!(refs[0].version_override.as_deref(), Some("13.0.3"));
    }

    #[test]
    fn parse_nuget_project_resolves_versions_from_props() {
        let project_dir = temp_project_dir();
        let csproj_path = project_dir.join("Sample.csproj");
        fs::write(
            &csproj_path,
            r#"
<Project Sdk="Microsoft.NET.Sdk">
  <ItemGroup>
    <PackageReference Include="Newtonsoft.Json" />
  </ItemGroup>
</Project>"#,
        )
        .unwrap();
        fs::write(
            project_dir.join("Directory.Packages.props"),
            r#"
<Project>
  <ItemGroup>
    <PackageVersion Include="Newtonsoft.Json" Version="13.0.3" />
  </ItemGroup>
</Project>"#,
        )
        .unwrap();

        let deps = parse_nuget_project(&csproj_path, &project_dir).unwrap();

        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].name, "Newtonsoft.Json");
        assert_eq!(deps[0].declared_version, "13.0.3");

        fs::remove_dir_all(project_dir).unwrap();
    }

    #[test]
    fn parse_nuget_project_uses_version_override_before_props() {
        let project_dir = temp_project_dir();
        let csproj_path = project_dir.join("Sample.csproj");
        fs::write(
            &csproj_path,
            r#"
<Project Sdk="Microsoft.NET.Sdk">
  <ItemGroup>
    <PackageReference Include="Newtonsoft.Json" VersionOverride="13.0.4" />
  </ItemGroup>
</Project>"#,
        )
        .unwrap();
        fs::write(
            project_dir.join("Directory.Packages.props"),
            r#"
<Project>
  <ItemGroup>
    <PackageVersion Include="Newtonsoft.Json" Version="13.0.3" />
  </ItemGroup>
</Project>"#,
        )
        .unwrap();

        let deps = parse_nuget_project(&csproj_path, &project_dir).unwrap();

        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0].declared_version, "13.0.4");

        fs::remove_dir_all(project_dir).unwrap();
    }

    #[test]
    fn parse_nuget_project_errors_without_props_for_cpm_dependency() {
        let project_dir = temp_project_dir();
        let csproj_path = project_dir.join("Sample.csproj");
        fs::write(
            &csproj_path,
            r#"
<Project Sdk="Microsoft.NET.Sdk">
  <ItemGroup>
    <PackageReference Include="Newtonsoft.Json" />
  </ItemGroup>
</Project>"#,
        )
        .unwrap();

        let err = parse_nuget_project(&csproj_path, &project_dir).unwrap_err();

        assert!(err
            .to_string()
            .contains("Project uses central package management"));

        fs::remove_dir_all(project_dir).unwrap();
    }
}
