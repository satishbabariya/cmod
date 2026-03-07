use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use cmod_core::error::CmodError;

/// Trust database for TOFU (trust-on-first-use) model.
///
/// Stores trusted module origins and their associated keys/fingerprints.
/// Persisted at `~/.config/cmod/trust.toml`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrustDb {
    /// Trusted module entries keyed by module name.
    #[serde(default)]
    pub modules: BTreeMap<String, TrustedModule>,
    /// Revoked signing key identifiers (fingerprints, emails, key IDs).
    #[serde(default)]
    pub revoked_keys: Vec<String>,
}

/// A trusted module entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrustedModule {
    /// Git URL origin.
    pub origin: String,
    /// First-seen commit hash.
    pub first_seen_commit: String,
    /// Key fingerprint (if signed).
    pub key_fingerprint: Option<String>,
    /// When this trust entry was created.
    pub trusted_at: String,
    /// Whether trust has been explicitly revoked.
    #[serde(default)]
    pub revoked: bool,
}

impl TrustDb {
    /// Load the trust database from the default config location.
    pub fn load_default() -> Result<Self, CmodError> {
        let path = Self::default_path();
        if path.exists() {
            Self::load(&path)
        } else {
            Ok(TrustDb::default())
        }
    }

    /// Load the trust database from a specific file.
    pub fn load(path: &Path) -> Result<Self, CmodError> {
        let content = std::fs::read_to_string(path)?;
        toml::from_str(&content)
            .map_err(|e| CmodError::Other(format!("failed to parse trust database: {}", e)))
    }

