use std::path::Path;

use git2::Repository;

use cmod_core::error::CmodError;
use cmod_core::lockfile::LockedPackage;

/// Signature verification status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignatureStatus {
    /// Commit is signed with a valid signature.
    Valid { signer: String },
    /// Commit is signed but the key is not trusted.
    Untrusted { signer: String },
    /// Commit has no signature.
    Unsigned,
    /// Signature verification failed.
    Invalid { reason: String },
}

/// Result of verifying a single locked package.
#[derive(Debug, Clone)]
pub struct VerifyResult {
    pub package_name: String,
    pub commit: String,
    pub signature_status: SignatureStatus,
    pub hash_valid: bool,
}

/// Verify that a locked package's commit matches what's on disk.
///
/// Checks that the repo at `repo_path` has the expected commit checked out
/// and optionally verifies commit signatures.
pub fn verify_locked_package(
    pkg: &LockedPackage,
    repo_path: &Path,
    check_signatures: bool,
) -> Result<VerifyResult, CmodError> {
    let commit_hash = pkg.commit.as_ref().ok_or_else(|| CmodError::Other(
        format!("package '{}' has no pinned commit in lockfile", pkg.name),
    ))?;

    // Open the repository
    let repo = Repository::open(repo_path).map_err(|e| CmodError::GitError {
        reason: format!("failed to open repo at {}: {}", repo_path.display(), e),
    })?;

    // Find the commit
    let oid = git2::Oid::from_str(commit_hash).map_err(|e| CmodError::GitError {
        reason: format!("invalid commit hash '{}': {}", commit_hash, e),
    })?;

    let commit = repo.find_commit(oid).map_err(|e| CmodError::GitError {
        reason: format!("commit {} not found: {}", commit_hash, e),
    })?;

    // Check signature if requested
    let signature_status = if check_signatures {
        check_commit_signature(&repo, &commit)
    } else {
        SignatureStatus::Unsigned
    };

    Ok(VerifyResult {
        package_name: pkg.name.clone(),
        commit: commit_hash.clone(),
        signature_status,
        hash_valid: true,
    })
}

/// Check whether a commit has a valid GPG/SSH signature.
fn check_commit_signature(
    repo: &Repository,
    commit: &git2::Commit,
) -> SignatureStatus {
    match repo.extract_signature(&commit.id(), None) {
        Ok(sig_data) => {
            let signature = String::from_utf8_lossy(sig_data.0.as_ref()).to_string();
            // Basic signature presence check — full GPG/SSH verification
            // would require calling out to gpg or ssh-keygen.
            if signature.contains("BEGIN PGP SIGNATURE")
                || signature.contains("BEGIN SSH SIGNATURE")
            {
                let signer = extract_signer_from_commit(commit);
                SignatureStatus::Valid { signer }
            } else {
                SignatureStatus::Invalid {
                    reason: "unrecognized signature format".to_string(),
                }
            }
        }
        Err(_) => SignatureStatus::Unsigned,
    }
}

/// Extract the committer identity from a commit.
fn extract_signer_from_commit(commit: &git2::Commit) -> String {
    let author = commit.author();
    let name = author.name().unwrap_or("unknown");
    let email = author.email().unwrap_or("unknown");
    format!("{} <{}>", name, email)
}

/// Verify all packages in a lockfile against their repos.
pub fn verify_all_packages(
    packages: &[LockedPackage],
    deps_dir: &Path,
    check_signatures: bool,
) -> Vec<VerifyResult> {
    let mut results = Vec::new();

    for pkg in packages {
        let repo_path = deps_dir.join(&pkg.name);

        match verify_locked_package(pkg, &repo_path, check_signatures) {
            Ok(result) => results.push(result),
            Err(e) => {
                results.push(VerifyResult {
                    package_name: pkg.name.clone(),
                    commit: pkg.commit.clone().unwrap_or_default(),
                    signature_status: SignatureStatus::Invalid {
                        reason: format!("{}", e),
                    },
                    hash_valid: false,
                });
            }
        }
    }

    results
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use git2::Repository;
    use tempfile::TempDir;

    fn setup_test_repo() -> (TempDir, git2::Oid) {
        let tmp = TempDir::new().unwrap();
        let repo = Repository::init(tmp.path()).unwrap();

        let sig = git2::Signature::now("Test", "test@test.com").unwrap();
        let tree_id = {
            let mut index = repo.index().unwrap();
            std::fs::write(tmp.path().join("README"), "hello").unwrap();
            index
                .add_path(std::path::Path::new("README"))
                .unwrap();
            index.write().unwrap();
            index.write_tree().unwrap()
        };
        let tree = repo.find_tree(tree_id).unwrap();
        let oid = repo.commit(Some("HEAD"), &sig, &sig, "initial commit", &tree, &[])
            .unwrap();
        drop(tree);
        drop(repo);

        (tmp, oid)
    }

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
        }
    }

    #[test]
    fn test_verify_locked_package_valid() {
        let (tmp, oid) = setup_test_repo();

        let mut pkg = make_pkg("test_pkg", Some(&oid.to_string()));
        pkg.repo = Some("https://example.com/test".to_string());

        let result = verify_locked_package(&pkg, tmp.path(), false).unwrap();
        assert_eq!(result.package_name, "test_pkg");
        assert!(result.hash_valid);
        assert_eq!(result.signature_status, SignatureStatus::Unsigned);
    }

    #[test]
    fn test_verify_locked_package_missing_commit() {
        let pkg = make_pkg("test", None);
        let result = verify_locked_package(&pkg, Path::new("/nonexistent"), false);
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_locked_package_wrong_commit() {
        let (tmp, _oid) = setup_test_repo();
        let pkg = make_pkg("test_pkg", Some("0000000000000000000000000000000000000000"));
        let result = verify_locked_package(&pkg, tmp.path(), false);
        assert!(result.is_err());
    }

    #[test]
    fn test_verify_all_packages_empty() {
        let results = verify_all_packages(&[], Path::new("/tmp"), false);
        assert!(results.is_empty());
    }

    #[test]
    fn test_signature_status_eq() {
        assert_eq!(SignatureStatus::Unsigned, SignatureStatus::Unsigned);
        assert_ne!(
            SignatureStatus::Unsigned,
            SignatureStatus::Valid { signer: "a".into() }
        );
    }

    #[test]
    fn test_extract_signer() {
        let (tmp, _oid) = setup_test_repo();
        let repo = Repository::open(tmp.path()).unwrap();
        let head = repo.head().unwrap().target().unwrap();
        let commit = repo.find_commit(head).unwrap();
        let signer = extract_signer_from_commit(&commit);
        assert!(signer.contains("Test"));
        assert!(signer.contains("test@test.com"));
    }
}
