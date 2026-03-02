use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

use cmod_core::error::CmodError;

use crate::key::hash_file;

/// Metadata describing a precompiled Binary Module Interface (BMI).
///
/// This is written alongside exported BMI packages for distribution,
/// enabling consumers to verify toolchain compatibility and integrity.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BmiMetadata {
    /// Module name (fully qualified, e.g., "github.fmtlib.fmt").
    pub module_name: String,
    /// Module version.
    pub version: String,
    /// Compiler used (e.g., "clang", "gcc").
    pub compiler: String,
    /// Compiler version (e.g., "18.1.0").
    pub compiler_version: String,
    /// Target triple (e.g., "x86_64-unknown-linux-gnu").
    pub target: String,
    /// C++ standard (e.g., "20", "23").
    pub cxx_standard: String,
    /// Standard library (e.g., "libc++", "libstdc++").
    pub stdlib: Option<String>,
    /// ABI variant (e.g., "itanium", "msvc").
    pub abi: Option<String>,
    /// SHA-256 hash of the source commit used to build.
    pub source_commit: Option<String>,
    /// SHA-256 hash of the PCM file.
    pub pcm_hash: Option<String>,
    /// SHA-256 hash of the object file.
    pub obj_hash: Option<String>,
    /// Cryptographic signature of the BMI package.
    pub signature: Option<String>,
    /// ISO 8601 creation timestamp.
    pub created_at: String,
    /// Additional metadata.
    #[serde(default)]
    pub extra: BTreeMap<String, String>,
}

/// A BMI export package: metadata + artifact files bundled together.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BmiPackage {
    /// Metadata about the exported BMI.
    pub metadata: BmiMetadata,
    /// Artifact file entries (filename → SHA-256 hash).
    pub files: BTreeMap<String, String>,
}

impl BmiMetadata {
    /// Check if this BMI is compatible with a given toolchain configuration.
    pub fn is_compatible(
        &self,
        compiler: &str,
        compiler_version: &str,
        target: &str,
        cxx_standard: &str,
    ) -> bool {
        self.compiler == compiler
            && self.compiler_version == compiler_version
            && self.target == target
            && self.cxx_standard == cxx_standard
    }

    /// Generate a compatibility key string for cache lookup.
    pub fn compat_key(&self) -> String {
        format!(
            "{}-{}-std{}-{}-{}",
            self.compiler,
            self.compiler_version,
            self.cxx_standard,
            self.stdlib.as_deref().unwrap_or("default"),
            self.target,
        )
    }
}

/// Export a cached module as a BMI package to a directory.
///
/// Creates a directory containing the PCM file, object file, and metadata JSON.
pub fn export_bmi(
    cache_root: &Path,
    module_name: &str,
    cache_key: &str,
    output_dir: &Path,
) -> Result<BmiPackage, CmodError> {
    let entry_dir = cache_root.join(module_name).join(cache_key);
    if !entry_dir.exists() {
        return Err(CmodError::Other(format!(
            "cache entry not found: {}/{}",
            module_name, cache_key
        )));
    }

    // Read existing metadata
    let metadata_path = entry_dir.join("metadata.json");
    let base_metadata: BmiMetadata = if metadata_path.exists() {
        let content = fs::read_to_string(&metadata_path)?;
        serde_json::from_str(&content)
            .map_err(|e| CmodError::Other(format!("failed to parse BMI metadata: {}", e)))?
    } else {
        // Create minimal metadata from cache entry
        let now = chrono_now();
        BmiMetadata {
            module_name: module_name.to_string(),
            version: "0.0.0".to_string(),
            compiler: "unknown".to_string(),
            compiler_version: "unknown".to_string(),
            target: "unknown".to_string(),
            cxx_standard: "20".to_string(),
            stdlib: None,
            abi: None,
            source_commit: None,
            pcm_hash: None,
            obj_hash: None,
            signature: None,
            created_at: now,
            extra: BTreeMap::new(),
        }
    };

    // Ensure output directory exists
    fs::create_dir_all(output_dir)?;

    // Copy artifacts and compute hashes
    let mut files = BTreeMap::new();

    for entry in fs::read_dir(&entry_dir)? {
        let entry = entry?;
        let file_name = entry.file_name().to_string_lossy().to_string();
        let src = entry.path();
        if src.is_file() {
            let hash = hash_file(&src).map_err(|e| {
                CmodError::Other(format!("failed to hash {}: {}", src.display(), e))
            })?;
            let dest = output_dir.join(&file_name);
            fs::copy(&src, &dest)?;
            files.insert(file_name, hash);
        }
    }

    // Write the BMI package metadata
    let package = BmiPackage {
        metadata: base_metadata,
        files,
    };

    let package_json = serde_json::to_string_pretty(&package)
        .map_err(|e| CmodError::Other(format!("failed to serialize BMI package: {}", e)))?;
    fs::write(output_dir.join("bmi_package.json"), &package_json)?;

    Ok(package)
}

