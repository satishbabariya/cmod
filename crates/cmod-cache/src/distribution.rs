//! Precompiled module distribution via Git repositories.
//!
//! Implements RFC-0011: Git-based BMI distribution where precompiled
//! Binary Module Interfaces are published to Git repositories
//! alongside source code, organized by toolchain and target.
//!
//! Distribution layout in a Git repository:
//! ```text
//! bmi/
//! ├── clang-18.1.0-std20-libc++-x86_64-unknown-linux-gnu/
//! │   ├── module.pcm
//! │   ├── module.o
//! │   └── metadata.json
//! └── clang-18.1.0-std20-libc++-aarch64-unknown-linux-gnu/
//!     ├── module.pcm
//!     ├── module.o
//!     └── metadata.json
//! ```

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use cmod_core::error::CmodError;

use crate::bmi::{BmiMetadata, BmiPackage};
use crate::key::hash_file;

/// Index of all precompiled module variants available in a repository.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BmiIndex {
    /// Module name.
    pub module_name: String,
    /// Module version.
    pub version: String,
    /// Available precompiled variants.
    pub variants: Vec<BmiVariant>,
    /// Index format version.
    pub format_version: u32,
}

/// A single precompiled variant in the index.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BmiVariant {
    /// Compiler used.
    pub compiler: String,
    /// Compiler version.
    pub compiler_version: String,
    /// Target triple.
    pub target: String,
    /// C++ standard.
    pub cxx_standard: String,
    /// Standard library.
    pub stdlib: Option<String>,
    /// Directory name containing the BMI files.
    pub directory: String,
    /// SHA-256 hash of the BMI package.
    pub package_hash: String,
    /// Size in bytes of the BMI package.
    pub size_bytes: u64,
    /// When this variant was built.
    pub built_at: String,
}

/// Publish BMI artifacts to a distribution directory (for later Git push).
pub fn publish_bmi(
    bmi_package: &BmiPackage,
    distribution_dir: &Path,
) -> Result<PathBuf, CmodError> {
    let compat_key = bmi_package.metadata.compat_key();
    let variant_dir = distribution_dir.join("bmi").join(&compat_key);
    fs::create_dir_all(&variant_dir)?;

    // Write metadata
    let metadata_json = serde_json::to_string_pretty(&bmi_package.metadata)
        .map_err(|e| CmodError::Other(format!("failed to serialize BMI metadata: {}", e)))?;
    fs::write(variant_dir.join("metadata.json"), &metadata_json)?;

    // Write package manifest
    let package_json = serde_json::to_string_pretty(bmi_package)
        .map_err(|e| CmodError::Other(format!("failed to serialize BMI package: {}", e)))?;
    fs::write(variant_dir.join("bmi_package.json"), &package_json)?;

    Ok(variant_dir)
}

/// Generate or update the BMI index for a distribution directory.
pub fn update_bmi_index(
    distribution_dir: &Path,
    module_name: &str,
    version: &str,
) -> Result<BmiIndex, CmodError> {
    let bmi_dir = distribution_dir.join("bmi");
    if !bmi_dir.exists() {
        return Ok(BmiIndex {
            module_name: module_name.to_string(),
            version: version.to_string(),
            variants: Vec::new(),
            format_version: 1,
        });
    }

    let mut variants = Vec::new();

    for entry in fs::read_dir(&bmi_dir)? {
        let entry = entry?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let metadata_path = path.join("metadata.json");
        if !metadata_path.exists() {
            continue;
        }

        let content = fs::read_to_string(&metadata_path)?;
        let metadata: BmiMetadata = serde_json::from_str(&content)
            .map_err(|e| CmodError::Other(format!("failed to parse BMI metadata: {}", e)))?;

        let dir_name = entry.file_name().to_string_lossy().to_string();

        // Compute package hash
        let package_hash = if let Ok(pkg_path) = path.join("bmi_package.json").canonicalize() {
            hash_file(&pkg_path).unwrap_or_default()
        } else {
            String::new()
        };

        // Calculate total size
        let size_bytes: u64 = fs::read_dir(&path)?
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_file())
            .filter_map(|e| e.metadata().ok())
            .map(|m| m.len())
            .sum();

        variants.push(BmiVariant {
            compiler: metadata.compiler,
            compiler_version: metadata.compiler_version,
            target: metadata.target,
            cxx_standard: metadata.cxx_standard,
            stdlib: metadata.stdlib,
            directory: dir_name,
            package_hash,
            size_bytes,
            built_at: metadata.created_at,
        });
    }

    let index = BmiIndex {
        module_name: module_name.to_string(),
        version: version.to_string(),
        variants,
        format_version: 1,
    };

    // Write index file
    let index_json = serde_json::to_string_pretty(&index)
        .map_err(|e| CmodError::Other(format!("failed to serialize BMI index: {}", e)))?;
    fs::write(distribution_dir.join("bmi").join("index.json"), &index_json)?;

    Ok(index)
}

