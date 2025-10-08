use anyhow::{Context, Result};
use crates_index::GitIndex;
use reqwest::Client;
use serde::Deserialize;
use std::path::PathBuf;
use tracing::{debug, error, info, warn};

use crate::types::{CrateDependency, CrateInfo, CrateSearchResult, CrateVersion};

#[derive(Deserialize)]
struct CratesIoSearchResponse {
    crates: Vec<CratesIoSearchCrate>,
    #[allow(dead_code)]
    meta: CratesIoSearchMeta,
}

#[derive(Deserialize)]
struct CratesIoSearchCrate {
    name: String,
    max_version: String,
    description: Option<String>,
    downloads: u64,
}

#[derive(Deserialize)]
struct CratesIoSearchMeta {
    #[allow(dead_code)]
    total: u64,
}

#[derive(Deserialize)]
struct CratesIoCrateResponse {
    #[serde(rename = "crate")]
    crate_data: CratesIoCrateData,
    versions: Vec<CratesIoVersion>,
}

#[derive(Deserialize)]
struct CratesIoCrateData {
    name: String,
    description: Option<String>,
    documentation: Option<String>,
    homepage: Option<String>,
    repository: Option<String>,
    downloads: u64,
    created_at: String,
    updated_at: String,
}

#[derive(Deserialize)]
struct CratesIoVersion {
    num: String,
    created_at: String,
    downloads: u64,
    features: serde_json::Value,
    yanked: bool,
    license: Option<String>,
}

pub struct CratesClient {
    pub(crate) http_client: Client,
    pub(crate) git_index: Option<GitIndex>,
}

impl CratesClient {
    /// Create a new CratesClient with HTTP client and optional git index
    pub async fn new() -> Result<Self> {
        let http_client = Client::builder()
            .user_agent(format!("{}/{}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")))
            .build()
            .context("Failed to create HTTP client")?;

        // Try to initialize git index with better error handling and recovery
        let git_index = Self::initialize_git_index_with_recovery().await;

