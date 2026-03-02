//! Remote/shared cache protocol for distributing build artifacts.
//!
//! Provides a trait-based abstraction for remote caches and an HTTP implementation
//! following a simple REST protocol:
//!
//! - `HEAD /cache/<module_id>/<key>` — check existence
//! - `GET  /cache/<module_id>/<key>/<artifact>` — download artifact
//! - `PUT  /cache/<module_id>/<key>/<artifact>` — upload artifact

use std::path::Path;

use cmod_core::error::CmodError;

use crate::key::CacheKey;

/// Remote cache access mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RemoteCacheMode {
    /// Remote cache disabled.
    Off,
    /// Read-only: download hits but never upload.
    ReadOnly,
    /// Read-write: download and upload.
    ReadWrite,
}

impl RemoteCacheMode {
    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "readonly" | "read-only" | "ro" => RemoteCacheMode::ReadOnly,
            "readwrite" | "read-write" | "rw" => RemoteCacheMode::ReadWrite,
            _ => RemoteCacheMode::Off,
        }
    }
}

/// Trait for remote cache backends.
///
/// Implementations handle the actual network transport (HTTP, S3, GCS, etc.).
pub trait RemoteCache: Send + Sync {
    /// Check if an artifact set exists for the given module and cache key.
    fn has(&self, module_id: &str, key: &CacheKey) -> Result<bool, CmodError>;

    /// Download an artifact to a local destination.
    ///
    /// Returns `true` if the artifact was successfully downloaded, `false` if not found.
    fn get(
        &self,
        module_id: &str,
        key: &CacheKey,
        artifact_name: &str,
        dest: &Path,
    ) -> Result<bool, CmodError>;

    /// Upload an artifact to the remote cache.
    fn put(
        &self,
        module_id: &str,
        key: &CacheKey,
        artifact_name: &str,
        source: &Path,
    ) -> Result<(), CmodError>;

    /// A descriptive name for this remote cache backend.
    fn name(&self) -> &str;
}

/// HTTP-based remote cache implementation.
///
/// Uses a simple REST protocol for artifact storage and retrieval.
pub struct HttpRemoteCache {
    /// Base URL of the cache server (e.g., `https://cache.example.com`).
    base_url: String,
    /// Access mode.
    mode: RemoteCacheMode,
}

impl HttpRemoteCache {
    /// Create a new HTTP remote cache client.
    pub fn new(base_url: &str, mode: RemoteCacheMode) -> Self {
        let base_url = base_url.trim_end_matches('/').to_string();
        HttpRemoteCache { base_url, mode }
    }

    /// Construct the URL for a cache entry.
    fn cache_url(&self, module_id: &str, key: &CacheKey, artifact: Option<&str>) -> String {
        match artifact {
            Some(name) => format!("{}/cache/{}/{}/{}", self.base_url, module_id, key, name),
            None => format!("{}/cache/{}/{}", self.base_url, module_id, key),
        }
    }

    /// Whether writes are allowed.
    pub fn can_write(&self) -> bool {
        self.mode == RemoteCacheMode::ReadWrite
    }

    /// Whether reads are allowed.
    pub fn can_read(&self) -> bool {
        self.mode != RemoteCacheMode::Off
    }
}

impl RemoteCache for HttpRemoteCache {
    fn has(&self, module_id: &str, key: &CacheKey) -> Result<bool, CmodError> {
        if !self.can_read() {
            return Ok(false);
        }

        let url = self.cache_url(module_id, key, None);

        // Use std::process::Command to call curl for HEAD request
        // (avoids adding a heavy HTTP client dependency)
        let output = std::process::Command::new("curl")
            .args([
                "-s",
                "-o",
                "/dev/null",
                "-w",
                "%{http_code}",
                "--head",
                &url,
            ])
            .output()
            .map_err(|e| CmodError::Other(format!("curl not available: {}", e)))?;

        let status = String::from_utf8_lossy(&output.stdout);
        Ok(status.trim() == "200")
    }

    fn get(
        &self,
        module_id: &str,
        key: &CacheKey,
        artifact_name: &str,
        dest: &Path,
    ) -> Result<bool, CmodError> {
        if !self.can_read() {
            return Ok(false);
        }

        let url = self.cache_url(module_id, key, Some(artifact_name));

        if let Some(parent) = dest.parent() {
            std::fs::create_dir_all(parent)?;
        }

        let output = std::process::Command::new("curl")
            .args([
                "-s",
                "-o",
                &dest.display().to_string(),
                "-w",
                "%{http_code}",
                "--fail",
                &url,
            ])
            .output()
            .map_err(|e| CmodError::Other(format!("curl not available: {}", e)))?;

        let status = String::from_utf8_lossy(&output.stdout);
        if status.trim() == "200" {
            // Verify the download is not empty
            let meta = std::fs::metadata(dest)?;
            if meta.len() > 0 {
                return Ok(true);
            }
        }

        // Clean up failed download
        let _ = std::fs::remove_file(dest);
        Ok(false)
    }

