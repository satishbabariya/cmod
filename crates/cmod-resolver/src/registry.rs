//! Module registry for ecosystem governance and module discovery.
//!
//! Implements RFC-0015: A Git-native module registry that indexes
//! available modules, their versions, and metadata for search and discovery.
//!
//! The registry is itself a Git repository containing an index of known modules.
//! This allows decentralized operation while providing a single discovery point.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use cmod_core::error::CmodError;

/// A module registry entry describing a published module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryEntry {
    /// Module name (reverse-domain format).
    pub name: String,
    /// Module description.
    pub description: Option<String>,
    /// Git repository URL.
    pub repository: String,
    /// Available versions (tag-based).
    pub versions: Vec<RegistryVersion>,
    /// Module keywords for search.
    pub keywords: Vec<String>,
    /// Module category.
    pub category: Option<String>,
    /// License identifier (SPDX).
    pub license: Option<String>,
    /// Module authors.
    pub authors: Vec<String>,
    /// When this entry was last updated.
    pub updated_at: String,
    /// Whether this module is verified/official.
    pub verified: bool,
    /// Deprecation notice (if deprecated).
    pub deprecated: Option<String>,
}

/// A specific version of a module in the registry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryVersion {
    /// Semver version string.
    pub version: String,
    /// Git tag for this version.
    pub tag: String,
    /// Commit hash at this tag.
    pub commit: String,
    /// Minimum C++ standard required.
    pub min_cpp_standard: Option<String>,
    /// When this version was published.
    pub published_at: String,
    /// Whether this version has been yanked.
    pub yanked: bool,
}

/// Parameters for publishing a module to the registry.
pub struct PublishModuleParams {
    pub name: String,
    pub version: String,
    pub tag: String,
    pub commit: String,
    pub description: Option<String>,
    pub license: Option<String>,
    pub repository: String,
}

/// The full registry index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegistryIndex {
    /// Registry format version.
    pub version: u32,
    /// Registry name.
    pub name: String,
    /// Registry description.
    pub description: String,
    /// Module entries.
    pub modules: BTreeMap<String, RegistryEntry>,
    /// Last update timestamp.
    pub updated_at: String,
}

impl RegistryIndex {
    /// Create an empty registry index.
    pub fn new(name: &str, description: &str) -> Self {
        RegistryIndex {
            version: 1,
            name: name.to_string(),
            description: description.to_string(),
            modules: BTreeMap::new(),
            updated_at: String::new(),
        }
    }

    /// Add or update a module entry.
    pub fn upsert_module(&mut self, entry: RegistryEntry) {
        self.modules.insert(entry.name.clone(), entry);
    }

    /// Remove a module entry.
    pub fn remove_module(&mut self, name: &str) -> bool {
        self.modules.remove(name).is_some()
    }

    /// Search modules by keyword (case-insensitive substring match).
    pub fn search(&self, query: &str) -> Vec<&RegistryEntry> {
        let query_lower = query.to_lowercase();
        self.modules
            .values()
            .filter(|entry| {
                entry.name.to_lowercase().contains(&query_lower)
                    || entry
                        .description
                        .as_ref()
                        .is_some_and(|d| d.to_lowercase().contains(&query_lower))
                    || entry
                        .keywords
                        .iter()
                        .any(|k| k.to_lowercase().contains(&query_lower))
                    || entry
                        .category
                        .as_ref()
                        .is_some_and(|c| c.to_lowercase().contains(&query_lower))
            })
            .filter(|entry| entry.deprecated.is_none())
            .collect()
    }

    /// Get the latest non-yanked version of a module.
    pub fn latest_version(&self, module_name: &str) -> Option<&RegistryVersion> {
        self.modules
            .get(module_name)
            .and_then(|entry| entry.versions.iter().rev().find(|v| !v.yanked))
    }

    /// Load a registry index from a JSON file.
    pub fn load(path: &Path) -> Result<Self, CmodError> {
        let content = std::fs::read_to_string(path)?;
        serde_json::from_str(&content)
            .map_err(|e| CmodError::Other(format!("failed to parse registry index: {}", e)))
    }

    /// Save the registry index to a JSON file.
    pub fn save(&self, path: &Path) -> Result<(), CmodError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|e| CmodError::Other(format!("failed to serialize registry: {}", e)))?;
        std::fs::write(path, json)?;
        Ok(())
    }
}

/// Registry client for fetching and updating the registry index.
pub struct RegistryClient {
    /// URL of the registry Git repository.
    registry_url: String,
    /// Local cache path for the registry.
    cache_dir: PathBuf,
}