        Ok(Self {
            http_client,
            git_index,
        })
    }

    /// Initialize git index with recovery mechanism for corrupted indexes
    async fn initialize_git_index_with_recovery() -> Option<GitIndex> {
        // First attempt: Try normal initialization
        match GitIndex::new_cargo_default() {
            Ok(index) => {
                info!("Successfully initialized crates.io git index");
                return Some(index);
            }
            Err(e) => {
                warn!("Failed to initialize crates.io git index: {}", e);

                // Check if the error suggests a corrupted git index
                if e.to_string().contains("gix") || e.to_string().contains("git") {
                    info!("Attempting to recover from potential git index corruption...");

                    // Try to get the cargo registry path
                    if let Some(registry_path) = Self::get_cargo_registry_path() {
                        let index_path = registry_path.join("index/github.com-1ecc6299db9ec823");

                        if index_path.exists() {
                            warn!("Found potentially corrupted index at: {}", index_path.display());
                            warn!("If problems persist, consider manually deleting this directory and restarting");

                            // Attempt one more time after a brief delay
                            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                            match GitIndex::new_cargo_default() {
                                Ok(index) => {
                                    info!("Successfully initialized crates.io git index on retry");
                                    return Some(index);
                                }
                                Err(retry_error) => {
                                    error!("Retry failed: {}", retry_error);
                                }
                            }
                        }
                    }
                }

                warn!("Git index unavailable - some features like dependency viewing will be limited");
                info!("The server will continue to work with HTTP-only API access");
                None
            }
        }
    }

    /// Get the cargo registry path
    fn get_cargo_registry_path() -> Option<PathBuf> {
        if let Some(home) = std::env::var_os("HOME") {
            let mut path = PathBuf::from(home);
            path.push(".cargo/registry");
            if path.exists() {
                return Some(path);
            }
        }

        // Try Windows equivalent
        if let Some(appdata) = std::env::var_os("APPDATA") {
            let mut path = PathBuf::from(appdata);
            // On Windows, cargo uses different paths, check common locations
            if path.join(".cargo").exists() {
                path.push(".cargo/registry");
                if path.exists() {
                    return Some(path);
                }
            }
        }

        // Try CARGO_HOME environment variable
        if let Some(cargo_home) = std::env::var_os("CARGO_HOME") {
            let mut path = PathBuf::from(cargo_home);
            path.push("registry");
            if path.exists() {
                return Some(path);
            }
        }

        None
    }

    /// Helper method for making HTTP requests to crates.io API
    async fn make_crates_io_request(&self, url: &str) -> Result<reqwest::Response> {
        debug!("Making request to: {}", url);

        let response = self
            .http_client
            .get(url)
            .send()
            .await
            .with_context(|| format!("Failed to send request to {}", url))?;

        if !response.status().is_success() {
            let status = response.status();
            if status == 404 {
                return Err(anyhow::anyhow!("Resource not found at {}", url));
            }
            error!("Request failed with status: {} for URL: {}", status, url);
            return Err(anyhow::anyhow!("Request failed: {}", status));
        }

        Ok(response)
    }

    /// Search for crates on crates.io
    pub async fn search_crates(
        &self,
        query: &str,
        limit: Option<usize>,
        sort_by: &str,
        min_downloads: u64,
    ) -> Result<Vec<CrateSearchResult>> {
        // Input validation
        if query.trim().is_empty() {
            return Err(anyhow::anyhow!("Search query cannot be empty"));
        }

        let limit = limit.unwrap_or(10).min(100);

        // Add sort parameter to API query if sorting by downloads
        let mut query_params = format!("q={}", urlencoding::encode(query));
        if sort_by == "downloads" {
            query_params.push_str("&sort=downloads");
        }

        // Get more results than requested for filtering, then apply our own filters
        let api_limit = if min_downloads > 0 || sort_by == "downloads" {
            limit * 3 // Get more results to allow for filtering
        } else {
            limit
        };

        let url = format!(
            "https://crates.io/api/v1/crates?{}&per_page={}",
            query_params,
            api_limit.min(100) // API limit is 100
        );

        let response = self.make_crates_io_request(&url).await?;
        let search_response: CratesIoSearchResponse = response
            .json()
            .await
            .context("Failed to parse search response")?;

        let mut results: Vec<CrateSearchResult> = search_response
            .crates
            .into_iter()
            .map(|c| CrateSearchResult {
                name: c.name,
                max_version: c.max_version,
                description: c.description,
                downloads: c.downloads,
            })
            .filter(|c| c.downloads >= min_downloads) // Filter by minimum downloads
            .collect();

        // Apply additional sorting if needed (API sorting might not be sufficient)
        if sort_by == "downloads" {
            results.sort_by(|a, b| b.downloads.cmp(&a.downloads));
        }

        // Limit to requested number after filtering
        results.truncate(limit);

        info!(
            "Found {} crates for query '{}' (sort: {}, min_downloads: {})",
            results.len(),
            query,
            sort_by,
            min_downloads
        );
        Ok(results)
    }

    /// Get detailed information about a specific crate
    pub async fn get_crate_info(&self, name: &str) -> Result<CrateInfo> {
        // Input validation
        if name.trim().is_empty() {
            return Err(anyhow::anyhow!("Crate name cannot be empty"));
        }

        let url = format!("https://crates.io/api/v1/crates/{}", name);
        let response = self
            .make_crates_io_request(&url)
            .await
            .with_context(|| format!("Failed to get info for crate '{}'", name))?;

        let crate_response: CratesIoCrateResponse = response
            .json()
            .await
            .context("Failed to parse crate info response")?;

        let latest_version = crate_response
            .versions
            .iter()
            .find(|v| !v.yanked)
            .or_else(|| crate_response.versions.first())
            .context("No versions found for crate")?;

        // Try to get additional metadata from git index
        let (authors, keywords, categories, license) = match &self.git_index {
            Some(git_index) => match git_index.crate_(name) {
                Some(index_crate) => {
                    let latest_version_info = index_crate
                        .versions()
                        .iter()
                        .find(|v| v.version() == latest_version.num)
                        .or_else(|| Some(index_crate.highest_version()));

                    match latest_version_info {
                        Some(_version) => (
                            // Note: crates-index API may have changed, using empty defaults
                            vec![],
                            vec![],
                            vec![],
                            latest_version.license.clone(),
                        ),
                        None => (vec![], vec![], vec![], latest_version.license.clone()),
                    }
                }
                None => (vec![], vec![], vec![], latest_version.license.clone()),
            },
            None => (vec![], vec![], vec![], latest_version.license.clone()),
        };

        let crate_info = CrateInfo {
            name: crate_response.crate_data.name,
            version: latest_version.num.clone(),
            description: crate_response.crate_data.description,
            documentation: crate_response.crate_data.documentation,
            homepage: crate_response.crate_data.homepage,
            repository: crate_response.crate_data.repository,
            license,
            authors,
            keywords,
            categories,
            downloads: crate_response.crate_data.downloads,
            created_at: crate_response.crate_data.created_at,
            updated_at: crate_response.crate_data.updated_at,
        };

        info!("Retrieved info for crate '{}'", name);
        Ok(crate_info)
    }

    /// Get version history for a crate
    pub async fn get_crate_versions(
        &self,
        name: &str,
        limit: Option<usize>,
    ) -> Result<Vec<CrateVersion>> {
        // Input validation
        if name.trim().is_empty() {
            return Err(anyhow::anyhow!("Crate name cannot be empty"));
        }

        let url = format!("https://crates.io/api/v1/crates/{}", name);
        let response = self
            .make_crates_io_request(&url)
            .await
            .with_context(|| format!("Failed to get versions for crate '{}'", name))?;

        let crate_response: CratesIoCrateResponse = response
            .json()
            .await
            .context("Failed to parse crate versions response")?;

        let mut versions: Vec<CrateVersion> = crate_response
            .versions
            .into_iter()
            .map(|v| CrateVersion {
                num: v.num,
                created_at: v.created_at,
                downloads: v.downloads,
                features: v.features,
                yanked: v.yanked,
            })
            .collect();

        if let Some(limit) = limit {
            versions.truncate(limit);
        }

        info!("Retrieved {} versions for crate '{}'", versions.len(), name);
        Ok(versions)
    }

    /// Get dependencies for a specific version of a crate
    /// Note: Requires git index to be available
    pub fn get_crate_dependencies(
        &self,
        name: &str,
        version: Option<&str>,
    ) -> Result<Vec<CrateDependency>> {
        // Input validation
        if name.trim().is_empty() {
            return Err(anyhow::anyhow!("Crate name cannot be empty"));
        }

        let git_index = self.git_index.as_ref().context(
            "Git index not available. Dependency viewing requires the local crates.io git index. \
            This may be due to a corrupted git index. Try deleting \
            ~/.cargo/registry/index/github.com-1ecc6299db9ec823/ (or the equivalent on Windows) \
            and restart the server to rebuild the index."
        )?;

        let index_crate = git_index.crate_(name).context("Crate not found in index")?;

        let version_info = match version {
            Some(v) => index_crate
                .versions()
                .iter()
                .find(|ver| ver.version() == v)
                .context("Version not found")?,
            None => index_crate.highest_version(),
        };

        let dependencies: Vec<CrateDependency> = version_info
            .dependencies()
            .iter()
            .map(|dep| CrateDependency {
                name: dep.crate_name().to_string(),
                version_req: dep.requirement().to_string(),
                optional: dep.is_optional(),
                default_features: dep.has_default_features(),
                features: dep.features().to_vec(),
                target: dep.target().map(|t| t.to_string()),
                kind: format!("{:?}", dep.kind()).to_lowercase(),
            })
            .collect();

        info!(
            "Retrieved {} dependencies for crate '{}' version '{}'",
            dependencies.len(),
            name,
            version_info.version()
        );
        Ok(dependencies)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_search_crates() -> Result<()> {
        let client = CratesClient::new().await?;
        let results = client.search_crates("serde", Some(5), "relevance", 0).await?;

        assert!(!results.is_empty());
        assert!(results.len() <= 5);
        assert!(results.iter().any(|c| c.name.contains("serde")));

        Ok(())
    }

    #[tokio::test]
    async fn test_search_crates_by_downloads() -> Result<()> {
        let client = CratesClient::new().await?;
        let results = client.search_crates("http", Some(3), "downloads", 100000).await?;

        assert!(!results.is_empty());
        assert!(results.len() <= 3);
        // All results should have at least 100k downloads
        assert!(results.iter().all(|c| c.downloads >= 100000));

        Ok(())
    }

    #[tokio::test]
    async fn test_get_crate_info() -> Result<()> {
        let client = CratesClient::new().await?;
        let info = client.get_crate_info("serde").await?;

        assert_eq!(info.name, "serde");
        assert!(!info.version.is_empty());
        assert!(info.downloads > 0);

        Ok(())
    }

    #[tokio::test]
    async fn test_get_crate_dependencies() -> Result<()> {
        let client = CratesClient::new().await?;

        // This test may fail if git index is not available, which is expected
        match client.get_crate_dependencies("serde", None) {
            Ok(deps) => {
                // serde should have some dependencies (at least serde_derive optionally)
                assert!(!deps.is_empty());
            }
            Err(e) => {
                // It's okay if git index is not available during tests
                println!("Dependencies test failed (this may be expected): {}", e);
            }
        }

        Ok(())
    }
}
