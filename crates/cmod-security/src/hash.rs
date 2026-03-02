use std::path::Path;

use sha2::{Digest, Sha256};

use cmod_core::error::CmodError;
use cmod_core::lockfile::{LockedPackage, Lockfile};

/// Result of verifying a lockfile entry's content hash.
#[derive(Debug, Clone)]
pub struct HashVerifyResult {
    pub package_name: String,
    pub expected_commit: String,
    pub actual_commit: Option<String>,
    pub valid: bool,
}

/// Compute the integrity hash for a lockfile.
///
/// This produces a single hash representing the full lockfile contents,
/// used to detect any tampering or unintended modifications.
pub fn lockfile_integrity_hash(lockfile: &Lockfile) -> String {
    let mut hasher = Sha256::new();
    hasher.update(format!("version:{}", lockfile.version).as_bytes());

    for pkg in &lockfile.packages {
        hasher.update(pkg.name.as_bytes());
        hasher.update(pkg.version.as_bytes());
        if let Some(ref repo) = pkg.repo {
            hasher.update(repo.as_bytes());
        }
        if let Some(ref commit) = pkg.commit {
            hasher.update(commit.as_bytes());
        }
        for dep in &pkg.deps {
            hasher.update(dep.as_bytes());
        }
    }

    hex::encode(hasher.finalize())
}

/// Verify that a dependency checkout matches the commit in the lockfile.
///
/// Opens the git repo at `repo_path` and checks that HEAD matches
/// the expected commit hash.
pub fn verify_checkout_hash(
    pkg: &LockedPackage,
    repo_path: &Path,
) -> Result<HashVerifyResult, CmodError> {
    let expected = pkg
        .commit
        .as_ref()
        .ok_or_else(|| CmodError::Other(format!("package '{}' has no commit hash", pkg.name)))?;

    let repo = git2::Repository::open(repo_path).map_err(|e| CmodError::GitError {
        reason: format!("failed to open {}: {}", repo_path.display(), e),
    })?;

    let head = repo.head().map_err(|e| CmodError::GitError {
        reason: format!("no HEAD in {}: {}", repo_path.display(), e),
    })?;

    let actual = head.target().map(|oid| oid.to_string());

    let valid = actual.as_deref() == Some(expected.as_str());

    Ok(HashVerifyResult {
        package_name: pkg.name.clone(),
        expected_commit: expected.clone(),
        actual_commit: actual,
        valid,
    })
}

/// Hash the contents of a directory tree for integrity checking.
pub fn hash_directory(path: &Path) -> Result<String, CmodError> {
    let mut hasher = Sha256::new();

    if !path.exists() {
        return Err(CmodError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("directory not found: {}", path.display()),
        )));
    }

    let mut entries: Vec<_> = walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .collect();

    // Sort for deterministic ordering
    entries.sort_by(|a, b| a.path().cmp(b.path()));

    for entry in entries {
        // Include relative path in hash for structural integrity
        let relative = entry.path().strip_prefix(path).unwrap_or(entry.path());
        hasher.update(relative.to_string_lossy().as_bytes());

        let content = std::fs::read(entry.path())?;
        hasher.update(&content);
    }

    Ok(hex::encode(hasher.finalize()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use tempfile::TempDir;

    fn make_pkg(name: &str, commit: Option<&str>) -> LockedPackage {
        LockedPackage {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            source: None,
            repo: None,
            commit: commit.map(|s| s.to_string()),
            hash: None,
            toolchain: None,
            targets: BTreeMap::new(),
            deps: vec![],
            features: vec![],
        }
    }

    #[test]
    fn test_lockfile_integrity_hash_deterministic() {
        let mut pkg = make_pkg("fmt", Some("abc123"));
        pkg.version = "10.2.0".to_string();
        pkg.repo = Some("https://github.com/fmtlib/fmt".to_string());

        let lockfile = Lockfile {
            version: 1,
            integrity: None,
            packages: vec![pkg],
        };

        let h1 = lockfile_integrity_hash(&lockfile);
        let h2 = lockfile_integrity_hash(&lockfile);
        assert_eq!(h1, h2);
        assert_eq!(h1.len(), 64);
    }

    #[test]
    fn test_lockfile_integrity_hash_changes_on_diff() {
        let l1 = Lockfile {
            version: 1,
            integrity: None,
            packages: vec![make_pkg("a", Some("aaa"))],
        };

        let l2 = Lockfile {
            version: 1,
            integrity: None,
            packages: vec![make_pkg("a", Some("bbb"))],
        };

        assert_ne!(lockfile_integrity_hash(&l1), lockfile_integrity_hash(&l2));
    }

    #[test]
    fn test_hash_directory() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("a.txt"), "hello").unwrap();
        std::fs::write(tmp.path().join("b.txt"), "world").unwrap();

        let h1 = hash_directory(tmp.path()).unwrap();
        let h2 = hash_directory(tmp.path()).unwrap();
        assert_eq!(h1, h2);
    }

    #[test]
    fn test_hash_directory_changes_on_content() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(tmp.path().join("a.txt"), "v1").unwrap();
        let h1 = hash_directory(tmp.path()).unwrap();

        std::fs::write(tmp.path().join("a.txt"), "v2").unwrap();
        let h2 = hash_directory(tmp.path()).unwrap();

        assert_ne!(h1, h2);
    }

    #[test]
    fn test_hash_directory_not_found() {
        let result = hash_directory(Path::new("/nonexistent/path"));
        assert!(result.is_err());
    }

    #[test]
    fn test_lockfile_integrity_hash_empty() {
        let lockfile = Lockfile {
            version: 1,
            integrity: None,
            packages: vec![],
        };
        let h = lockfile_integrity_hash(&lockfile);
        assert_eq!(h.len(), 64);
    }

    #[test]
    fn test_verify_checkout_hash_missing_commit() {
        let pkg = make_pkg("test", None);
        let result = verify_checkout_hash(&pkg, Path::new("/tmp"));
        assert!(result.is_err());
    }
}