    /// Save the trust database.
    pub fn save(&self, path: &Path) -> Result<(), CmodError> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)
            .map_err(|e| CmodError::Other(format!("failed to serialize trust database: {}", e)))?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Save to the default location.
    pub fn save_default(&self) -> Result<(), CmodError> {
        self.save(&Self::default_path())
    }

    /// Default path for the trust database.
    pub fn default_path() -> PathBuf {
        dirs::config_dir()
            .unwrap_or_else(|| PathBuf::from(".config"))
            .join("cmod")
            .join("trust.toml")
    }

    /// Record trust for a module on first use.
    ///
    /// Returns `true` if this is a new entry, `false` if already trusted.
    pub fn trust_on_first_use(&mut self, module_name: &str, origin: &str, commit: &str) -> bool {
        if self.modules.contains_key(module_name) {
            return false;
        }

        self.modules.insert(
            module_name.to_string(),
            TrustedModule {
                origin: origin.to_string(),
                first_seen_commit: commit.to_string(),
                key_fingerprint: None,
                trusted_at: String::new(),
                revoked: false,
            },
        );

        true
    }

    /// Check whether a module is trusted.
    pub fn is_trusted(&self, module_name: &str) -> bool {
        self.modules.get(module_name).is_some_and(|m| !m.revoked)
    }

    /// Check whether a module's origin matches what was trusted.
    pub fn origin_matches(&self, module_name: &str, origin: &str) -> Option<bool> {
        self.modules.get(module_name).map(|m| m.origin == origin)
    }

    /// Revoke trust for a module.
    pub fn revoke(&mut self, module_name: &str) -> bool {
        if let Some(entry) = self.modules.get_mut(module_name) {
            entry.revoked = true;
            true
        } else {
            false
        }
    }

    /// Remove a trust entry entirely.
    pub fn remove(&mut self, module_name: &str) -> bool {
        self.modules.remove(module_name).is_some()
    }

    /// Check whether a signing key has been revoked.
    pub fn is_key_revoked(&self, key_id: &str) -> bool {
        self.revoked_keys.iter().any(|k| k == key_id)
    }

    /// Revoke a signing key by identifier.
    ///
    /// Returns `true` if the key was newly added to the revocation list.
    pub fn revoke_key(&mut self, key_id: &str) -> bool {
        if self.is_key_revoked(key_id) {
            return false;
        }
        self.revoked_keys.push(key_id.to_string());
        true
    }

    /// List all trusted (non-revoked) modules.
    pub fn trusted_modules(&self) -> Vec<&str> {
        self.modules
            .iter()
            .filter(|(_, m)| !m.revoked)
            .map(|(name, _)| name.as_str())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_trust_on_first_use() {
        let mut db = TrustDb::default();
        assert!(db.trust_on_first_use(
            "github.fmtlib.fmt",
            "https://github.com/fmtlib/fmt",
            "abc123"
        ));
        // Second time should return false
        assert!(!db.trust_on_first_use(
            "github.fmtlib.fmt",
            "https://github.com/fmtlib/fmt",
            "abc123"
        ));
    }

    #[test]
    fn test_is_trusted() {
        let mut db = TrustDb::default();
        assert!(!db.is_trusted("nonexistent"));

        db.trust_on_first_use("mod1", "https://example.com", "abc");
        assert!(db.is_trusted("mod1"));
    }

    #[test]
    fn test_revoke() {
        let mut db = TrustDb::default();
        db.trust_on_first_use("mod1", "https://example.com", "abc");
        assert!(db.is_trusted("mod1"));

        db.revoke("mod1");
        assert!(!db.is_trusted("mod1"));
    }

    #[test]
    fn test_remove() {
        let mut db = TrustDb::default();
        db.trust_on_first_use("mod1", "https://example.com", "abc");
        assert!(db.remove("mod1"));
        assert!(!db.remove("mod1")); // Already removed
    }

    #[test]
    fn test_origin_matches() {
        let mut db = TrustDb::default();
        db.trust_on_first_use("mod1", "https://example.com/repo", "abc");

        assert_eq!(
            db.origin_matches("mod1", "https://example.com/repo"),
            Some(true)
        );
        assert_eq!(
            db.origin_matches("mod1", "https://other.com/repo"),
            Some(false)
        );
        assert_eq!(db.origin_matches("nonexistent", "url"), None);
    }

    #[test]
    fn test_trusted_modules() {
        let mut db = TrustDb::default();
        db.trust_on_first_use("a", "url_a", "c1");
        db.trust_on_first_use("b", "url_b", "c2");
        db.trust_on_first_use("c", "url_c", "c3");
        db.revoke("b");

        let trusted = db.trusted_modules();
        assert_eq!(trusted.len(), 2);
        assert!(trusted.contains(&"a"));
        assert!(trusted.contains(&"c"));
    }

    #[test]
    fn test_save_and_load() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("trust.toml");

        let mut db = TrustDb::default();
        db.trust_on_first_use("fmt", "https://github.com/fmtlib/fmt", "abc123");
        db.save(&path).unwrap();

        let loaded = TrustDb::load(&path).unwrap();
        assert!(loaded.is_trusted("fmt"));
        assert_eq!(
            loaded.modules["fmt"].origin,
            "https://github.com/fmtlib/fmt"
        );
    }

    #[test]
    fn test_load_nonexistent_returns_default() {
        let db = TrustDb::load_default();
        // Should not error if file doesn't exist
        assert!(db.is_ok());
    }

    #[test]
    fn test_default_path() {
        let path = TrustDb::default_path();
        assert!(path.to_string_lossy().contains("cmod"));
        assert!(path.to_string_lossy().contains("trust.toml"));
    }

    #[test]
    fn test_revoke_nonexistent() {
        let mut db = TrustDb::default();
        assert!(!db.revoke("nonexistent"));
    }

    #[test]
    fn test_key_revocation() {
        let mut db = TrustDb::default();
        assert!(!db.is_key_revoked("ABCD1234"));

        assert!(db.revoke_key("ABCD1234"));
        assert!(db.is_key_revoked("ABCD1234"));

        // Duplicate revoke returns false
        assert!(!db.revoke_key("ABCD1234"));
    }

    #[test]
    fn test_revoked_keys_persist() {
        let tmp = TempDir::new().unwrap();
        let path = tmp.path().join("trust.toml");

        let mut db = TrustDb::default();
        db.revoke_key("key1");
        db.revoke_key("key2");
        db.save(&path).unwrap();

        let loaded = TrustDb::load(&path).unwrap();
        assert!(loaded.is_key_revoked("key1"));
        assert!(loaded.is_key_revoked("key2"));
        assert!(!loaded.is_key_revoked("key3"));
    }
}
