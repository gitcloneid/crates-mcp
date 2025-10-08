use serde::{Deserialize, Serialize};

/// Comprehensive information about a Rust crate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateInfo {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub documentation: Option<String>,
    pub homepage: Option<String>,
    pub repository: Option<String>,
    pub license: Option<String>,
    pub authors: Vec<String>,
    pub keywords: Vec<String>,
    pub categories: Vec<String>,
    pub downloads: u64,
    pub created_at: String,
    pub updated_at: String,
}

/// Search result for a crate from crates.io
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateSearchResult {
    pub name: String,
    pub max_version: String,
    pub description: Option<String>,
    pub downloads: u64,
}

/// Information about a specific version of a crate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateVersion {
    pub num: String,
    pub created_at: String,
    pub downloads: u64,
    pub features: serde_json::Value,
    pub yanked: bool,
}

/// A dependency of a crate
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateDependency {
    pub name: String,
    pub version_req: String,
    pub optional: bool,
    pub default_features: bool,
    pub features: Vec<String>,
    pub target: Option<String>,
    pub kind: String,
}

/// Documentation information for a crate from docs.rs
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CrateDocumentation {
    pub name: String,
    pub version: String,
    pub description: Option<String>,
    pub readme: Option<String>,
    pub modules: Vec<String>,
    pub items: Vec<DocumentationItem>,
}

/// An item in the crate documentation (function, struct, etc.)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DocumentationItem {
    pub name: String,
    pub kind: String,
    pub path: String,
    pub description: Option<String>,
}