impl RegistryClient {
    /// Create a new registry client.
    pub fn new(registry_url: &str, cache_dir: PathBuf) -> Self {
        RegistryClient {
            registry_url: registry_url.to_string(),
            cache_dir,
        }
    }

    /// Get the default registry URL.
    pub fn default_url() -> &'static str {
        "https://github.com/cmod-registry/index"
    }

    /// Fetch or update the local registry cache.
    pub fn update(&self) -> Result<RegistryIndex, CmodError> {
        let index_dir = self.cache_dir.join("registry");
        std::fs::create_dir_all(&index_dir)?;

        let repo_path = index_dir.join("index");
        if repo_path.exists() {
            // Pull latest
            self.pull_registry(&repo_path)?;
        } else {
            // Clone registry
            self.clone_registry(&repo_path)?;
        }

        let index_path = repo_path.join("index.json");
        if index_path.exists() {
            RegistryIndex::load(&index_path)
        } else {
            Ok(RegistryIndex::new("cmod", "C++ Module Registry"))
        }
    }

    /// Get the cached index without fetching.
    pub fn cached_index(&self) -> Result<Option<RegistryIndex>, CmodError> {
        let index_path = self
            .cache_dir
            .join("registry")
            .join("index")
            .join("index.json");
        if index_path.exists() {
            Ok(Some(RegistryIndex::load(&index_path)?))
        } else {
            Ok(None)
        }
    }

    /// Submit a module to the registry after publishing.
    ///
    /// Creates or updates the registry entry with the module's metadata and version.
    pub fn publish_module(&self, params: &PublishModuleParams) -> Result<(), CmodError> {
        let mut index = match self.cached_index()? {
            Some(idx) => idx,
            None => {
                self.update()?;
                self.cached_index()?
                    .unwrap_or_else(|| RegistryIndex::new("cmod", "C++ Module Registry"))
            }
        };

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
            .to_string();

        let new_version = RegistryVersion {
            version: params.version.clone(),
            tag: params.tag.clone(),
            commit: params.commit.clone(),
            min_cpp_standard: None,
            published_at: now.clone(),
            yanked: false,
        };

        if let Some(entry) = index.modules.get_mut(&params.name) {
            entry.versions.push(new_version);
            entry.updated_at = now;
            if let Some(ref desc) = params.description {
                entry.description = Some(desc.clone());
            }
            if let Some(ref lic) = params.license {
                entry.license = Some(lic.clone());
            }
        } else {
            let entry = RegistryEntry {
                name: params.name.clone(),
                description: params.description.clone(),
                repository: params.repository.clone(),
                versions: vec![new_version],
                keywords: Vec::new(),
                category: None,
                license: params.license.clone(),
                authors: Vec::new(),
                updated_at: now,
                verified: false,
                deprecated: None,
            };
            index.upsert_module(entry);
        }

        let index_path = self
            .cache_dir
            .join("registry")
            .join("index")
            .join("index.json");
        index.save(&index_path)?;

        Ok(())
    }

    fn clone_registry(&self, dest: &Path) -> Result<(), CmodError> {
        git2::Repository::clone(&self.registry_url, dest).map_err(|e| CmodError::GitError {
            reason: format!("failed to clone registry: {}", e),
        })?;
        Ok(())
    }

    fn pull_registry(&self, repo_path: &Path) -> Result<(), CmodError> {
        let repo = git2::Repository::open(repo_path).map_err(|e| CmodError::GitError {
            reason: format!("failed to open registry repo: {}", e),
        })?;

        let mut remote = repo
            .find_remote("origin")
            .map_err(|e| CmodError::GitError {
                reason: format!("registry has no origin remote: {}", e),
            })?;

        remote
            .fetch(&["refs/heads/main:refs/heads/main"], None, None)
            .map_err(|e| CmodError::GitError {
                reason: format!("failed to fetch registry updates: {}", e),
            })?;

        Ok(())
    }
}

/// Governance policy for module publishing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GovernancePolicy {
    /// Whether modules must have a license to be published.
    pub require_license: bool,
    /// Whether modules must have a description.
    pub require_description: bool,
    /// Minimum version format (must be valid semver).
    pub require_semver: bool,
    /// Whether modules must have signed commits.
    pub require_signed_commits: bool,
    /// Naming conventions that must be followed.
    pub naming_rules: NamingRules,
    /// Banned module name patterns.
    pub banned_names: Vec<String>,
}

