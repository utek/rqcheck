use crate::config::Config;
use crate::error::{Result, RqCheckError};
use futures::stream::{self, StreamExt};
use serde::Deserialize;
use std::time::Duration;

#[derive(Deserialize, Debug)]
struct Info {
    version: String,
}

#[derive(Deserialize, Debug)]
struct PyPiResponse {
    info: Info,
}

/// Package requirement data (owned version for async processing)
#[derive(Debug, Clone)]
pub struct PackageRequirement {
    pub name: String,
    pub version: String,
}

/// Result of checking a single package
#[derive(Debug)]
pub struct PackageCheckResult {
    pub name: String,
    pub old_version: String,
    pub new_version: String,
    pub has_update: bool,
}

/// Check multiple packages in batches with async concurrency
pub async fn check_packages_batched(
    requirements: Vec<PackageRequirement>,
    config: &Config,
) -> Vec<PackageCheckResult> {
    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(config.timeout_seconds))
        .build()
        .expect("Failed to create HTTP client");

    let total = requirements.len();
    log::info!("Checking {} packages in batches of {}", total, config.batch_size);

    // Process requirements in batches
    let results: Vec<PackageCheckResult> = stream::iter(requirements)
        .map(|req| {
            let client = client.clone();
            let config = config.clone();
            async move { check_single_package(req, &client, &config).await }
        })
        .buffer_unordered(config.batch_size)
        .filter_map(|result| async move {
            match result {
                Ok(check_result) => Some(check_result),
                Err(e) => {
                    log::error!("{}", e);
                    None
                }
            }
        })
        .collect()
        .await;

    log::info!("Completed checking {} packages", results.len());
    results
}

/// Check a single package with retry logic
async fn check_single_package(
    req: PackageRequirement,
    client: &reqwest::Client,
    config: &Config,
) -> Result<PackageCheckResult> {
    if config.verbose {
        log::debug!("Checking package: {} (current: {})", req.name, req.version);
    }

    // Fetch latest version from PyPI with retries
    let new_version = fetch_latest_version(&req.name, client, config).await?;

    let has_update = new_version != req.version;

    Ok(PackageCheckResult {
        name: req.name,
        old_version: req.version,
        new_version,
        has_update,
    })
}

/// Fetch latest version from PyPI with retry logic
async fn fetch_latest_version(
    package_name: &str,
    client: &reqwest::Client,
    config: &Config,
) -> Result<String> {
    let url = format!("{}/{}/json", config.pypi_base_url, package_name);

    let mut last_error = None;

    for attempt in 0..=config.max_retries {
        if attempt > 0 {
            let delay = config.retry_delay_ms * (2_u64.pow(attempt - 1)); // Exponential backoff
            if config.verbose {
                log::debug!(
                    "Retry attempt {} for {} after {}ms",
                    attempt,
                    package_name,
                    delay
                );
            }
            tokio::time::sleep(Duration::from_millis(delay)).await;
        }

        match fetch_version_once(&url, package_name, client).await {
            Ok(version) => return Ok(version),
            Err(e) => {
                last_error = Some(e);
                if config.verbose {
                    log::warn!("Attempt {} failed for {}", attempt + 1, package_name);
                }
            }
        }
    }

    Err(last_error.unwrap_or_else(|| RqCheckError::InvalidRequirement(
        format!("Failed to fetch version for package '{}' after {} retries", package_name, config.max_retries)
    )))
}

/// Single attempt to fetch version from PyPI
async fn fetch_version_once(
    url: &str,
    package_name: &str,
    client: &reqwest::Client,
) -> Result<String> {
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|e| RqCheckError::NetworkError {
            package: package_name.to_string(),
            source: e,
        })?;

    if !response.status().is_success() {
        return Err(RqCheckError::HttpError {
            package: package_name.to_string(),
            status: response.status().as_u16(),
        });
    }

    let pypi_response = response
        .json::<PyPiResponse>()
        .await
        .map_err(|e| RqCheckError::NetworkError {
            package: package_name.to_string(),
            source: e,
        })?;

    let mut version = pypi_response.info.version;
    
    // Remove leading 'v' if present
    if version.starts_with('v') || version.starts_with('V') {
        version.remove(0);
    }

    Ok(version)
}

#[cfg(test)]
mod tests {
    use super::*;
    use mockito::Server;

    #[test]
    fn test_package_requirement_creation() {
        let req = PackageRequirement {
            name: "requests".to_string(),
            version: "2.28.0".to_string(),
        };
        assert_eq!(req.name, "requests");
        assert_eq!(req.version, "2.28.0");
    }

    #[tokio::test]
    async fn test_fetch_success() {
        let mut server = Server::new_async().await;
        
        let _m = server.mock("GET", "/requests/json")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"info": {"version": "2.32.0"}}"#)
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let mut config = Config::default();
        config.pypi_base_url = server.url();

        let result = fetch_latest_version("requests", &client, &config).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "2.32.0");
    }

    #[tokio::test]
    async fn test_fetch_404() {
        let mut server = Server::new_async().await;
        
        let _m = server.mock("GET", "/nonexistent/json")
            .with_status(404)
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let mut config = Config::default();
        config.pypi_base_url = server.url();

        let result = fetch_latest_version("nonexistent", &client, &config).await;
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), RqCheckError::HttpError { .. }));
    }

    #[tokio::test]
    async fn test_version_normalization() {
        let mut server = Server::new_async().await;
        
        let _m = server.mock("GET", "/package/json")
            .with_status(200)
            .with_header("content-type", "application/json")
            .with_body(r#"{"info": {"version": "v1.2.3"}}"#)
            .create_async()
            .await;

        let client = reqwest::Client::new();
        let mut config = Config::default();
        config.pypi_base_url = server.url();

        let result = fetch_latest_version("package", &client, &config).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "1.2.3"); // 'v' prefix removed
    }
}
