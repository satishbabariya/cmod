//! Remote/shared cache protocol for distributing build artifacts.
//!
//! Provides a trait-based abstraction for remote caches and an HTTP implementation
//! following a simple REST protocol:
//!
//! - `HEAD /cache/<module_id>/<key>` — check existence
//! - `GET  /cache/<module_id>/<key>/<artifact>` — download artifact
//! - `PUT  /cache/<module_id>/<key>/<artifact>` — upload artifact

use std::path::Path;
use std::time::Duration;

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
    #[allow(clippy::should_implement_trait)]
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

/// HTTP-based remote cache implementation using native HTTP (`ureq`).
///
/// Features:
/// - `Authorization: Bearer <token>` support
/// - Configurable timeout (default 30s)
/// - Retry with exponential backoff (default 3 attempts)
pub struct HttpRemoteCache {
    /// Base URL of the cache server (e.g., `https://cache.example.com`).
    base_url: String,
    /// Access mode.
    mode: RemoteCacheMode,
    /// Optional bearer token for authentication.
    auth_token: Option<String>,
    /// HTTP timeout per request.
    timeout: Duration,
    /// Number of retry attempts.
    max_retries: u32,
}

impl HttpRemoteCache {
    /// Create a new HTTP remote cache client.
    pub fn new(base_url: &str, mode: RemoteCacheMode) -> Self {
        let base_url = base_url.trim_end_matches('/').to_string();
        HttpRemoteCache {
            base_url,
            mode,
            auth_token: None,
            timeout: Duration::from_secs(30),
            max_retries: 3,
        }
    }

    /// Set the bearer token for authentication.
    pub fn with_auth_token(mut self, token: Option<String>) -> Self {
        self.auth_token = token;
        self
    }

    /// Set the HTTP timeout.
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Set the maximum number of retry attempts.
    pub fn with_retries(mut self, retries: u32) -> Self {
        self.max_retries = retries;
        self
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

    /// Execute a request with retry and exponential backoff.
    fn with_retry<F, T>(&self, operation: &str, f: F) -> Result<T, CmodError>
    where
        F: Fn() -> Result<T, CmodError>,
    {
        let mut last_err = None;
        for attempt in 0..self.max_retries {
            match f() {
                Ok(result) => return Ok(result),
                Err(e) => {
                    last_err = Some(e);
                    if attempt + 1 < self.max_retries {
                        // Exponential backoff: 100ms, 200ms, 400ms, ...
                        let delay = Duration::from_millis(100 * (1 << attempt));
                        std::thread::sleep(delay);
                    }
                }
            }
        }
        Err(last_err.unwrap_or_else(|| {
            CmodError::Other(format!(
                "remote cache {} failed after {} retries",
                operation, self.max_retries
            ))
        }))
    }

    /// Build a ureq agent configured to NOT treat non-2xx as errors,
    /// so we can handle 404/etc gracefully.
    fn agent(&self) -> ureq::Agent {
        ureq::Agent::new_with_config(
            ureq::config::Config::builder()
                .timeout_global(Some(self.timeout))
                .http_status_as_error(false)
                .build(),
        )
    }
}

impl RemoteCache for HttpRemoteCache {
    fn has(&self, module_id: &str, key: &CacheKey) -> Result<bool, CmodError> {
        if !self.can_read() {
            return Ok(false);
        }

        let url = self.cache_url(module_id, key, None);

        self.with_retry("HEAD", || {
            let agent = self.agent();
            let mut req = agent.head(&url);
            if let Some(ref token) = self.auth_token {
                req = req.header("Authorization", &format!("Bearer {}", token));
            }
            match req.call() {
                Ok(resp) => Ok(resp.status().as_u16() == 200),
                Err(e) => Err(CmodError::Other(format!("remote cache HEAD failed: {}", e))),
            }
        })
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

        self.with_retry("GET", || {
            let agent = self.agent();
            let mut req = agent.get(&url);
            if let Some(ref token) = self.auth_token {
                req = req.header("Authorization", &format!("Bearer {}", token));
            }
            match req.call() {
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    if status == 200 {
                        let data = resp.into_body().read_to_vec().map_err(|e| {
                            CmodError::Other(format!("failed to read response body: {}", e))
                        })?;

                        if data.is_empty() {
                            return Ok(false);
                        }

                        std::fs::write(dest, &data)?;
                        Ok(true)
                    } else if status == 404 {
                        Ok(false)
                    } else if status >= 500 {
                        Err(CmodError::Other(format!(
                            "remote cache GET returned server error {}",
                            status
                        )))
                    } else {
                        Ok(false)
                    }
                }
                Err(e) => Err(CmodError::Other(format!("remote cache GET failed: {}", e))),
            }
        })
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
        let body = std::fs::read(source)?;

        self.with_retry("PUT", || {
            let agent = self.agent();
            let mut req = agent.put(&url);
            if let Some(ref token) = self.auth_token {
                req = req.header("Authorization", &format!("Bearer {}", token));
            }
            req = req.header("Content-Type", "application/octet-stream");
            match req.send(&body[..]) {
                Ok(resp) => {
                    let status = resp.status().as_u16();
                    if (200..300).contains(&status) {
                        Ok(())
                    } else {
                        Err(CmodError::Other(format!(
                            "remote cache upload failed with HTTP {}",
                            status
                        )))
                    }
                }
                Err(e) => Err(CmodError::Other(format!("remote cache PUT failed: {}", e))),
            }
        })
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
    pub auth_token: Option<String>,
    pub timeout_secs: u64,
    pub retries: u32,
}

impl RemoteCacheConfig {
    /// Create a remote cache client from this config.
    pub fn into_client(self) -> HttpRemoteCache {
        HttpRemoteCache::new(&self.url, self.mode)
            .with_auth_token(self.auth_token)
            .with_timeout(Duration::from_secs(self.timeout_secs))
            .with_retries(self.retries)
    }
}

impl Default for RemoteCacheConfig {
    fn default() -> Self {
        RemoteCacheConfig {
            url: String::new(),
            mode: RemoteCacheMode::Off,
            auth_token: None,
            timeout_secs: 30,
            retries: 3,
        }
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
            auth_token: Some("my-token".to_string()),
            timeout_secs: 60,
            retries: 5,
        };
        let client = config.into_client();
        assert_eq!(client.name(), "http");
        assert!(client.can_write());
        assert_eq!(client.timeout, Duration::from_secs(60));
        assert_eq!(client.max_retries, 5);
        assert_eq!(client.auth_token.as_deref(), Some("my-token"));
    }

    #[test]
    fn test_builder_methods() {
        let cache = HttpRemoteCache::new("https://cache.example.com", RemoteCacheMode::ReadWrite)
            .with_auth_token(Some("token123".to_string()))
            .with_timeout(Duration::from_secs(60))
            .with_retries(5);

        assert_eq!(cache.auth_token.as_deref(), Some("token123"));
        assert_eq!(cache.timeout, Duration::from_secs(60));
        assert_eq!(cache.max_retries, 5);
    }

    #[test]
    fn test_default_config() {
        let config = RemoteCacheConfig::default();
        assert_eq!(config.timeout_secs, 30);
        assert_eq!(config.retries, 3);
        assert_eq!(config.mode, RemoteCacheMode::Off);
        assert!(config.auth_token.is_none());
    }
}
