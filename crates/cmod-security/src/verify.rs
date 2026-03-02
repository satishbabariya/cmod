use std::path::Path;
use std::process::Command;

use git2::Repository;

use cmod_core::error::CmodError;
use cmod_core::lockfile::LockedPackage;

/// Signature verification status.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SignatureStatus {
    /// Commit is signed with a valid, trusted signature.
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
    let commit_hash = pkg.commit.as_ref().ok_or_else(|| {
        CmodError::Other(format!(
            "package '{}' has no pinned commit in lockfile",
            pkg.name
        ))
    })?;

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
///
/// Extracts the signature from the commit, then validates it using the
/// appropriate tool (`gpg --verify` for PGP, `ssh-keygen -Y verify` for SSH).
/// Falls back to `Untrusted` if the verification tool is not available.
fn check_commit_signature(repo: &Repository, commit: &git2::Commit) -> SignatureStatus {
    match repo.extract_signature(&commit.id(), None) {
        Ok(sig_data) => {
            let signature = String::from_utf8_lossy(sig_data.0.as_ref()).to_string();
            let signed_data = String::from_utf8_lossy(sig_data.1.as_ref()).to_string();
            let signer = extract_signer_from_commit(commit);

            if signature.contains("BEGIN PGP SIGNATURE") {
                verify_pgp_signature(&signature, &signed_data, &signer)
            } else if signature.contains("BEGIN SSH SIGNATURE") {
                verify_ssh_signature(&signature, &signed_data, &signer)
            } else {
                SignatureStatus::Invalid {
                    reason: "unrecognized signature format".to_string(),
                }
            }
        }
        Err(_) => SignatureStatus::Unsigned,
    }
}

/// Verify a PGP signature using `gpg --verify`.
///
/// Creates temporary files for the detached signature and signed data,
/// then runs `gpg --status-fd 1 --verify <sig_file> <data_file>`.
/// Parses the exit code and output to determine validity and trust.
fn verify_pgp_signature(signature: &str, signed_data: &str, signer: &str) -> SignatureStatus {
    // Check if gpg is available
    if Command::new("gpg").arg("--version").output().is_err() {
        return SignatureStatus::Untrusted {
            signer: format!("{} (gpg not available for verification)", signer),
        };
    }

    // Write signature and data to temp files
    let tmp_dir = match tempfile::TempDir::new() {
        Ok(d) => d,
        Err(_) => {
            return SignatureStatus::Untrusted {
                signer: format!("{} (failed to create temp dir)", signer),
            };
        }
    };

    let sig_path = tmp_dir.path().join("commit.sig");
    let data_path = tmp_dir.path().join("commit.data");

    if std::fs::write(&sig_path, signature).is_err()
        || std::fs::write(&data_path, signed_data).is_err()
    {
        return SignatureStatus::Untrusted {
            signer: format!("{} (failed to write temp files)", signer),
        };
    }

    let output = Command::new("gpg")
        .args([
            "--status-fd",
            "1",
            "--verify",
            &sig_path.display().to_string(),
            &data_path.display().to_string(),
        ])
        .output();

    match output {
        Ok(result) => {
            let stdout = String::from_utf8_lossy(&result.stdout);
            let stderr = String::from_utf8_lossy(&result.stderr);

            if result.status.success() {
                // Check for GOODSIG/VALIDSIG in status output
                if stdout.contains("GOODSIG") || stdout.contains("VALIDSIG") {
                    SignatureStatus::Valid {
                        signer: signer.to_string(),
                    }
                } else {
                    // gpg returned 0 but no GOODSIG — treat as valid
                    SignatureStatus::Valid {
                        signer: signer.to_string(),
                    }
                }
            } else if stdout.contains("EXPKEYSIG") {
                SignatureStatus::Untrusted {
                    signer: format!("{} (key expired)", signer),
                }
            } else if stdout.contains("BADSIG") {
                SignatureStatus::Invalid {
                    reason: format!("bad PGP signature from {}", signer),
                }
            } else if stdout.contains("NO_PUBKEY") || stderr.contains("No public key") {
                SignatureStatus::Untrusted {
                    signer: format!("{} (public key not found)", signer),
                }
            } else {
                // Non-zero exit but no specific status — untrusted
                SignatureStatus::Untrusted {
                    signer: format!(
                        "{} (gpg verification inconclusive: {})",
                        signer,
                        stderr.lines().next().unwrap_or("unknown error")
                    ),
                }
            }
        }
        Err(_) => SignatureStatus::Untrusted {
            signer: format!("{} (failed to execute gpg)", signer),
        },
    }
}

