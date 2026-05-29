use std::path::Path;

use anyhow::{Context, Result};
use quick_xml::events::Event;
use quick_xml::Reader;

use crate::core::models::{Dependency, DependencySource, Ecosystem};

/// Parsed package reference from a .csproj or Directory.Packages.props file.
#[derive(Debug)]
struct PackageRef {
    name: String,
    version: String,
}

/// Parse all NuGet dependencies from the given manifest files.
///
/// Handles two formats:
/// - `.csproj`: `<PackageReference Include="..." Version="..." />`
/// - `Directory.Packages.props`: `<PackageVersion Include="..." Version="..." />`
pub fn parse_nuget_manifests(
    _project_path: &Path,
    manifest_files: &[std::path::PathBuf],
) -> Result<Vec<Dependency>> {
    let mut dependencies = Vec::new();

    for file_path in manifest_files {
        let content = std::fs::read_to_string(file_path)
            .with_context(|| format!("Failed to read '{}'", file_path.display()))?;

        let file_name = file_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();

        let refs = if file_name == "Directory.Packages.props" {
            parse_package_versions(&content)?
        } else {
            parse_package_references(&content)?
        };

        for pkg_ref in refs {
            dependencies.push(Dependency {
                name: pkg_ref.name,
                declared_version: pkg_ref.version,
                resolved_version: None, // No lock file for NuGet
                source: DependencySource {
                    manifest_file: file_path.clone(),
                    lock_file: None,
                    ecosystem: Ecosystem::NuGet,
                },
            });
        }
    }

    // Deduplicate: if a package appears in Directory.Packages.props, prefer that.
    // This handles the CPM case where .csproj has PackageReference without Version
    // and Directory.Packages.props has the versioned PackageVersion.
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

/// Generic XML element parser that extracts Include and Version attributes
/// from the specified element name.
fn parse_xml_elements(xml_content: &str, element_name: &str) -> Result<Vec<PackageRef>> {
    let mut reader = Reader::from_str(xml_content);
    let mut refs = Vec::new();

    loop {
        match reader.read_event() {
            Ok(Event::Empty(ref e)) | Ok(Event::Start(ref e)) => {
                let local_name_raw = e.local_name();
                let local_name = std::str::from_utf8(local_name_raw.as_ref())
                    .unwrap_or_default();

                if local_name.eq_ignore_ascii_case(element_name) {
                    let mut include = None;
                    let mut version = None;

                    for attr in e.attributes().flatten() {
                        let key = std::str::from_utf8(attr.key.as_ref())
                            .unwrap_or_default();
                        let val = std::str::from_utf8(&attr.value)
                            .unwrap_or_default()
                            .to_string();

                        if key.eq_ignore_ascii_case("Include") {
                            include = Some(val);
                        } else if key.eq_ignore_ascii_case("Version") {
                            version = Some(val);
                        }
                    }

                    // Only include if both name and version are present.
                    // In CPM, .csproj PackageReference may omit Version.
                    if let (Some(name), Some(ver)) = (include, version) {
                        if !ver.is_empty() {
                            refs.push(PackageRef {
                                name,
                                version: ver,
                            });
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(e) => {
                anyhow::bail!("XML parse error at position {}: {e}", reader.error_position());
            }
            _ => {}
        }
    }

    Ok(refs)
}

/// Remove duplicate package entries. If the same package appears in both
/// Directory.Packages.props and a .csproj, keep the one from Directory.Packages.props
/// (which has the authoritative version in CPM scenarios).
fn deduplicate_dependencies(deps: &mut Vec<Dependency>) {
    let mut seen: std::collections::HashMap<String, (usize, bool)> = std::collections::HashMap::new();

    for (i, dep) in deps.iter().enumerate() {
        let key = dep.name.to_lowercase();
        let is_props = dep
            .source
            .manifest_file
            .file_name()
            .map(|f| f == "Directory.Packages.props")
            .unwrap_or(false);

        match seen.get(&key) {
            Some(&(_, prev_is_props)) => {
                // Prefer Directory.Packages.props over .csproj
                if is_props && !prev_is_props {
                    seen.insert(key, (i, true));
                }
            }
            None => {
                seen.insert(key, (i, is_props));
            }
        }
    }

    let keep_indices: std::collections::HashSet<usize> =
        seen.values().map(|&(i, _)| i).collect();

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
        assert_eq!(refs[0].version, "13.0.3");
        assert_eq!(refs[1].name, "Serilog");
        assert_eq!(refs[1].version, "3.1.1");
        assert_eq!(refs[2].name, "Microsoft.Extensions.Logging");
        assert_eq!(refs[2].version, "8.0.*");
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
        assert_eq!(refs[0].version, "13.0.3");
    }

    #[test]
    fn test_parse_csproj_without_version_skipped() {
        // In CPM mode, .csproj files may have PackageReference without Version
        let xml = r#"
<Project Sdk="Microsoft.NET.Sdk">
  <ItemGroup>
    <PackageReference Include="Newtonsoft.Json" />
    <PackageReference Include="Serilog" Version="3.1.1" />
  </ItemGroup>
</Project>"#;

        let refs = parse_package_references(xml).unwrap();
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].name, "Serilog");
    }
}
