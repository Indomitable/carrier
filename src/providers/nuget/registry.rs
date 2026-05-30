use anyhow::{Context, Result};
use serde::Deserialize;

/// Response from the NuGet v3 flat container API.
/// GET https://api.nuget.org/v3-flatcontainer/{id}/index.json
#[derive(Debug, Deserialize)]
struct VersionsResponse {
    versions: Vec<String>,
}

const NUGET_FLAT_CONTAINER_BASE: &str = "https://api.nuget.org/v3-flatcontainer";

/// Query the NuGet v3 flat container API for the latest stable version of a package.
///
/// Returns `Ok(None)` if the package is not found (404).
/// Returns `Ok(Some(version))` with the highest non-prerelease version.
pub fn get_latest_stable_version(
    agent: &ureq::Agent,
    package_name: &str,
) -> Result<Option<String>> {
    let url = format!(
        "{}/{}/index.json",
        NUGET_FLAT_CONTAINER_BASE,
        package_name.to_lowercase()
    );

    let mut attempts = 0;
    let max_attempts = 3;

    loop {
        attempts += 1;
        let response = agent.get(&url).call();

        match response {
            Ok(resp) => {
                let body: VersionsResponse = resp.into_body().read_json().with_context(|| {
                    format!("Failed to parse NuGet API response for '{package_name}'")
                })?;

                // Filter out pre-release versions (those containing '-')
                // and find the highest stable version.
                let latest = body
                    .versions
                    .iter()
                    .filter(|v| !v.contains('-'))
                    .filter_map(|v| semver::Version::parse(v).ok().map(|parsed| (v, parsed)))
                    .max_by(|(_, a), (_, b)| a.cmp(b))
                    .map(|(original, _)| original.clone());

                return Ok(latest);
            }
            Err(ureq::Error::StatusCode(404)) => {
                // Package not found on nuget.org
                return Ok(None);
            }
            Err(e) => {
                if attempts >= max_attempts {
                    return Err(anyhow::anyhow!(
                        "HTTP request failed for package '{}' after {} attempts: {}",
                        package_name,
                        attempts,
                        e
                    ));
                }
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    // Integration tests would require network access.
    // Unit tests for version filtering logic:

    #[test]
    fn test_filter_prerelease() {
        let versions = vec![
            "1.0.0".to_string(),
            "2.0.0-beta1".to_string(),
            "2.0.0".to_string(),
            "3.0.0-rc1".to_string(),
        ];

        let latest = versions
            .iter()
            .filter(|v| !v.contains('-'))
            .filter_map(|v| semver::Version::parse(v).ok().map(|parsed| (v, parsed)))
            .max_by(|(_, a), (_, b)| a.cmp(b))
            .map(|(original, _)| original.clone());

        assert_eq!(latest, Some("2.0.0".to_string()));
    }
}
