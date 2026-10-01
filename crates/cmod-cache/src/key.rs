use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fmt;

/// A content-addressed cache key computed from deterministic build inputs.
///
/// Cache key = hash(source content, dependency hashes, compiler, flags, target, stdlib).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CacheKey(pub String);

impl CacheKey {
    /// Build a cache key from component inputs.
    pub fn compute(inputs: &CacheKeyInputs) -> Self {
        let mut hasher = Sha256::new();

        hasher.update(inputs.source_hash.as_bytes());

        for dep_hash in &inputs.dependency_hashes {
            hasher.update(dep_hash.as_bytes());
        }

        hasher.update(inputs.compiler.as_bytes());
        hasher.update(inputs.compiler_version.as_bytes());
        hasher.update(inputs.cxx_standard.as_bytes());
        hasher.update(inputs.stdlib.as_bytes());
        hasher.update(inputs.target.as_bytes());

        for flag in &inputs.flags {
            hasher.update(flag.as_bytes());
        }

        let result = hasher.finalize();
        CacheKey(hex::encode(result))
    }

    /// The key that holds the include manifest for sources with this key:
    /// the header sets seen when compiling them (see [`IncludeManifest`]).
    ///
    /// [`IncludeManifest`]: crate::cache::IncludeManifest
    pub fn include_manifest_key(&self) -> CacheKey {
        let mut hasher = Sha256::new();
        hasher.update(b"cmod-include-manifest-v1\0");
        hasher.update(self.0.as_bytes());
        CacheKey(hex::encode(hasher.finalize()))
    }

    /// The key for artifacts compiled from this key's inputs while
    /// including exactly `headers`. Artifacts are stored and restored under
    /// this key, never under `self`, so a cached object is only reused when
    /// every header it read has the same content.
    pub fn with_headers(&self, headers: &[HeaderDigest]) -> CacheKey {
        let mut sorted: Vec<&HeaderDigest> = headers.iter().collect();
        sorted.sort_by(|a, b| a.path.cmp(&b.path));

        let mut hasher = Sha256::new();
        hasher.update(b"cmod-headers-v1\0");
        hasher.update(self.0.as_bytes());
        for header in sorted {
            hasher.update(b"\0");
            hasher.update(header.path.as_bytes());
            hasher.update(b"\0");
            hasher.update(header.hash.as_bytes());
        }
        CacheKey(hex::encode(hasher.finalize()))
    }

    /// Create a CacheKey from a hex string (for tests or lookups).
    pub fn from_hex(hex_str: &str) -> Option<Self> {
        if hex_str.is_empty() {
            return None;
        }
        Some(CacheKey(hex_str.to_string()))
    }
}

impl fmt::Display for CacheKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// One header a translation unit included.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeaderDigest {
    /// Relative to the package root when the header is inside it (so
    /// checkouts in different places share entries), absolute otherwise.
    pub path: String,
    /// SHA-256 of the header's content.
    pub hash: String,
}

/// Inputs for computing a deterministic cache key.
pub struct CacheKeyInputs {
    /// Hash of the source file content.
    pub source_hash: String,
    /// Hashes of all direct dependency artifacts (transitive).
    pub dependency_hashes: Vec<String>,
    /// Compiler name (e.g., "clang").
    pub compiler: String,
    /// Compiler version (e.g., "18.1.0").
    pub compiler_version: String,
    /// C++ standard (e.g., "20").
    pub cxx_standard: String,
    /// Standard library (e.g., "libc++").
    pub stdlib: String,
    /// Target triple (e.g., "x86_64-unknown-linux-gnu").
    pub target: String,
    /// Additional compilation flags.
    pub flags: Vec<String>,
}