/// Find a compatible BMI variant from an index.
pub fn find_compatible_variant<'a>(
    index: &'a BmiIndex,
    compiler: &str,
    compiler_version: &str,
    target: &str,
    cxx_standard: &str,
) -> Option<&'a BmiVariant> {
    // Exact match first
    if let Some(variant) = index.variants.iter().find(|v| {
        v.compiler == compiler
            && v.compiler_version == compiler_version
            && v.target == target
            && v.cxx_standard == cxx_standard
    }) {
        return Some(variant);
    }

    // Fuzzy match: same compiler major version, same target and standard
    let major_version = compiler_version.split('.').next().unwrap_or("");
    index.variants.iter().find(|v| {
        v.compiler == compiler
            && v.compiler_version.starts_with(major_version)
            && v.target == target
            && v.cxx_standard == cxx_standard
    })
}

/// Compute a delta between two BMI packages (files that changed).
pub fn compute_bmi_delta(old_package: &BmiPackage, new_package: &BmiPackage) -> BmiDelta {
    let mut added = Vec::new();
    let mut modified = Vec::new();
    let mut removed = Vec::new();

    // Files in new but not old, or changed
    for (name, hash) in &new_package.files {
        match old_package.files.get(name) {
            Some(old_hash) if old_hash != hash => {
                modified.push(name.clone());
            }
            None => {
                added.push(name.clone());
            }
            _ => {} // unchanged
        }
    }

    // Files in old but not new
    for name in old_package.files.keys() {
        if !new_package.files.contains_key(name) {
            removed.push(name.clone());
        }
    }

    BmiDelta {
        added,
        modified,
        removed,
        old_version: old_package.metadata.version.clone(),
        new_version: new_package.metadata.version.clone(),
    }
}

/// Delta between two BMI package versions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BmiDelta {
    /// Files added in the new version.
    pub added: Vec<String>,
    /// Files modified between versions.
    pub modified: Vec<String>,
    /// Files removed in the new version.
    pub removed: Vec<String>,
    /// Old version.
    pub old_version: String,
    /// New version.
    pub new_version: String,
}

impl BmiDelta {
    pub fn is_empty(&self) -> bool {
        self.added.is_empty() && self.modified.is_empty() && self.removed.is_empty()
    }

    pub fn changed_file_count(&self) -> usize {
        self.added.len() + self.modified.len() + self.removed.len()
    }
}

/// Remote BMI distributor that fetches precompiled modules from HTTP endpoints.
pub struct BmiDistributor {
    /// Base URL for the BMI distribution server.
    base_url: String,
    /// HTTP timeout in seconds.
    timeout_secs: u64,
}

impl BmiDistributor {
    pub fn new(base_url: &str) -> Self {
        BmiDistributor {
            base_url: base_url.trim_end_matches('/').to_string(),
            timeout_secs: 30,
        }
    }

    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    /// Fetch a remote BMI index for a module.
    pub fn fetch_index(&self, module_name: &str) -> Result<BmiIndex, CmodError> {
        let url = format!("{}/bmi/{}/index.json", self.base_url, module_name);

        let agent = ureq::Agent::new_with_config(
            ureq::config::Config::builder()
                .timeout_global(Some(std::time::Duration::from_secs(self.timeout_secs)))
                .http_status_as_error(false)
                .build(),
        );

        let resp = agent
            .get(&url)
            .call()
            .map_err(|e| CmodError::Other(format!("failed to fetch BMI index: {}", e)))?;

        if resp.status().as_u16() != 200 {
            return Err(CmodError::Other(format!(
                "BMI index not found for '{}' (HTTP {})",
                module_name,
                resp.status()
            )));
        }

        let body: String = resp
            .into_body()
            .read_to_string()
            .map_err(|e| CmodError::Other(format!("failed to read BMI index: {}", e)))?;

        serde_json::from_str(&body)
            .map_err(|e| CmodError::Other(format!("failed to parse BMI index: {}", e)))
    }

    /// Fetch a precompiled BMI variant and save it to a local directory.
    pub fn fetch_variant(
        &self,
        module_name: &str,
        variant: &BmiVariant,
        output_dir: &Path,
    ) -> Result<PathBuf, CmodError> {
        let variant_dir = output_dir.join(&variant.directory);
        fs::create_dir_all(&variant_dir)?;

        // Fetch the metadata file
        let metadata_url = format!(
            "{}/bmi/{}/{}/metadata.json",
            self.base_url, module_name, variant.directory
        );
        self.download_file(&metadata_url, &variant_dir.join("metadata.json"))?;

        // Fetch the package manifest
        let package_url = format!(
            "{}/bmi/{}/{}/bmi_package.json",
            self.base_url, module_name, variant.directory
        );
        self.download_file(&package_url, &variant_dir.join("bmi_package.json"))?;

        Ok(variant_dir)
    }