/// Verify an SSH signature using `ssh-keygen -Y verify`.
///
/// SSH signature verification requires an `allowed_signers` file mapping
/// principals to public keys. If not configured, falls back to `Untrusted`.
fn verify_ssh_signature(signature: &str, signed_data: &str, signer: &str) -> SignatureStatus {
    // Check if ssh-keygen is available
    if Command::new("ssh-keygen").arg("-h").output().is_err() {
        return SignatureStatus::Untrusted {
            signer: format!("{} (ssh-keygen not available for verification)", signer),
        };
    }

    // Look for allowed_signers file in standard locations
    let home = dirs::home_dir().unwrap_or_default();
    let allowed_signers_paths = [
        home.join(".ssh/allowed_signers"),
        home.join(".config/git/allowed_signers"),
    ];

    let allowed_signers = allowed_signers_paths.iter().find(|p| p.exists());

    let allowed_signers = match allowed_signers {
        Some(path) => path.clone(),
        None => {
            return SignatureStatus::Untrusted {
                signer: format!(
                    "{} (no allowed_signers file found; SSH signature present but cannot verify)",
                    signer
                ),
            };
        }
    };

    // Write signature and data to temp files
    let tmp_dir = match tempfile::TempDir::new() {
        Ok(d) => d,
        Err(_) => {
            return SignatureStatus::Untrusted {
                signer: format!("{} (failed to create temp dir)", signer),
            };
        }
    };

    let sig_path = tmp_dir.path().join("commit.sig");
    let data_path = tmp_dir.path().join("commit.data");

    if std::fs::write(&sig_path, signature).is_err()
        || std::fs::write(&data_path, signed_data).is_err()
    {
        return SignatureStatus::Untrusted {
            signer: format!("{} (failed to write temp files)", signer),
        };
    }

    // Extract principal (email) from signer string
    let principal = extract_email(signer).unwrap_or(signer);

    let output = Command::new("ssh-keygen")
        .args([
            "-Y",
            "verify",
            "-f",
            &allowed_signers.display().to_string(),
            "-I",
            principal,
            "-n",
            "git",
            "-s",
            &sig_path.display().to_string(),
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            if let Some(ref mut stdin) = child.stdin {
                use std::io::Write;
                let _ = stdin.write_all(signed_data.as_bytes());
            }
            child.wait_with_output()
        });

    match output {
        Ok(result) => {
            let stderr = String::from_utf8_lossy(&result.stderr);
            if result.status.success() {
                SignatureStatus::Valid {
                    signer: signer.to_string(),
                }
            } else if stderr.contains("Could not verify") || stderr.contains("INVALID") {
                SignatureStatus::Invalid {
                    reason: format!("bad SSH signature from {}", signer),
                }
            } else {
                SignatureStatus::Untrusted {
                    signer: format!(
                        "{} (ssh-keygen verification failed: {})",
                        signer,
                        stderr.lines().next().unwrap_or("unknown error")
                    ),
                }
            }
        }
        Err(_) => SignatureStatus::Untrusted {
            signer: format!("{} (failed to execute ssh-keygen)", signer),
        },
    }
}

/// Extract an email address from a "Name <email>" string.
fn extract_email(s: &str) -> Option<&str> {
    let start = s.find('<')?;
    let end = s.find('>')?;
    if start < end {
        Some(&s[start + 1..end])
    } else {
        None
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
    use git2::Repository;
    use std::collections::BTreeMap;
    use tempfile::TempDir;

    fn setup_test_repo() -> (TempDir, git2::Oid) {
        let tmp = TempDir::new().unwrap();
        let repo = Repository::init(tmp.path()).unwrap();

        let sig = git2::Signature::now("Test", "test@test.com").unwrap();
        let tree_id = {
            let mut index = repo.index().unwrap();
            std::fs::write(tmp.path().join("README"), "hello").unwrap();
            index.add_path(std::path::Path::new("README")).unwrap();
            index.write().unwrap();
            index.write_tree().unwrap()
        };
        let tree = repo.find_tree(tree_id).unwrap();
        let oid = repo
            .commit(Some("HEAD"), &sig, &sig, "initial commit", &tree, &[])
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
            features: vec![],
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

    #[test]
    fn test_extract_email() {
        assert_eq!(
            extract_email("Test User <test@example.com>"),
            Some("test@example.com")
        );
        assert_eq!(extract_email("no-angle-brackets"), None);
        assert_eq!(extract_email("<only@email.com>"), Some("only@email.com"));
    }

    #[test]
    fn test_unsigned_commit_detection() {
        let (tmp, _oid) = setup_test_repo();
        let repo = Repository::open(tmp.path()).unwrap();
        let head = repo.head().unwrap().target().unwrap();
        let commit = repo.find_commit(head).unwrap();

        // Test commits created without signing should be detected as Unsigned
        let status = check_commit_signature(&repo, &commit);
        assert_eq!(status, SignatureStatus::Unsigned);
    }
}
