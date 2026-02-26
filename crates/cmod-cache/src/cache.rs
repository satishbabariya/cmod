use std::fs;
use std::path::{Path, PathBuf};

use cmod_core::error::CmodError;
use serde::{Deserialize, Serialize};

use crate::key::{hash_file, CacheKey};

/// Metadata stored alongside cached artifacts.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactMetadata {
    pub module_name: String,
    pub cache_key: String,
    pub source_hash: String,
    pub compiler: String,
    pub compiler_version: String,
    pub target: String,
    pub created_at: String,
    pub artifacts: Vec<CachedArtifactEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedArtifactEntry {
    pub name: String,
    pub hash: String,
    pub size: u64,
}

/// Local artifact cache.
///
/// Layout:
/// ```text
/// <cache_root>/
///   <module_id>/
///     <cache_key>/
///       metadata.json
///       module.pcm
///       object.o
/// ```
pub struct ArtifactCache {
    root: PathBuf,
}

impl ArtifactCache {
    /// Create a new cache rooted at the given directory.
    pub fn new(root: PathBuf) -> Self {
        ArtifactCache { root }
    }

    /// Return the cache root path.
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Get the cache directory for a given module + key.
    pub fn entry_dir(&self, module_id: &str, key: &CacheKey) -> PathBuf {
        self.root.join(module_id).join(&key.0)
    }

    /// Check if a cache entry exists and is valid.
    pub fn has(&self, module_id: &str, key: &CacheKey) -> bool {
        let dir = self.entry_dir(module_id, key);
        dir.join("metadata.json").exists()
    }

    /// Store artifacts in the cache.
    pub fn store(
        &self,
        module_id: &str,
        key: &CacheKey,
        metadata: &ArtifactMetadata,
        artifact_files: &[(&str, &Path)],
    ) -> Result<(), CmodError> {
        let dir = self.entry_dir(module_id, key);
        fs::create_dir_all(&dir)?;

        // Write metadata
        let meta_json = serde_json::to_string_pretty(metadata).map_err(|e| {
            CmodError::CacheError {
                reason: format!("failed to serialize metadata: {}", e),
            }
        })?;
        fs::write(dir.join("metadata.json"), meta_json)?;

        // Copy artifact files
        for (name, src) in artifact_files {
            let dest = dir.join(name);
            fs::copy(src, &dest)?;
        }

        Ok(())
    }

    /// Retrieve a cached artifact file path. Returns None if not cached.
    pub fn get_artifact(
        &self,
        module_id: &str,
        key: &CacheKey,
        artifact_name: &str,
    ) -> Option<PathBuf> {
        let path = self.entry_dir(module_id, key).join(artifact_name);
        if path.exists() {
            Some(path)
        } else {
            None
        }
    }

    /// Load metadata for a cache entry.
    pub fn get_metadata(
        &self,
        module_id: &str,
        key: &CacheKey,
    ) -> Result<ArtifactMetadata, CmodError> {
        let path = self.entry_dir(module_id, key).join("metadata.json");
        let content = fs::read_to_string(&path).map_err(|_| CmodError::CacheError {
            reason: format!("cache metadata not found for {} at key {}", module_id, key),
        })?;
        serde_json::from_str(&content).map_err(|e| CmodError::CacheError {
            reason: format!("invalid cache metadata: {}", e),
        })
    }