/// Import a BMI package from a directory into the local cache.
pub fn import_bmi(cache_root: &Path, package_dir: &Path) -> Result<BmiMetadata, CmodError> {
    let package_path = package_dir.join("bmi_package.json");
    if !package_path.exists() {
        return Err(CmodError::Other(format!(
            "no bmi_package.json found in {}",
            package_dir.display()
        )));
    }

    let content = fs::read_to_string(&package_path)?;
    let package: BmiPackage = serde_json::from_str(&content)
        .map_err(|e| CmodError::Other(format!("failed to parse BMI package: {}", e)))?;

    // Verify file hashes
    for (file_name, expected_hash) in &package.files {
        let file_path = package_dir.join(file_name);
        if !file_path.exists() {
            return Err(CmodError::SecurityViolation {
                reason: format!("BMI package missing file: {}", file_name),
            });
        }
        let actual_hash = hash_file(&file_path)
            .map_err(|e| CmodError::Other(format!("failed to hash {}: {}", file_name, e)))?;
        if &actual_hash != expected_hash {
            return Err(CmodError::SecurityViolation {
                reason: format!(
                    "BMI file hash mismatch for '{}': expected {}, got {}",
                    file_name, expected_hash, actual_hash
                ),
            });
        }
    }

    // Determine cache entry location
    let cache_key = package.metadata.compat_key();
    let entry_dir = cache_root
        .join(&package.metadata.module_name)
        .join(&cache_key);
    fs::create_dir_all(&entry_dir)?;

    // Copy files into cache
    for file_name in package.files.keys() {
        let src = package_dir.join(file_name);
        let dest = entry_dir.join(file_name);
        fs::copy(&src, &dest)?;
    }

    // Write metadata
    let metadata_json = serde_json::to_string_pretty(&package.metadata)
        .map_err(|e| CmodError::Other(format!("failed to serialize BMI metadata: {}", e)))?;
    fs::write(entry_dir.join("metadata.json"), &metadata_json)?;

    Ok(package.metadata)
}

