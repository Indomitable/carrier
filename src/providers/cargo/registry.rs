use anyhow::{Context, Result};
use serde::Deserialize;

const CRATES_IO_BASE: &str = "https://crates.io/api/v1/crates";

#[derive(Debug, Deserialize)]
struct CrateResponse {
    #[serde(rename = "crate")]
    krate: CrateDetails,
}

#[derive(Debug, Deserialize)]
struct CrateDetails {
    max_stable_version: Option<String>,
}

/// Query crates.io for the latest stable version of a crate.
///
/// Returns `Ok(None)` if the crate is not found or has no stable version.
pub fn get_latest_stable_version(
    agent: &ureq::Agent,
    package_name: &str,
) -> Result<Option<String>> {
    let url = format!("{CRATES_IO_BASE}/{package_name}");
    let response = agent.get(&url).call();

    match response {
        Ok(resp) => {
            let body: CrateResponse = resp.into_body().read_json().with_context(|| {
                format!("Failed to parse crates.io API response for '{package_name}'")
            })?;

            Ok(body.krate.max_stable_version)
        }
        Err(ureq::Error::StatusCode(404)) => Ok(None),
        Err(e) => Err(anyhow::anyhow!(
            "HTTP request failed for crate '{}': {}",
            package_name,
            e
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_max_stable_version() {
        let json = r#"
{
  "crate": {
    "id": "serde",
    "max_version": "1.0.228",
    "max_stable_version": "1.0.228"
  }
}
"#;

        let response: CrateResponse = serde_json::from_str(json).unwrap();

        assert_eq!(
            response.krate.max_stable_version,
            Some("1.0.228".to_string())
        );
    }
}