/// Compute a SHA-256 hash of file content bytes.
pub fn hash_bytes(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

/// Compute a SHA-256 hash of a file on disk.
pub fn hash_file(path: &std::path::Path) -> Result<String, std::io::Error> {
    let data = std::fs::read(path)?;
    Ok(hash_bytes(&data))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_key_deterministic() {
        let inputs = CacheKeyInputs {
            source_hash: "abc123".to_string(),
            dependency_hashes: vec!["dep1hash".to_string()],
            compiler: "clang".to_string(),
            compiler_version: "18.1.0".to_string(),
            cxx_standard: "20".to_string(),
            stdlib: "libc++".to_string(),
            target: "x86_64-unknown-linux-gnu".to_string(),
            flags: vec!["-O2".to_string()],
        };

        let key1 = CacheKey::compute(&inputs);
        let key2 = CacheKey::compute(&inputs);
        assert_eq!(key1, key2);
    }

    #[test]
    fn test_cache_key_differs_on_input_change() {
        let inputs1 = CacheKeyInputs {
            source_hash: "abc123".to_string(),
            dependency_hashes: vec![],
            compiler: "clang".to_string(),
            compiler_version: "18.1.0".to_string(),
            cxx_standard: "20".to_string(),
            stdlib: "libc++".to_string(),
            target: "x86_64-unknown-linux-gnu".to_string(),
            flags: vec![],
        };

        let inputs2 = CacheKeyInputs {
            source_hash: "def456".to_string(),
            dependency_hashes: vec![],
            compiler: "clang".to_string(),
            compiler_version: "18.1.0".to_string(),
            cxx_standard: "20".to_string(),
            stdlib: "libc++".to_string(),
            target: "x86_64-unknown-linux-gnu".to_string(),
            flags: vec![],
        };

        let key1 = CacheKey::compute(&inputs1);
        let key2 = CacheKey::compute(&inputs2);
        assert_ne!(key1, key2);
    }

    #[test]
    fn test_hash_bytes() {
        let h1 = hash_bytes(b"hello");
        let h2 = hash_bytes(b"hello");
        assert_eq!(h1, h2);

        let h3 = hash_bytes(b"world");
        assert_ne!(h1, h3);
    }

    #[test]
    fn test_cache_key_display() {
        let inputs = CacheKeyInputs {
            source_hash: "test".to_string(),
            dependency_hashes: vec![],
            compiler: "clang".to_string(),
            compiler_version: "18".to_string(),
            cxx_standard: "20".to_string(),
            stdlib: "libc++".to_string(),
            target: "x86_64".to_string(),
            flags: vec![],
        };
        let key = CacheKey::compute(&inputs);
        let display = format!("{}", key);
        assert_eq!(display, key.0);
        assert_eq!(display.len(), 64); // SHA-256 hex is 64 chars
    }

    #[test]
    fn test_cache_key_differs_on_compiler_change() {
        let base = CacheKeyInputs {
            source_hash: "same".to_string(),
            dependency_hashes: vec![],
            compiler: "clang".to_string(),
            compiler_version: "18".to_string(),
            cxx_standard: "20".to_string(),
            stdlib: "libc++".to_string(),
            target: "x86_64".to_string(),
            flags: vec![],
        };
        let different = CacheKeyInputs {
            source_hash: "same".to_string(),
            dependency_hashes: vec![],
            compiler: "gcc".to_string(),
            compiler_version: "18".to_string(),
            cxx_standard: "20".to_string(),
            stdlib: "libc++".to_string(),
            target: "x86_64".to_string(),
            flags: vec![],
        };

        assert_ne!(CacheKey::compute(&base), CacheKey::compute(&different));
    }

    #[test]
    fn test_cache_key_differs_on_target_change() {
        let key_linux = CacheKey::compute(&CacheKeyInputs {
            source_hash: "s".to_string(),
            dependency_hashes: vec![],
            compiler: "clang".to_string(),
            compiler_version: "18".to_string(),
            cxx_standard: "20".to_string(),
            stdlib: "libc++".to_string(),
            target: "x86_64-unknown-linux-gnu".to_string(),
            flags: vec![],
        });
        let key_darwin = CacheKey::compute(&CacheKeyInputs {
            source_hash: "s".to_string(),
            dependency_hashes: vec![],
            compiler: "clang".to_string(),
            compiler_version: "18".to_string(),
            cxx_standard: "20".to_string(),
            stdlib: "libc++".to_string(),
            target: "arm64-apple-darwin".to_string(),
            flags: vec![],
        });

        assert_ne!(key_linux, key_darwin);
    }

    #[test]
    fn test_cache_key_differs_on_flags() {
        let key_no_flags = CacheKey::compute(&CacheKeyInputs {
            source_hash: "s".to_string(),
            dependency_hashes: vec![],
            compiler: "clang".to_string(),
            compiler_version: "18".to_string(),
            cxx_standard: "20".to_string(),
            stdlib: "libc++".to_string(),
            target: "x86_64".to_string(),
            flags: vec![],
        });
        let key_with_flags = CacheKey::compute(&CacheKeyInputs {
            source_hash: "s".to_string(),
            dependency_hashes: vec![],
            compiler: "clang".to_string(),
            compiler_version: "18".to_string(),
            cxx_standard: "20".to_string(),
            stdlib: "libc++".to_string(),
            target: "x86_64".to_string(),
            flags: vec!["-fsanitize=address".to_string()],
        });

        assert_ne!(key_no_flags, key_with_flags);
    }

    fn digest(path: &str, hash: &str) -> HeaderDigest {
        HeaderDigest {
            path: path.to_string(),
            hash: hash.to_string(),
        }
    }

    #[test]
    fn test_with_headers_depends_on_header_content() {
        let base = CacheKey("base".to_string());
        let v1 = base.with_headers(&[digest("include/value.h", "h1")]);
        let v2 = base.with_headers(&[digest("include/value.h", "h2")]);
        assert_ne!(v1, v2);
        assert_ne!(v1, base);
        assert_ne!(base.with_headers(&[]), base);
    }

    #[test]
    fn test_with_headers_ignores_order_but_not_paths() {
        let base = CacheKey("base".to_string());
        let ab = base.with_headers(&[digest("a.h", "1"), digest("b.h", "2")]);
        let ba = base.with_headers(&[digest("b.h", "2"), digest("a.h", "1")]);
        assert_eq!(ab, ba);

        // Same contents under swapped names is a different translation unit.
        let swapped = base.with_headers(&[digest("a.h", "2"), digest("b.h", "1")]);
        assert_ne!(ab, swapped);
    }

    #[test]
    fn test_include_manifest_key_is_distinct() {
        let base = CacheKey("base".to_string());
        assert_ne!(base.include_manifest_key(), base);
        assert_ne!(base.include_manifest_key(), base.with_headers(&[]));
        assert_eq!(base.include_manifest_key().0.len(), 64);
    }

    #[test]
    fn test_hash_file() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("test.txt");
        std::fs::write(&path, b"test content").unwrap();

        let h1 = hash_file(&path).unwrap();
        let h2 = hash_file(&path).unwrap();
        assert_eq!(h1, h2);

        // Should match hash_bytes with same content
        let h3 = hash_bytes(b"test content");
        assert_eq!(h1, h3);
    }

    #[test]
    fn test_hash_bytes_empty() {
        let h = hash_bytes(b"");
        // SHA-256 of empty string is well-known
        assert_eq!(
            h,
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }
}