    fn put(
        &self,
        module_id: &str,
        key: &CacheKey,
        artifact_name: &str,
        source: &Path,
    ) -> Result<(), CmodError> {
        if !self.can_write() {
            return Err(CmodError::Other("remote cache is read-only".to_string()));
        }

        let url = self.cache_url(module_id, key, Some(artifact_name));

        let status = std::process::Command::new("curl")
            .args([
                "-s",
                "-o",
                "/dev/null",
                "-w",
                "%{http_code}",
                "-X",
                "PUT",
                "--data-binary",
                &format!("@{}", source.display()),
                &url,
            ])
            .output()
            .map_err(|e| CmodError::Other(format!("curl not available: {}", e)))?;

        let code = String::from_utf8_lossy(&status.stdout);
        let code = code.trim();
        if code.starts_with('2') {
            Ok(())
        } else {
            Err(CmodError::Other(format!(
                "remote cache upload failed with HTTP {}",
                code
            )))
        }
    }

    fn name(&self) -> &str {
        "http"
    }
}

/// Configuration for remote cache (parsed from manifest `[cache]` section).
#[derive(Debug, Clone)]
pub struct RemoteCacheConfig {
    pub url: String,
    pub mode: RemoteCacheMode,
}

impl RemoteCacheConfig {
    /// Create a remote cache client from this config.
    pub fn into_client(self) -> HttpRemoteCache {
        HttpRemoteCache::new(&self.url, self.mode)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_remote_cache_mode_from_str() {
        assert_eq!(
            RemoteCacheMode::from_str("readonly"),
            RemoteCacheMode::ReadOnly
        );
        assert_eq!(
            RemoteCacheMode::from_str("read-only"),
            RemoteCacheMode::ReadOnly
        );
        assert_eq!(RemoteCacheMode::from_str("ro"), RemoteCacheMode::ReadOnly);
        assert_eq!(
            RemoteCacheMode::from_str("readwrite"),
            RemoteCacheMode::ReadWrite
        );
        assert_eq!(
            RemoteCacheMode::from_str("read-write"),
            RemoteCacheMode::ReadWrite
        );
        assert_eq!(RemoteCacheMode::from_str("rw"), RemoteCacheMode::ReadWrite);
        assert_eq!(RemoteCacheMode::from_str("off"), RemoteCacheMode::Off);
        assert_eq!(RemoteCacheMode::from_str("anything"), RemoteCacheMode::Off);
    }

    #[test]
    fn test_http_cache_url_construction() {
        let cache = HttpRemoteCache::new("https://cache.example.com/", RemoteCacheMode::ReadWrite);
        let key = CacheKey::from_hex("abc123").unwrap();

        let url = cache.cache_url("mymod", &key, None);
        assert_eq!(url, "https://cache.example.com/cache/mymod/abc123");

        let url = cache.cache_url("mymod", &key, Some("mymod.pcm"));
        assert_eq!(
            url,
            "https://cache.example.com/cache/mymod/abc123/mymod.pcm"
        );
    }

    #[test]
    fn test_http_cache_can_read_write() {
        let rw = HttpRemoteCache::new("https://cache.example.com", RemoteCacheMode::ReadWrite);
        assert!(rw.can_read());
        assert!(rw.can_write());

        let ro = HttpRemoteCache::new("https://cache.example.com", RemoteCacheMode::ReadOnly);
        assert!(ro.can_read());
        assert!(!ro.can_write());

        let off = HttpRemoteCache::new("https://cache.example.com", RemoteCacheMode::Off);
        assert!(!off.can_read());
        assert!(!off.can_write());
    }

    #[test]
    fn test_has_returns_false_when_off() {
        let cache = HttpRemoteCache::new("https://cache.example.com", RemoteCacheMode::Off);
        let key = CacheKey::from_hex("abc123").unwrap();
        assert!(!cache.has("mymod", &key).unwrap());
    }

    #[test]
    fn test_get_returns_false_when_off() {
        let cache = HttpRemoteCache::new("https://cache.example.com", RemoteCacheMode::Off);
        let key = CacheKey::from_hex("abc123").unwrap();
        let result = cache
            .get("mymod", &key, "test.pcm", Path::new("/tmp/test"))
            .unwrap();
        assert!(!result);
    }

    #[test]
    fn test_put_fails_when_readonly() {
        let cache = HttpRemoteCache::new("https://cache.example.com", RemoteCacheMode::ReadOnly);
        let key = CacheKey::from_hex("abc123").unwrap();
        let result = cache.put("mymod", &key, "test.pcm", Path::new("/tmp/test"));
        assert!(result.is_err());
    }

    #[test]
    fn test_remote_cache_config() {
        let config = RemoteCacheConfig {
            url: "https://cache.example.com".to_string(),
            mode: RemoteCacheMode::ReadWrite,
        };
        let client = config.into_client();
        assert_eq!(client.name(), "http");
        assert!(client.can_write());
    }
}