impl Default for GovernancePolicy {
    fn default() -> Self {
        GovernancePolicy {
            require_license: true,
            require_description: true,
            require_semver: true,
            require_signed_commits: false,
            naming_rules: NamingRules::default(),
            banned_names: vec![
                "std".to_string(),
                "std.*".to_string(),
                "stdx".to_string(),
                "stdx.*".to_string(),
            ],
        }
    }
}

/// Module naming rules.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NamingRules {
    /// Minimum name length.
    pub min_length: usize,
    /// Maximum name length.
    pub max_length: usize,
    /// Allowed characters regex pattern.
    pub allowed_chars: String,
    /// Whether reverse-domain format is required.
    pub require_reverse_domain: bool,
}

impl Default for NamingRules {
    fn default() -> Self {
        NamingRules {
            min_length: 2,
            max_length: 128,
            allowed_chars: r"[a-zA-Z0-9._-]".to_string(),
            require_reverse_domain: true,
        }
    }
}

/// Validate a module against governance policy before publishing.
pub fn validate_for_publishing(
    name: &str,
    version: &str,
    description: Option<&str>,
    license: Option<&str>,
    policy: &GovernancePolicy,
) -> Vec<String> {
    let mut violations = Vec::new();

    // Check naming rules
    if name.len() < policy.naming_rules.min_length {
        violations.push(format!(
            "module name '{}' is too short (min {} chars)",
            name, policy.naming_rules.min_length
        ));
    }
    if name.len() > policy.naming_rules.max_length {
        violations.push(format!(
            "module name '{}' is too long (max {} chars)",
            name, policy.naming_rules.max_length
        ));
    }

    // Check banned names
    for banned in &policy.banned_names {
        if banned.ends_with('*') {
            let prefix = &banned[..banned.len() - 1];
            if name.starts_with(prefix) {
                violations.push(format!(
                    "module name '{}' matches banned pattern '{}'",
                    name, banned
                ));
            }
        } else if name == banned {
            violations.push(format!("module name '{}' is banned", name));
        }
    }

    // Check semver
    if policy.require_semver && semver::Version::parse(version).is_err() {
        violations.push(format!("version '{}' is not valid semver", version));
    }

    // Check description
    if policy.require_description && description.map_or(true, |d| d.trim().is_empty()) {
        violations.push("module must have a description".to_string());
    }

    // Check license
    if policy.require_license && license.map_or(true, |l| l.trim().is_empty()) {
        violations.push("module must have a license".to_string());
    }

    violations
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_registry_index_new() {
        let index = RegistryIndex::new("test", "Test registry");
        assert_eq!(index.version, 1);
        assert!(index.modules.is_empty());
    }

    #[test]
    fn test_registry_index_upsert() {
        let mut index = RegistryIndex::new("test", "");
        let entry = RegistryEntry {
            name: "github.fmtlib.fmt".into(),
            description: Some("Format library".into()),
            repository: "https://github.com/fmtlib/fmt".into(),
            versions: vec![RegistryVersion {
                version: "10.2.0".into(),
                tag: "v10.2.0".into(),
                commit: "abc123".into(),
                min_cpp_standard: Some("20".into()),
                published_at: "2024-01-01".into(),
                yanked: false,
            }],
            keywords: vec!["formatting".into()],
            category: Some("text".into()),
            license: Some("MIT".into()),
            authors: vec!["Victor Zverovich".into()],
            updated_at: "2024-01-01".into(),
            verified: true,
            deprecated: None,
        };
        index.upsert_module(entry);
        assert_eq!(index.modules.len(), 1);
    }

    #[test]
    fn test_registry_search() {
        let mut index = RegistryIndex::new("test", "");
        index.upsert_module(RegistryEntry {
            name: "github.fmtlib.fmt".into(),
            description: Some("A modern formatting library".into()),
            repository: "https://github.com/fmtlib/fmt".into(),
            versions: vec![],
            keywords: vec!["format".into(), "string".into()],
            category: Some("text".into()),
            license: Some("MIT".into()),
            authors: vec![],
            updated_at: "".into(),
            verified: false,
            deprecated: None,
        });
        index.upsert_module(RegistryEntry {
            name: "github.gabime.spdlog".into(),
            description: Some("Fast C++ logging library".into()),
            repository: "https://github.com/gabime/spdlog".into(),
            versions: vec![],
            keywords: vec!["logging".into()],
            category: Some("diagnostics".into()),
            license: Some("MIT".into()),
            authors: vec![],
            updated_at: "".into(),
            verified: false,
            deprecated: None,
        });

        let results = index.search("fmt");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "github.fmtlib.fmt");

        let results = index.search("logging");
        assert_eq!(results.len(), 1);

        let results = index.search("MIT");
        assert_eq!(results.len(), 0); // license is not searched

        let results = index.search("nonexistent");
        assert!(results.is_empty());
    }

    #[test]
    fn test_registry_latest_version() {
        let mut index = RegistryIndex::new("test", "");
        index.upsert_module(RegistryEntry {
            name: "mod1".into(),
            description: None,
            repository: "url".into(),
            versions: vec![
                RegistryVersion {
                    version: "1.0.0".into(),
                    tag: "v1.0.0".into(),
                    commit: "aaa".into(),
                    min_cpp_standard: None,
                    published_at: "".into(),
                    yanked: false,
                },
                RegistryVersion {
                    version: "2.0.0".into(),
                    tag: "v2.0.0".into(),
                    commit: "bbb".into(),
                    min_cpp_standard: None,
                    published_at: "".into(),
                    yanked: true, // yanked!
                },
                RegistryVersion {
                    version: "1.1.0".into(),
                    tag: "v1.1.0".into(),
                    commit: "ccc".into(),
                    min_cpp_standard: None,
                    published_at: "".into(),
                    yanked: false,
                },
            ],
            keywords: vec![],
            category: None,
            license: None,
            authors: vec![],
            updated_at: "".into(),
            verified: false,
            deprecated: None,
        });

        let latest = index.latest_version("mod1").unwrap();
        assert_eq!(latest.version, "1.1.0"); // 2.0.0 is yanked
    }

    #[test]
    fn test_validate_for_publishing_valid() {
        let policy = GovernancePolicy::default();
        let violations = validate_for_publishing(
            "github.user.mylib",
            "1.0.0",
            Some("A great library"),
            Some("MIT"),
            &policy,
        );
        assert!(violations.is_empty());
    }

    #[test]
    fn test_validate_for_publishing_banned_name() {
        let policy = GovernancePolicy::default();
        let violations =
            validate_for_publishing("std.io", "1.0.0", Some("desc"), Some("MIT"), &policy);
        assert!(!violations.is_empty());
        assert!(violations.iter().any(|v| v.contains("banned")));
    }

    #[test]
    fn test_validate_for_publishing_missing_license() {
        let policy = GovernancePolicy::default();
        let violations =
            validate_for_publishing("github.user.mylib", "1.0.0", Some("desc"), None, &policy);
        assert!(violations.iter().any(|v| v.contains("license")));
    }

    #[test]
    fn test_validate_for_publishing_bad_semver() {
        let policy = GovernancePolicy::default();
        let violations = validate_for_publishing(
            "github.user.mylib",
            "not-semver",
            Some("desc"),
            Some("MIT"),
            &policy,
        );
        assert!(violations.iter().any(|v| v.contains("semver")));
    }

    #[test]
    fn test_governance_policy_default() {
        let policy = GovernancePolicy::default();
        assert!(policy.require_license);
        assert!(policy.require_description);
        assert!(policy.require_semver);
        assert!(!policy.require_signed_commits);
    }

    #[test]
    fn test_registry_remove_module() {
        let mut index = RegistryIndex::new("test", "");
        index.upsert_module(RegistryEntry {
            name: "mod1".into(),
            description: None,
            repository: "url".into(),
            versions: vec![],
            keywords: vec![],
            category: None,
            license: None,
            authors: vec![],
            updated_at: "".into(),
            verified: false,
            deprecated: None,
        });
        assert!(index.remove_module("mod1"));
        assert!(!index.remove_module("mod1"));
    }

    #[test]
    fn test_registry_index_serde() {
        let index = RegistryIndex::new("test", "Test");
        let json = serde_json::to_string(&index).unwrap();
        let parsed: RegistryIndex = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, "test");
    }

    #[test]
    fn test_search_excludes_deprecated() {
        let mut index = RegistryIndex::new("test", "");
        index.upsert_module(RegistryEntry {
            name: "old_lib".into(),
            description: Some("Old library".into()),
            repository: "url".into(),
            versions: vec![],
            keywords: vec![],
            category: None,
            license: None,
            authors: vec![],
            updated_at: "".into(),
            verified: false,
            deprecated: Some("Use new_lib instead".into()),
        });

        let results = index.search("old_lib");
        assert!(results.is_empty());
    }
}
