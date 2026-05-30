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
    
    let mut attempts = 0;
    let max_attempts = 3;

    loop {
        attempts += 1;
        let response = agent.get(&url).call();

        match response {
            Ok(resp) => {
                let body: CrateResponse = resp.into_body().read_json().with_context(|| {
                    format!("Failed to parse crates.io API response for '{package_name}'")
                })?;

                return Ok(body.krate.max_stable_version);
            }
            Err(ureq::Error::StatusCode(404)) => return Ok(None),
            Err(e) => {
                if attempts >= max_attempts {
                    return Err(anyhow::anyhow!(
                        "HTTP request failed for crate '{}' after {} attempts: {}",
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