    /// Download a single file from a URL to a local path.
    fn download_file(&self, url: &str, dest: &Path) -> Result<(), CmodError> {
        let agent = ureq::Agent::new_with_config(
            ureq::config::Config::builder()
                .timeout_global(Some(std::time::Duration::from_secs(self.timeout_secs)))
                .http_status_as_error(false)
                .build(),
        );

        let resp = agent
            .get(url)
            .call()
            .map_err(|e| CmodError::Other(format!("failed to download {}: {}", url, e)))?;

        if resp.status().as_u16() != 200 {
            return Err(CmodError::Other(format!(
                "download failed for {} (HTTP {})",
                url,
                resp.status()
            )));
        }

        let body: String = resp
            .into_body()
            .read_to_string()
            .map_err(|e| CmodError::Other(format!("failed to read response body: {}", e)))?;

        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(dest, body)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use tempfile::TempDir;

    fn make_metadata(compiler: &str, version: &str, target: &str) -> BmiMetadata {
        BmiMetadata {
            module_name: "test".to_string(),
            version: "1.0.0".to_string(),
            compiler: compiler.to_string(),
            compiler_version: version.to_string(),
            target: target.to_string(),
            cxx_standard: "20".to_string(),
            stdlib: None,
            abi: None,
            source_commit: None,
            pcm_hash: None,
            obj_hash: None,
            signature: None,
            created_at: "0".to_string(),
            extra: BTreeMap::new(),
        }
    }

    #[test]
    fn test_publish_bmi() {
        let tmp = TempDir::new().unwrap();
        let package = BmiPackage {
            metadata: make_metadata("clang", "18.1.0", "x86_64-unknown-linux-gnu"),
            files: BTreeMap::from([("module.pcm".into(), "hash1".into())]),
        };

        let result = publish_bmi(&package, tmp.path());
        assert!(result.is_ok());
        let variant_dir = result.unwrap();
        assert!(variant_dir.join("metadata.json").exists());
        assert!(variant_dir.join("bmi_package.json").exists());
    }

    #[test]
    fn test_find_compatible_variant_exact() {
        let index = BmiIndex {
            module_name: "test".into(),
            version: "1.0.0".into(),
            format_version: 1,
            variants: vec![BmiVariant {
                compiler: "clang".into(),
                compiler_version: "18.1.0".into(),
                target: "x86_64-unknown-linux-gnu".into(),
                cxx_standard: "20".into(),
                stdlib: None,
                directory: "variant1".into(),
                package_hash: "abc".into(),
                size_bytes: 1024,
                built_at: "0".into(),
            }],
        };

        assert!(find_compatible_variant(
            &index,
            "clang",
            "18.1.0",
            "x86_64-unknown-linux-gnu",
            "20"
        )
        .is_some());

        assert!(
            find_compatible_variant(&index, "gcc", "13.0.0", "x86_64-unknown-linux-gnu", "20")
                .is_none()
        );
    }

    #[test]
    fn test_find_compatible_variant_fuzzy() {
        let index = BmiIndex {
            module_name: "test".into(),
            version: "1.0.0".into(),
            format_version: 1,
            variants: vec![BmiVariant {
                compiler: "clang".into(),
                compiler_version: "18.1.0".into(),
                target: "x86_64-unknown-linux-gnu".into(),
                cxx_standard: "20".into(),
                stdlib: None,
                directory: "variant1".into(),
                package_hash: "abc".into(),
                size_bytes: 1024,
                built_at: "0".into(),
            }],
        };

        // Should fuzzy-match 18.2.0 to 18.1.0 (same major version)
        assert!(find_compatible_variant(
            &index,
            "clang",
            "18.2.0",
            "x86_64-unknown-linux-gnu",
            "20"
        )
        .is_some());
    }

    #[test]
    fn test_compute_bmi_delta() {
        let old = BmiPackage {
            metadata: make_metadata("clang", "18", "x86_64"),
            files: BTreeMap::from([
                ("a.pcm".into(), "hash1".into()),
                ("b.o".into(), "hash2".into()),
            ]),
        };

        let new = BmiPackage {
            metadata: BmiMetadata {
                version: "1.1.0".to_string(),
                ..make_metadata("clang", "18", "x86_64")
            },
            files: BTreeMap::from([
                ("a.pcm".into(), "hash1_modified".into()),
                ("c.pcm".into(), "hash3".into()),
            ]),
        };

        let delta = compute_bmi_delta(&old, &new);
        assert_eq!(delta.modified, vec!["a.pcm"]);
        assert_eq!(delta.added, vec!["c.pcm"]);
        assert_eq!(delta.removed, vec!["b.o"]);
        assert_eq!(delta.old_version, "1.0.0");
        assert_eq!(delta.new_version, "1.1.0");
        assert_eq!(delta.changed_file_count(), 3);
    }

    #[test]
    fn test_bmi_delta_empty() {
        let pkg = BmiPackage {
            metadata: make_metadata("clang", "18", "x86_64"),
            files: BTreeMap::from([("a.pcm".into(), "hash1".into())]),
        };

        let delta = compute_bmi_delta(&pkg, &pkg);
        assert!(delta.is_empty());
    }

    #[test]
    fn test_bmi_index_serde() {
        let index = BmiIndex {
            module_name: "test".into(),
            version: "1.0.0".into(),
            format_version: 1,
            variants: vec![],
        };
        let json = serde_json::to_string(&index).unwrap();
        let parsed: BmiIndex = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.module_name, "test");
    }
}