fn chrono_now() -> String {
    // Simple ISO-like timestamp without chrono dependency
    let dur = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default();
    format!("{}", dur.as_secs())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_bmi_metadata_compat_key() {
        let meta = BmiMetadata {
            module_name: "github.fmtlib.fmt".to_string(),
            version: "10.2.0".to_string(),
            compiler: "clang".to_string(),
            compiler_version: "18.1.0".to_string(),
            target: "x86_64-unknown-linux-gnu".to_string(),
            cxx_standard: "20".to_string(),
            stdlib: Some("libc++".to_string()),
            abi: Some("itanium".to_string()),
            source_commit: None,
            pcm_hash: None,
            obj_hash: None,
            signature: None,
            created_at: "0".to_string(),
            extra: BTreeMap::new(),
        };

        let key = meta.compat_key();
        assert!(key.contains("clang"));
        assert!(key.contains("18.1.0"));
        assert!(key.contains("std20"));
        assert!(key.contains("libc++"));
        assert!(key.contains("x86_64"));
    }

    #[test]
    fn test_bmi_metadata_is_compatible() {
        let meta = BmiMetadata {
            module_name: "test".to_string(),
            version: "1.0.0".to_string(),
            compiler: "clang".to_string(),
            compiler_version: "18.1.0".to_string(),
            target: "x86_64-unknown-linux-gnu".to_string(),
            cxx_standard: "20".to_string(),
            stdlib: None,
            abi: None,
            source_commit: None,
            pcm_hash: None,
            obj_hash: None,
            signature: None,
            created_at: "0".to_string(),
            extra: BTreeMap::new(),
        };

        assert!(meta.is_compatible("clang", "18.1.0", "x86_64-unknown-linux-gnu", "20"));
        assert!(!meta.is_compatible("gcc", "18.1.0", "x86_64-unknown-linux-gnu", "20"));
        assert!(!meta.is_compatible("clang", "17.0.0", "x86_64-unknown-linux-gnu", "20"));
        assert!(!meta.is_compatible("clang", "18.1.0", "aarch64-unknown-linux-gnu", "20"));
        assert!(!meta.is_compatible("clang", "18.1.0", "x86_64-unknown-linux-gnu", "23"));
    }

    #[test]
    fn test_bmi_package_serde_roundtrip() {
        let package = BmiPackage {
            metadata: BmiMetadata {
                module_name: "test".to_string(),
                version: "1.0.0".to_string(),
                compiler: "clang".to_string(),
                compiler_version: "18.0".to_string(),
                target: "x86_64".to_string(),
                cxx_standard: "20".to_string(),
                stdlib: None,
                abi: None,
                source_commit: Some("abc123".to_string()),
                pcm_hash: Some("hash1".to_string()),
                obj_hash: Some("hash2".to_string()),
                signature: None,
                created_at: "12345".to_string(),
                extra: BTreeMap::new(),
            },
            files: BTreeMap::from([
                ("module.pcm".to_string(), "pcmhash".to_string()),
                ("object.o".to_string(), "objhash".to_string()),
            ]),
        };

        let json = serde_json::to_string(&package).unwrap();
        let parsed: BmiPackage = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.metadata.module_name, "test");
        assert_eq!(parsed.files.len(), 2);
    }

    #[test]
    fn test_export_import_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let cache_root = tmp.path().join("cache");
        let export_dir = tmp.path().join("export");

        // Create a fake cache entry
        let entry_dir = cache_root.join("mymod").join("testkey");
        fs::create_dir_all(&entry_dir).unwrap();
        fs::write(entry_dir.join("module.pcm"), b"fake pcm data").unwrap();
        fs::write(entry_dir.join("object.o"), b"fake object data").unwrap();

        // Export
        let package = export_bmi(&cache_root, "mymod", "testkey", &export_dir).unwrap();
        assert_eq!(package.metadata.module_name, "mymod");
        assert!(package.files.contains_key("module.pcm"));
        assert!(package.files.contains_key("object.o"));

        // Import into a fresh cache
        let new_cache = tmp.path().join("cache2");
        let meta = import_bmi(&new_cache, &export_dir).unwrap();
        assert_eq!(meta.module_name, "mymod");

        // Verify files exist in new cache
        let compat_key = meta.compat_key();
        let imported_dir = new_cache.join("mymod").join(&compat_key);
        assert!(imported_dir.join("module.pcm").exists());
        assert!(imported_dir.join("object.o").exists());
    }

    #[test]
    fn test_import_detects_hash_mismatch() {
        let tmp = TempDir::new().unwrap();
        let pkg_dir = tmp.path().join("pkg");
        fs::create_dir_all(&pkg_dir).unwrap();

        // Create a package with a wrong hash
        let package = BmiPackage {
            metadata: BmiMetadata {
                module_name: "test".to_string(),
                version: "1.0.0".to_string(),
                compiler: "clang".to_string(),
                compiler_version: "18".to_string(),
                target: "x86_64".to_string(),
                cxx_standard: "20".to_string(),
                stdlib: None,
                abi: None,
                source_commit: None,
                pcm_hash: None,
                obj_hash: None,
                signature: None,
                created_at: "0".to_string(),
                extra: BTreeMap::new(),
            },
            files: BTreeMap::from([("module.pcm".to_string(), "wrong_hash".to_string())]),
        };

        let json = serde_json::to_string(&package).unwrap();
        fs::write(pkg_dir.join("bmi_package.json"), &json).unwrap();
        fs::write(pkg_dir.join("module.pcm"), b"actual data").unwrap();

        let cache_root = tmp.path().join("cache");
        let result = import_bmi(&cache_root, &pkg_dir);
        assert!(result.is_err());
    }
}
