use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

use crate::error::CmodError;

/// The cmod.lock lockfile format.
///
/// Locks exact dependency versions, commit hashes, and toolchain info
/// for reproducible builds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lockfile {
    /// Lockfile format version.
    pub version: u32,

    /// Locked packages.
    #[serde(default, rename = "package")]
    pub packages: Vec<LockedPackage>,
}

/// A single locked dependency in the lockfile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedPackage {
    /// Dependency key (e.g., `github.com/fmtlib/fmt`).
    pub name: String,

    /// Exact resolved version string.
    pub version: String,

    /// Source type (`git` or `path`).
    #[serde(default)]
    pub source: Option<String>,

    /// Git repository URL.
    #[serde(default)]
    pub repo: Option<String>,

    /// Exact commit hash.
    #[serde(default)]
    pub commit: Option<String>,

    /// Content hash of the resolved sources.
    #[serde(default)]
    pub hash: Option<String>,

    /// Locked toolchain info for this package.
    #[serde(default)]
    pub toolchain: Option<LockedToolchain>,

    /// Locked target platforms.
    #[serde(default)]
    pub targets: BTreeMap<String, toml::Value>,

    /// Dependencies of this package.
    #[serde(default)]
    pub deps: Vec<String>,

    /// Activated features for this package.
    #[serde(default)]
    pub features: Vec<String>,
}

/// Toolchain info locked in the lockfile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LockedToolchain {
    #[serde(default)]
    pub compiler: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub stdlib: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
}

impl Lockfile {
    /// Create a new empty lockfile.
    pub fn new() -> Self {
        Lockfile {
            version: 1,
            packages: Vec::new(),
        }
    }

    /// Load a lockfile from disk.
    pub fn load(path: &Path) -> Result<Self, CmodError> {
        let content = std::fs::read_to_string(path).map_err(|_| CmodError::LockfileNotFound)?;
        Self::from_str(&content)
    }

    /// Parse a lockfile from a TOML string.
    pub fn from_str(content: &str) -> Result<Self, CmodError> {
        toml::from_str(content).map_err(|e| CmodError::LockfileIntegrity {
            reason: e.to_string(),
        })
    }

    /// Serialize lockfile to TOML.
    pub fn to_toml_string(&self) -> Result<String, CmodError> {
        toml::to_string_pretty(self).map_err(|e| CmodError::LockfileIntegrity {
            reason: e.to_string(),
        })
    }

    /// Write the lockfile to disk.
    pub fn save(&self, path: &Path) -> Result<(), CmodError> {
        let content = self.to_toml_string()?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Find a locked package by name.
    pub fn find_package(&self, name: &str) -> Option<&LockedPackage> {
        self.packages.iter().find(|p| p.name == name)
    }

    /// Add or update a locked package.
    pub fn upsert_package(&mut self, pkg: LockedPackage) {
        if let Some(existing) = self.packages.iter_mut().find(|p| p.name == pkg.name) {
            *existing = pkg;
        } else {
            self.packages.push(pkg);
        }
        // Keep packages sorted for deterministic output.
        self.packages.sort_by(|a, b| a.name.cmp(&b.name));
    }

    /// Remove a package from the lockfile.
    pub fn remove_package(&mut self, name: &str) {
        self.packages.retain(|p| p.name != name);
    }

    /// Check whether the lockfile contains any packages.
    pub fn is_empty(&self) -> bool {
        self.packages.is_empty()
    }
}

impl Default for Lockfile {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_lockfile() {
        let lock = Lockfile::new();
        assert_eq!(lock.version, 1);
        assert!(lock.is_empty());
    }

    #[test]
    fn test_lockfile_roundtrip() {
        let mut lock = Lockfile::new();
        lock.upsert_package(LockedPackage {
            name: "github.com/fmtlib/fmt".to_string(),
            version: "10.2.1".to_string(),
            source: Some("git".to_string()),
            repo: Some("https://github.com/fmtlib/fmt".to_string()),
            commit: Some("a1b2c3d4".to_string()),
            hash: Some("sha256:abcdef1234567890".to_string()),
            toolchain: Some(LockedToolchain {
                compiler: Some("clang".to_string()),
                version: Some("18.1.0".to_string()),
                stdlib: Some("libc++".to_string()),
                target: Some("x86_64-linux-gnu".to_string()),
            }),
            targets: BTreeMap::new(),
            deps: vec![],
            features: vec![],
        });

        let toml_str = lock.to_toml_string().unwrap();
        let parsed = Lockfile::from_str(&toml_str).unwrap();
        assert_eq!(parsed.packages.len(), 1);
        assert_eq!(parsed.packages[0].name, "github.com/fmtlib/fmt");
        assert_eq!(parsed.packages[0].version, "10.2.1");
        assert_eq!(
            parsed.packages[0].commit.as_deref(),
            Some("a1b2c3d4")
        );
    }

    #[test]
    fn test_lockfile_upsert_and_remove() {
        let mut lock = Lockfile::new();

        lock.upsert_package(LockedPackage {
            name: "pkg_a".to_string(),
            version: "1.0.0".to_string(),
            source: None,
            repo: None,
            commit: None,
            hash: None,
            toolchain: None,
            targets: BTreeMap::new(),
            deps: vec![],
            features: vec![],
        });
        lock.upsert_package(LockedPackage {
            name: "pkg_b".to_string(),
            version: "2.0.0".to_string(),
            source: None,
            repo: None,
            commit: None,
            hash: None,
            toolchain: None,
            targets: BTreeMap::new(),
            deps: vec![],
            features: vec![],
        });
        assert_eq!(lock.packages.len(), 2);

        // Upsert (update) pkg_a
        lock.upsert_package(LockedPackage {
            name: "pkg_a".to_string(),
            version: "1.1.0".to_string(),
            source: None,
            repo: None,
            commit: None,
            hash: None,
            toolchain: None,
            targets: BTreeMap::new(),
            deps: vec![],
            features: vec![],
        });
        assert_eq!(lock.packages.len(), 2);
        assert_eq!(lock.find_package("pkg_a").unwrap().version, "1.1.0");

        lock.remove_package("pkg_b");
        assert_eq!(lock.packages.len(), 1);
        assert!(lock.find_package("pkg_b").is_none());
    }
}