    /// Remove a single cache entry.
    pub fn evict(&self, module_id: &str, key: &CacheKey) -> Result<(), CmodError> {
        let dir = self.entry_dir(module_id, key);
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
        }
        Ok(())
    }

    /// Remove all cache entries for a module.
    pub fn evict_module(&self, module_id: &str) -> Result<(), CmodError> {
        let dir = self.root.join(module_id);
        if dir.exists() {
            fs::remove_dir_all(&dir)?;
        }
        Ok(())
    }

    /// Clear the entire cache.
    pub fn clean(&self) -> Result<(), CmodError> {
        if self.root.exists() {
            fs::remove_dir_all(&self.root)?;
            fs::create_dir_all(&self.root)?;
        }
        Ok(())
    }

    /// Compute total cache size in bytes.
    pub fn total_size(&self) -> Result<u64, CmodError> {
        if !self.root.exists() {
            return Ok(0);
        }
        let mut total = 0u64;
        for entry in walkdir::WalkDir::new(&self.root)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            if entry.file_type().is_file() {
                total += entry.metadata().map(|m| m.len()).unwrap_or(0);
            }
        }
        Ok(total)
    }

    /// List all cached modules.
    pub fn list_modules(&self) -> Result<Vec<String>, CmodError> {
        let mut modules = Vec::new();
        if !self.root.exists() {
            return Ok(modules);
        }
        for entry in fs::read_dir(&self.root)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                if let Some(name) = entry.file_name().to_str() {
                    modules.push(name.to_string());
                }
            }
        }
        modules.sort();
        Ok(modules)
    }

    /// Verify integrity of a cached artifact against its recorded hash.
    pub fn verify_artifact(
        &self,
        module_id: &str,
        key: &CacheKey,
        artifact_name: &str,
    ) -> Result<bool, CmodError> {
        let metadata = self.get_metadata(module_id, key)?;
        let artifact_path = self.entry_dir(module_id, key).join(artifact_name);

        if !artifact_path.exists() {
            return Ok(false);
        }

        let actual_hash = hash_file(&artifact_path)?;

        // Find the expected hash in metadata
        for entry in &metadata.artifacts {
            if entry.name == artifact_name {
                return Ok(entry.hash == actual_hash);
            }
        }

        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_cache() -> (TempDir, ArtifactCache) {
        let tmp = TempDir::new().unwrap();
        let cache = ArtifactCache::new(tmp.path().to_path_buf());
        (tmp, cache)
    }

    #[test]
    fn test_cache_has_miss() {
        let (_tmp, cache) = test_cache();
        let key = CacheKey("deadbeef".to_string());
        assert!(!cache.has("some.module", &key));
    }

    #[test]
    fn test_cache_store_and_retrieve() {
        let (tmp, cache) = test_cache();
        let key = CacheKey("testkey123".to_string());

        // Create a fake artifact file
        let artifact_path = tmp.path().join("test.o");
        fs::write(&artifact_path, b"fake object file").unwrap();

        let metadata = ArtifactMetadata {
            module_name: "test.module".to_string(),
            cache_key: key.0.clone(),
            source_hash: "srchash".to_string(),
            compiler: "clang".to_string(),
            compiler_version: "18.1.0".to_string(),
            target: "x86_64-unknown-linux-gnu".to_string(),
            created_at: "2025-01-01T00:00:00Z".to_string(),
            artifacts: vec![CachedArtifactEntry {
                name: "test.o".to_string(),
                hash: "fakehash".to_string(),
                size: 16,
            }],
        };

        cache
            .store("test.module", &key, &metadata, &[("test.o", &artifact_path)])
            .unwrap();

        assert!(cache.has("test.module", &key));

        let retrieved = cache.get_artifact("test.module", &key, "test.o");
        assert!(retrieved.is_some());

        let loaded_meta = cache.get_metadata("test.module", &key).unwrap();
        assert_eq!(loaded_meta.module_name, "test.module");
    }

    #[test]
    fn test_cache_evict() {
        let (tmp, cache) = test_cache();
        let key = CacheKey("evictme".to_string());

        let artifact_path = tmp.path().join("dummy.o");
        fs::write(&artifact_path, b"data").unwrap();

        let metadata = ArtifactMetadata {
            module_name: "mod".to_string(),
            cache_key: key.0.clone(),
            source_hash: "h".to_string(),
            compiler: "clang".to_string(),
            compiler_version: "18".to_string(),
            target: "x86_64".to_string(),
            created_at: "now".to_string(),
            artifacts: vec![],
        };

        cache
            .store("mod", &key, &metadata, &[("dummy.o", &artifact_path)])
            .unwrap();
        assert!(cache.has("mod", &key));

        cache.evict("mod", &key).unwrap();
        assert!(!cache.has("mod", &key));
    }

    #[test]
    fn test_cache_clean() {
        let (tmp, cache) = test_cache();
        let key = CacheKey("k".to_string());
        let artifact_path = tmp.path().join("f.o");
        fs::write(&artifact_path, b"x").unwrap();

        let metadata = ArtifactMetadata {
            module_name: "m".to_string(),
            cache_key: key.0.clone(),
            source_hash: "h".to_string(),
            compiler: "c".to_string(),
            compiler_version: "1".to_string(),
            target: "t".to_string(),
            created_at: "now".to_string(),
            artifacts: vec![],
        };

        cache
            .store("m", &key, &metadata, &[("f.o", &artifact_path)])
            .unwrap();
        cache.clean().unwrap();
        assert!(!cache.has("m", &key));
    }

    #[test]
    fn test_cache_list_modules() {
        let (tmp, cache) = test_cache();
        let key = CacheKey("k".to_string());
        let artifact = tmp.path().join("x.o");
        fs::write(&artifact, b"data").unwrap();

        let meta = ArtifactMetadata {
            module_name: "alpha".to_string(),
            cache_key: key.0.clone(),
            source_hash: "h".to_string(),
            compiler: "c".to_string(),
            compiler_version: "1".to_string(),
            target: "t".to_string(),
            created_at: "now".to_string(),
            artifacts: vec![],
        };

        cache.store("alpha", &key, &meta, &[("x.o", &artifact)]).unwrap();
        cache.store("beta", &key, &meta, &[("x.o", &artifact)]).unwrap();

        let modules = cache.list_modules().unwrap();
        assert_eq!(modules, vec!["alpha", "beta"]);
    }

    #[test]
    fn test_cache_total_size() {
        let (tmp, cache) = test_cache();
        let key = CacheKey("k".to_string());
        let artifact = tmp.path().join("data.o");
        fs::write(&artifact, b"hello world!").unwrap(); // 12 bytes

        let meta = ArtifactMetadata {
            module_name: "m".to_string(),
            cache_key: key.0.clone(),
            source_hash: "h".to_string(),
            compiler: "c".to_string(),
            compiler_version: "1".to_string(),
            target: "t".to_string(),
            created_at: "now".to_string(),
            artifacts: vec![],
        };

        cache.store("m", &key, &meta, &[("data.o", &artifact)]).unwrap();

        let size = cache.total_size().unwrap();
        assert!(size > 0, "cache size should be > 0 after storing artifacts");
    }

    #[test]
    fn test_cache_evict_module() {
        let (tmp, cache) = test_cache();
        let key1 = CacheKey("k1".to_string());
        let key2 = CacheKey("k2".to_string());
        let artifact = tmp.path().join("x.o");
        fs::write(&artifact, b"data").unwrap();

        let meta = ArtifactMetadata {
            module_name: "target_mod".to_string(),
            cache_key: "k".to_string(),
            source_hash: "h".to_string(),
            compiler: "c".to_string(),
            compiler_version: "1".to_string(),
            target: "t".to_string(),
            created_at: "now".to_string(),
            artifacts: vec![],
        };

        cache.store("target_mod", &key1, &meta, &[("x.o", &artifact)]).unwrap();
        cache.store("target_mod", &key2, &meta, &[("x.o", &artifact)]).unwrap();
        cache.store("other_mod", &key1, &meta, &[("x.o", &artifact)]).unwrap();

        // Evict all entries for target_mod
        cache.evict_module("target_mod").unwrap();

        assert!(!cache.has("target_mod", &key1));
        assert!(!cache.has("target_mod", &key2));
        // other_mod should still exist
        assert!(cache.has("other_mod", &key1));
    }

    #[test]
    fn test_cache_get_artifact_miss() {
        let (_tmp, cache) = test_cache();
        let key = CacheKey("k".to_string());
        assert!(cache.get_artifact("missing", &key, "test.o").is_none());
    }

    #[test]
    fn test_cache_verify_artifact() {
        let (tmp, cache) = test_cache();
        let key = CacheKey("k".to_string());
        let artifact = tmp.path().join("verified.o");
        let content = b"verified content";
        fs::write(&artifact, content).unwrap();

        // Compute the actual hash
        let actual_hash = hash_file(&artifact).unwrap();

        let meta = ArtifactMetadata {
            module_name: "m".to_string(),
            cache_key: key.0.clone(),
            source_hash: "h".to_string(),
            compiler: "c".to_string(),
            compiler_version: "1".to_string(),
            target: "t".to_string(),
            created_at: "now".to_string(),
            artifacts: vec![CachedArtifactEntry {
                name: "verified.o".to_string(),
                hash: actual_hash,
                size: content.len() as u64,
            }],
        };

        cache.store("m", &key, &meta, &[("verified.o", &artifact)]).unwrap();

        // Verification should pass
        assert!(cache.verify_artifact("m", &key, "verified.o").unwrap());
    }

    #[test]
    fn test_cache_verify_corrupt_artifact() {
        let (tmp, cache) = test_cache();
        let key = CacheKey("k".to_string());
        let artifact = tmp.path().join("corrupt.o");
        fs::write(&artifact, b"original").unwrap();

        let meta = ArtifactMetadata {
            module_name: "m".to_string(),
            cache_key: key.0.clone(),
            source_hash: "h".to_string(),
            compiler: "c".to_string(),
            compiler_version: "1".to_string(),
            target: "t".to_string(),
            created_at: "now".to_string(),
            artifacts: vec![CachedArtifactEntry {
                name: "corrupt.o".to_string(),
                hash: "wrong_hash".to_string(),
                size: 8,
            }],
        };

        cache.store("m", &key, &meta, &[("corrupt.o", &artifact)]).unwrap();

        // Verification should fail (hash mismatch)
        assert!(!cache.verify_artifact("m", &key, "corrupt.o").unwrap());
    }

    #[test]
    fn test_cache_root() {
        let (tmp, cache) = test_cache();
        assert_eq!(cache.root(), tmp.path());
    }

    #[test]
    fn test_cache_entry_dir() {
        let (_tmp, cache) = test_cache();
        let key = CacheKey("abc123".to_string());
        let dir = cache.entry_dir("github.fmtlib.fmt", &key);
        assert!(dir.ends_with("github.fmtlib.fmt/abc123"));
    }
}
