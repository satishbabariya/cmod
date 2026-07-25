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

impl RegistryEntry {
    /// Build the entry a publication would create — shared by
    /// `RegistryClient::publish_module` and the `cmod publish` PR-fragment
    /// fallback so the two can never drift (#79).
    pub fn from_publish_params(params: &PublishModuleParams, now: &str) -> Self {
        RegistryEntry {
            name: params.name.clone(),
            description: params.description.clone(),
            repository: params.repository.clone(),
            versions: vec![RegistryVersion {
                version: params.version.clone(),
                tag: params.tag.clone(),
                commit: params.commit.clone(),
                min_cpp_standard: None,
                published_at: now.to_string(),
                yanked: false,
            }],
            keywords: Vec::new(),
            category: None,
            license: params.license.clone(),
            authors: Vec::new(),
            updated_at: now.to_string(),
            verified: false,
            deprecated: None,
        }
    }
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
            index.upsert_module(RegistryEntry::from_publish_params(params, &now));
        }

        let repo_path = self.cache_dir.join("registry").join("index");
        let index_path = repo_path.join("index.json");
        index.save(&index_path)?;

        // Commit and push the updated index — a publication that only edits
        // the local cache clone is silently lost on the next pull (#78).
        let repo = git2::Repository::open(&repo_path).map_err(|e| CmodError::GitError {
            reason: format!("failed to open registry clone: {}", e),
        })?;
        let mut git_index = repo.index().map_err(|e| CmodError::GitError {
            reason: format!("registry index: {}", e),
        })?;
        git_index
            .add_path(Path::new("index.json"))
            .and_then(|_| git_index.write())
            .map_err(|e| CmodError::GitError {
                reason: format!("failed to stage index.json: {}", e),
            })?;
        let tree_id = git_index.write_tree().map_err(|e| CmodError::GitError {
            reason: format!("failed to write tree: {}", e),
        })?;
        let tree = repo.find_tree(tree_id).map_err(|e| CmodError::GitError {
            reason: format!("tree lookup: {}", e),
        })?;
        let sig = repo
            .signature()
            .or_else(|_| git2::Signature::now("cmod", "cmod@localhost"))
            .map_err(|e| CmodError::GitError {
                reason: format!("signature: {}", e),
            })?;
        let head =
            repo.head()
                .and_then(|h| h.peel_to_commit())
                .map_err(|e| CmodError::GitError {
                    reason: format!("registry HEAD: {}", e),
                })?;
        let branch = repo
            .head()
            .ok()
            .and_then(|h| h.shorthand().map(str::to_string))
            .unwrap_or_else(|| "main".to_string());
        repo.commit(
            Some("HEAD"),
            &sig,
            &sig,
            &format!("publish {} v{}", params.name, params.version),
            &tree,
            &[&head],
        )
        .map_err(|e| CmodError::GitError {
            reason: format!("failed to commit registry update: {}", e),
        })?;

        let mut remote = repo
            .find_remote("origin")
            .map_err(|e| CmodError::GitError {
                reason: format!("registry remote: {}", e),
            })?;
        let config = repo.config().map_err(|e| CmodError::GitError {
            reason: format!("git config: {}", e),
        })?;
        let mut callbacks = git2::RemoteCallbacks::new();
        callbacks.credentials(move |url, username, _| {
            git2::Cred::credential_helper(&config, url, username).or_else(|_| git2::Cred::default())
        });
        let mut push_opts = git2::PushOptions::new();
        push_opts.remote_callbacks(callbacks);
        remote
            .push(
                &[&format!("refs/heads/{b}:refs/heads/{b}", b = branch)],
                Some(&mut push_opts),
            )
            .map_err(|e| CmodError::GitError {
                reason: format!(
                    "failed to push registry update (write access to the registry \
                     repository is required): {}",
                    e
                ),
            })?;

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

        // Determine the default branch dynamically instead of assuming "main".
        let default_branch = repo
            .head()
            .ok()
            .and_then(|h| h.shorthand().map(String::from))
            .unwrap_or_else(|| "main".to_string());

        let refspec = format!("refs/heads/{0}:refs/heads/{0}", default_branch);

        remote
            .fetch(&[&refspec], None, None)
            .map_err(|e| CmodError::GitError {
                reason: format!("failed to fetch registry updates: {}", e),
            })?;

        // Update the working tree to reflect fetched commits.
        repo.checkout_head(Some(git2::build::CheckoutBuilder::new().force()))
            .map_err(|e| CmodError::GitError {
                reason: format!("failed to checkout after fetch: {}", e),
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

/// Validate every entry in a registry index against governance policy.
///
/// Returns human-readable violations (empty = valid). This is the same rule
/// set `validate_for_publishing` applies to individual submissions, plus
/// structural checks — the registry's validation Action runs exactly this
/// via `cmod registry validate` (#79).
pub fn validate_index(index: &RegistryIndex, policy: &GovernancePolicy) -> Vec<String> {
    let mut violations = Vec::new();
    for (key, entry) in &index.modules {
        if key != &entry.name {
            violations.push(format!(
                "{}: map key does not match entry name '{}'",
                key, entry.name
            ));
        }
        if entry.repository.is_empty() {
            violations.push(format!("{}: repository URL is empty", entry.name));
        }
        if entry.versions.is_empty() {
            violations.push(format!("{}: no versions listed", entry.name));
        }
        for v in &entry.versions {
            for violation in validate_for_publishing(
                &entry.name,
                &v.version,
                entry.description.as_deref(),
                entry.license.as_deref(),
                policy,
            ) {
                violations.push(format!("{} v{}: {}", entry.name, v.version, violation));
            }
        }
    }
    violations
}

/// Validate an updated index against its base revision.
///
/// Policy (POLICY.md): listings and version rows are never deleted — a yank
/// flips the `yanked` flag so existing lockfiles keep resolving.
pub fn validate_index_against_base(new: &RegistryIndex, base: &RegistryIndex) -> Vec<String> {
    let mut violations = Vec::new();
    for (name, base_entry) in &base.modules {
        match new.modules.get(name) {
            None => violations.push(format!(
                "{}: module removed — yank versions instead of deleting listings",
                name
            )),
            Some(new_entry) => {
                for bv in &base_entry.versions {
                    if !new_entry.versions.iter().any(|nv| nv.version == bv.version) {
                        violations.push(format!(
                            "{} v{}: version row removed — set \"yanked\": true instead",
                            name, bv.version
                        ));
                    }
                }
            }
        }
    }
    violations
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
    fn test_entry_from_params_matches_publish_shape() {
        let params = PublishModuleParams {
            name: "com.github.x.y".to_string(),
            version: "1.2.0".to_string(),
            tag: "v1.2.0".to_string(),
            commit: "a".repeat(40),
            description: Some("d".to_string()),
            license: Some("MIT".to_string()),
            repository: "https://github.com/x/y".to_string(),
        };
        let entry = RegistryEntry::from_publish_params(&params, "ts");
        assert_eq!(entry.name, "com.github.x.y");
        assert_eq!(entry.versions.len(), 1);
        assert_eq!(entry.versions[0].tag, "v1.2.0");
        assert_eq!(entry.updated_at, "ts");
        assert!(!entry.verified);
    }

    // --- index validation for registry phase 2 (#79) ---

    fn seeded_entry(name: &str) -> RegistryEntry {
        RegistryEntry {
            name: name.to_string(),
            description: Some("desc".to_string()),
            repository: "https://github.com/x/y".to_string(),
            versions: vec![RegistryVersion {
                version: "1.0.0".to_string(),
                tag: "v1.0.0".to_string(),
                commit: "c".repeat(40),
                min_cpp_standard: None,
                published_at: "now".to_string(),
                yanked: false,
            }],
            keywords: vec![],
            category: None,
            license: Some("MIT".to_string()),
            authors: vec![],
            updated_at: "now".to_string(),
            verified: false,
            deprecated: None,
        }
    }

    #[test]
    fn test_validate_index_clean() {
        let mut idx = RegistryIndex::new("t", "t");
        idx.upsert_module(seeded_entry("com.github.x.y"));
        assert!(validate_index(&idx, &GovernancePolicy::default()).is_empty());
    }

    #[test]
    fn test_validate_index_flags_violations() {
        let mut idx = RegistryIndex::new("t", "t");
        let mut banned = seeded_entry("std.core");
        banned.license = None;
        banned.versions[0].version = "not-semver".to_string();
        idx.upsert_module(banned);
        let violations = validate_index(&idx, &GovernancePolicy::default());
        let joined = violations.join("\n");
        assert!(joined.contains("std.core"), "banned name: {joined}");
        assert!(
            joined.to_lowercase().contains("license"),
            "license: {joined}"
        );
        assert!(
            joined.to_lowercase().contains("semver") || joined.contains("not-semver"),
            "semver: {joined}"
        );
    }

    #[test]
    fn test_validate_index_against_base_rejects_removals() {
        let mut base = RegistryIndex::new("t", "t");
        base.upsert_module(seeded_entry("com.github.x.a"));
        let mut b_entry = seeded_entry("com.github.x.b");
        b_entry.versions.push(RegistryVersion {
            version: "1.1.0".to_string(),
            tag: "v1.1.0".to_string(),
            commit: "d".repeat(40),
            min_cpp_standard: None,
            published_at: "now".to_string(),
            yanked: false,
        });
        base.upsert_module(b_entry.clone());

        // New index drops module a and removes a version row of b
        let mut new = RegistryIndex::new("t", "t");
        let mut b_short = b_entry.clone();
        b_short.versions.pop();
        new.upsert_module(b_short);

        let violations = validate_index_against_base(&new, &base);
        let joined = violations.join("\n");
        assert!(
            joined.contains("com.github.x.a"),
            "module removal: {joined}"
        );
        assert!(joined.contains("1.1.0"), "version removal: {joined}");

        // Yank-flag flips are allowed
        let mut yanked = base.clone();
        yanked.modules.get_mut("com.github.x.b").unwrap().versions[1].yanked = true;
        assert!(validate_index_against_base(&yanked, &base).is_empty());
    }

    /// Seed a local bare registry remote containing `index` and return its
    /// path. Publish tests must go through real git — a cache-only fixture
    /// is how the missing-push bug stayed hidden (#78).
    fn local_registry(tmp: &std::path::Path, index: &RegistryIndex) -> PathBuf {
        let seed = tmp.join("seed");
        std::fs::create_dir_all(&seed).unwrap();
        index.save(&seed.join("index.json")).unwrap();
        let repo = git2::Repository::init(&seed).unwrap();
        let mut gidx = repo.index().unwrap();
        gidx.add_path(Path::new("index.json")).unwrap();
        gidx.write().unwrap();
        let tree = repo.find_tree(gidx.write_tree().unwrap()).unwrap();
        let sig = git2::Signature::now("t", "t@t").unwrap();
        repo.commit(Some("HEAD"), &sig, &sig, "seed", &tree, &[])
            .unwrap();
        let bare = tmp.join("registry.git");
        git2::build::RepoBuilder::new()
            .bare(true)
            .clone(seed.to_str().unwrap(), &bare)
            .unwrap();
        bare
    }

    #[test]
    fn test_publish_module_pushes_to_remote() {
        // #78 round-trip: a publication must reach the registry remote —
        // a fresh consumer clone has to see it.
        let tmp = tempfile::tempdir().unwrap();
        let bare = local_registry(tmp.path(), &RegistryIndex::new("t", "t"));

        // Publisher client publishes.
        let publisher = RegistryClient::new(bare.to_str().unwrap(), tmp.path().join("cache-a"));
        publisher
            .publish_module(&PublishModuleParams {
                name: "com.github.example.demo".to_string(),
                version: "1.0.0".to_string(),
                tag: "v1.0.0".to_string(),
                commit: "0".repeat(40),
                description: Some("probe".to_string()),
                license: Some("MIT".to_string()),
                repository: "https://github.com/example/demo".to_string(),
            })
            .unwrap();

        // A fresh consumer (separate cache) must see the publication.
        let consumer = RegistryClient::new(bare.to_str().unwrap(), tmp.path().join("cache-b"));
        let index = consumer.update().unwrap();
        assert!(
            index.modules.contains_key("com.github.example.demo"),
            "publication never reached the remote"
        );
    }

    #[test]
    fn test_publish_module_creates_entry() {
        let tmp = tempfile::tempdir().unwrap();
        let bare = local_registry(tmp.path(), &RegistryIndex::new("test", "Test registry"));
        let client = RegistryClient::new(bare.to_str().unwrap(), tmp.path().join("cache"));

        let params = PublishModuleParams {
            name: "github.user.mylib".into(),
            version: "1.0.0".into(),
            tag: "v1.0.0".into(),
            commit: "abc123".into(),
            description: Some("My library".into()),
            license: Some("MIT".into()),
            repository: "https://github.com/user/mylib".into(),
        };

        client.publish_module(&params).unwrap();

        let index = client.cached_index().unwrap().unwrap();
        assert!(index.modules.contains_key("github.user.mylib"));
        let entry = &index.modules["github.user.mylib"];
        assert_eq!(entry.versions.len(), 1);
        assert_eq!(entry.versions[0].version, "1.0.0");
        assert_eq!(entry.description.as_deref(), Some("My library"));
    }

    #[test]
    fn test_publish_module_appends_version() {
        let tmp = tempfile::tempdir().unwrap();
        let mut index = RegistryIndex::new("test", "Test");
        index.upsert_module(RegistryEntry {
            name: "github.user.mylib".into(),
            description: Some("Old desc".into()),
            repository: "https://github.com/user/mylib".into(),
            versions: vec![RegistryVersion {
                version: "0.1.0".into(),
                tag: "v0.1.0".into(),
                commit: "old".into(),
                min_cpp_standard: None,
                published_at: "0".into(),
                yanked: false,
            }],
            keywords: vec![],
            category: None,
            license: None,
            authors: vec![],
            updated_at: "0".into(),
            verified: false,
            deprecated: None,
        });
        let bare = local_registry(tmp.path(), &index);
        let client = RegistryClient::new(bare.to_str().unwrap(), tmp.path().join("cache"));

        let params = PublishModuleParams {
            name: "github.user.mylib".into(),
            version: "1.0.0".into(),
            tag: "v1.0.0".into(),
            commit: "new123".into(),
            description: Some("Updated desc".into()),
            license: Some("MIT".into()),
            repository: "https://github.com/user/mylib".into(),
        };
        client.publish_module(&params).unwrap();

        let updated = client.cached_index().unwrap().unwrap();
        let entry = &updated.modules["github.user.mylib"];
        assert_eq!(entry.versions.len(), 2);
        assert_eq!(entry.versions[1].version, "1.0.0");
        assert_eq!(entry.description.as_deref(), Some("Updated desc"));
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
