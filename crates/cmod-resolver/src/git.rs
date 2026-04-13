use std::path::Path;

use git2::{Oid, Repository};
use semver::Version;

use cmod_core::error::CmodError;

/// Allowed Git URL schemes for security.
const ALLOWED_SCHEMES: &[&str] = &["https://", "ssh://git@", "git@"];

/// Environment variable to allow file:// URLs (for testing only).
const ALLOW_LOCAL_GIT_URLS_ENV: &str = "CMOD_ALLOW_LOCAL_GIT_URLS";

/// Validate that a Git URL uses a secure protocol.
///
/// Only allows https://, ssh://git@, and git@ (SSH shorthand) schemes.
/// Rejects file://, git://, http://, and local paths which could be
/// used for unauthorized access or data exfiltration.
///
/// For testing purposes, set `CMOD_ALLOW_LOCAL_GIT_URLS=1` to allow file:// URLs.
fn validate_git_url(url: &str) -> Result<(), CmodError> {
    // Check for allowed schemes
    let has_allowed_scheme = ALLOWED_SCHEMES.iter().any(|scheme| url.starts_with(scheme));

    if has_allowed_scheme {
        return Ok(());
    }

    // Allow file:// URLs if explicitly enabled (for testing)
    if (url.starts_with("file://") || url.starts_with('/') || url.starts_with('.'))
        && std::env::var(ALLOW_LOCAL_GIT_URLS_ENV).is_ok_and(|v| v == "1" || v == "true")
    {
        return Ok(());
    }

    // Provide specific error message for common insecure protocols
    if url.starts_with("http://") {
        return Err(CmodError::SecurityViolation {
            reason: format!(
                "insecure Git protocol: '{}' uses unencrypted HTTP. Use HTTPS instead.",
                url
            ),
        });
    }
    if url.starts_with("git://") {
        return Err(CmodError::SecurityViolation {
            reason: format!(
                "insecure Git protocol: '{}' uses unencrypted git://. Use HTTPS or SSH instead.",
                url
            ),
        });
    }
    if url.starts_with("file://") || url.starts_with('/') || url.starts_with('.') {
        return Err(CmodError::SecurityViolation {
            reason: format!(
                "local file paths not allowed as Git URLs: '{}'. Use a remote repository URL.",
                url
            ),
        });
    }

    Err(CmodError::SecurityViolation {
        reason: format!(
            "unsupported Git URL scheme: '{}'. Only HTTPS and SSH URLs are allowed.",
            url
        ),
    })
}

/// Fetch or open a Git repository.
///
/// If `dest` exists and is already a Git repo, fetches updates.
/// Otherwise, clones from `url`.
///
/// Only allows secure protocols (HTTPS, SSH). Rejects file://, git://,
/// http://, and local paths for security.
pub fn fetch_repo(url: &str, dest: &Path) -> Result<Repository, CmodError> {
    // Validate URL before any network operations
    validate_git_url(url)?;

    if dest.exists() && dest.join(".git").exists() {
        // Open and fetch
        let repo = Repository::open(dest).map_err(|e| CmodError::GitError {
            reason: format!("failed to open repo at {}: {}", dest.display(), e),
        })?;
        fetch_remote(&repo, "origin")?;
        Ok(repo)
    } else if dest.exists() {
        // Bare repo or non-git directory — try opening as bare
        let repo = Repository::open_bare(dest).map_err(|e| CmodError::GitError {
            reason: format!("failed to open bare repo at {}: {}", dest.display(), e),
        })?;
        fetch_remote(&repo, "origin")?;
        Ok(repo)
    } else {
        clone_repo(url, dest)
    }
}

/// Clone a repository.
///
/// Validates URL security before cloning. Only HTTPS and SSH are allowed.
fn clone_repo(url: &str, dest: &Path) -> Result<Repository, CmodError> {
    // Validate URL (double-check even if called from fetch_repo)
    validate_git_url(url)?;

    std::fs::create_dir_all(dest)?;
    Repository::clone(url, dest).map_err(|e| CmodError::GitRepoNotFound {
        url: format!("{}: {}", url, e),
    })
}

/// Fetch updates from a remote.
fn fetch_remote(repo: &Repository, remote_name: &str) -> Result<(), CmodError> {
    let mut remote = repo
        .find_remote(remote_name)
        .map_err(|e| CmodError::GitError {
            reason: format!("remote '{}' not found: {}", remote_name, e),
        })?;

    remote
        .fetch(&[] as &[&str], None, None)
        .map_err(|e| CmodError::GitError {
            reason: format!("fetch failed: {}", e),
        })?;

    Ok(())
}

/// List all tags in a repository that look like semver versions.
pub fn list_version_tags(repo: &Repository) -> Result<Vec<(Version, Oid)>, CmodError> {
    let mut versions = Vec::new();

    let tags = repo.tag_names(None).map_err(|e| CmodError::GitError {
        reason: format!("failed to list tags: {}", e),
    })?;

    for tag_name in tags.iter().flatten() {
        // Strip 'v' prefix and try to parse as semver
        let version_str = tag_name.strip_prefix('v').unwrap_or(tag_name);
        if let Ok(version) = Version::parse(version_str) {
            // Resolve the tag to a commit OID via revparse
            if let Ok(obj) = repo.revparse_single(&format!("refs/tags/{}", tag_name)) {
                let commit_oid = obj
                    .peel_to_commit()
                    .map(|c| c.id())
                    .unwrap_or_else(|_| obj.id());
                versions.push((version, commit_oid));
            }
        }
    }

    versions.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(versions)
}

/// Resolve a branch name to a commit OID.
pub fn resolve_branch(repo: &Repository, branch: &str) -> Result<Oid, CmodError> {
    // Try remote branch first
    let refname = format!("refs/remotes/origin/{}", branch);
    if let Ok(reference) = repo.find_reference(&refname) {
        if let Some(oid) = reference.target() {
            return Ok(oid);
        }
    }

    // Try local branch
    let refname = format!("refs/heads/{}", branch);
    if let Ok(reference) = repo.find_reference(&refname) {
        if let Some(oid) = reference.target() {
            return Ok(oid);
        }
    }

    Err(CmodError::GitRefNotFound {
        reference: branch.to_string(),
        url: "local".to_string(),
    })
}

/// Resolve an exact commit hash (prefix match).
pub fn resolve_commit(repo: &Repository, rev: &str) -> Result<Oid, CmodError> {
    let obj = repo
        .revparse_single(rev)
        .map_err(|e| CmodError::GitRefNotFound {
            reference: rev.to_string(),
            url: format!("revparse failed: {}", e),
        })?;

    Ok(obj
        .peel_to_commit()
        .map(|c| c.id())
        .unwrap_or_else(|_| obj.id()))
}

/// Get the short hash (first 8 chars) of an OID.
///
/// Uses safe string slicing to avoid panics on malformed OIDs.
pub fn short_hash(oid: &Oid) -> String {
    let full = oid.to_string();
    full.chars().take(8).collect()
}

/// Get the date of a commit as YYYYMMDD.
///
/// Logs a warning if the commit has an invalid timestamp and falls back
/// to the current date to avoid cache key collisions.
pub fn commit_date(repo: &Repository, oid: Oid) -> Result<String, CmodError> {
    let commit = repo.find_commit(oid).map_err(|e| CmodError::GitError {
        reason: format!("commit not found: {}", e),
    })?;

    let time = commit.time();
    let secs = time.seconds();
    let dt = match chrono::DateTime::from_timestamp(secs, 0) {
        Some(dt) => dt,
        None => {
            // Log warning about invalid timestamp - use eprintln for now
            // In production, this should use a proper logging framework
            eprintln!(
                "warning: commit {} has invalid timestamp ({}), using current date",
                short_hash(&oid),
                secs
            );
            // Fall back to current time instead of UNIX_EPOCH to avoid cache collisions
            chrono::Utc::now()
        }
    };
    Ok(dt.format("%Y%m%d").to_string())
}

/// Checkout a specific commit into the working directory of a repo.
pub fn checkout_commit(repo: &Repository, oid: Oid) -> Result<(), CmodError> {
    let commit = repo.find_commit(oid).map_err(|e| CmodError::GitError {
        reason: format!("commit not found: {}", e),
    })?;

    let tree = commit.tree().map_err(|e| CmodError::GitError {
        reason: format!("failed to get tree: {}", e),
    })?;

    repo.checkout_tree(
        tree.as_object(),
        Some(git2::build::CheckoutBuilder::new().force()),
    )
    .map_err(|e| CmodError::GitError {
        reason: format!("checkout failed: {}", e),
    })?;

    repo.set_head_detached(oid)
        .map_err(|e| CmodError::GitError {
            reason: format!("failed to detach HEAD: {}", e),
        })?;

    Ok(())
}

/// Compute a SHA-256 content hash for the files in a repository at a given commit.
pub fn content_hash_at_commit(repo: &Repository, oid: Oid) -> Result<String, CmodError> {
    use sha2::{Digest, Sha256};

    let commit = repo.find_commit(oid).map_err(|e| CmodError::GitError {
        reason: format!("commit not found: {}", e),
    })?;

    let tree = commit.tree().map_err(|e| CmodError::GitError {
        reason: format!("failed to get tree: {}", e),
    })?;

    let mut hasher = Sha256::new();

    // Walk the tree and hash all blob contents
    tree.walk(git2::TreeWalkMode::PreOrder, |dir, entry| {
        if let Some(git2::ObjectType::Blob) = entry.kind() {
            if let Ok(blob) = repo.find_blob(entry.id()) {
                hasher.update(dir.as_bytes());
                if let Some(name) = entry.name() {
                    hasher.update(name.as_bytes());
                }
                hasher.update(blob.content());
            }
        }
        git2::TreeWalkResult::Ok
    })
    .map_err(|e| CmodError::GitError {
        reason: format!("tree walk failed: {}", e),
    })?;

    Ok(format!("sha256:{}", hex::encode(hasher.finalize())))
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- validate_git_url tests ---

    #[test]
    fn test_validate_https_url() {
        assert!(validate_git_url("https://github.com/fmtlib/fmt").is_ok());
        assert!(validate_git_url("https://gitlab.com/org/repo.git").is_ok());
    }

    #[test]
    fn test_validate_ssh_url() {
        assert!(validate_git_url("ssh://git@github.com/org/repo").is_ok());
        assert!(validate_git_url("git@github.com:org/repo.git").is_ok());
    }

    #[test]
    fn test_reject_http_url() {
        let err = validate_git_url("http://github.com/fmtlib/fmt").unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("insecure") || msg.contains("HTTP"));
    }

    #[test]
    fn test_reject_git_protocol() {
        let err = validate_git_url("git://github.com/fmtlib/fmt").unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("insecure") || msg.contains("git://"));
    }

    #[test]
    fn test_reject_file_url() {
        let err = validate_git_url("file:///tmp/repo").unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("local file"));
    }

    #[test]
    fn test_reject_absolute_path() {
        let err = validate_git_url("/tmp/repo").unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("local file"));
    }

    #[test]
    fn test_reject_relative_path() {
        let err = validate_git_url("./local/repo").unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("local file"));
    }

    #[test]
    fn test_reject_unknown_scheme() {
        let err = validate_git_url("ftp://example.com/repo").unwrap_err();
        let msg = format!("{}", err);
        assert!(msg.contains("unsupported"));
    }

    // --- short_hash tests ---

    #[test]
    fn test_short_hash_length() {
        let oid = Oid::from_str("aabbccdd11223344556677889900aabbccddeeff").unwrap();
        let short = short_hash(&oid);
        assert_eq!(short.len(), 8);
        assert_eq!(short, "aabbccdd");
    }

    // --- content_hash_at_commit tests ---

    #[test]
    fn test_content_hash_deterministic() {
        let tmp = tempfile::TempDir::new().unwrap();
        let repo = Repository::init(tmp.path()).unwrap();

        // Create a file and commit it
        let file_path = tmp.path().join("hello.txt");
        std::fs::write(&file_path, "hello world\n").unwrap();

        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new("hello.txt")).unwrap();
        index.write().unwrap();
        let tree_oid = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_oid).unwrap();
        let sig = git2::Signature::now("test", "test@test.com").unwrap();
        let commit_oid = repo
            .commit(Some("HEAD"), &sig, &sig, "initial", &tree, &[])
            .unwrap();

        let hash1 = content_hash_at_commit(&repo, commit_oid).unwrap();
        let hash2 = content_hash_at_commit(&repo, commit_oid).unwrap();
        assert_eq!(hash1, hash2);
        assert!(hash1.starts_with("sha256:"));
    }

    #[test]
    fn test_content_hash_changes_with_content() {
        let tmp = tempfile::TempDir::new().unwrap();
        let repo = Repository::init(tmp.path()).unwrap();
        let sig = git2::Signature::now("test", "test@test.com").unwrap();

        // First commit
        std::fs::write(tmp.path().join("file.txt"), "version 1").unwrap();
        let mut index = repo.index().unwrap();
        index
            .add_path(std::path::Path::new("file.txt"))
            .unwrap();
        index.write().unwrap();
        let tree1 = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let oid1 = repo
            .commit(Some("HEAD"), &sig, &sig, "v1", &tree1, &[])
            .unwrap();

        // Second commit with different content
        std::fs::write(tmp.path().join("file.txt"), "version 2").unwrap();
        let mut index = repo.index().unwrap();
        index
            .add_path(std::path::Path::new("file.txt"))
            .unwrap();
        index.write().unwrap();
        let tree2 = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let parent = repo.find_commit(oid1).unwrap();
        let oid2 = repo
            .commit(Some("HEAD"), &sig, &sig, "v2", &tree2, &[&parent])
            .unwrap();

        let hash1 = content_hash_at_commit(&repo, oid1).unwrap();
        let hash2 = content_hash_at_commit(&repo, oid2).unwrap();
        assert_ne!(hash1, hash2);
    }

    // --- list_version_tags tests ---

    #[test]
    fn test_list_version_tags_parses_semver() {
        let tmp = tempfile::TempDir::new().unwrap();
        let repo = Repository::init(tmp.path()).unwrap();
        let sig = git2::Signature::now("test", "test@test.com").unwrap();

        // Create initial commit
        std::fs::write(tmp.path().join("f.txt"), "data").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new("f.txt")).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let commit_oid = repo
            .commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .unwrap();
        let commit_obj = repo.find_object(commit_oid, None).unwrap();

        // Create semver tags
        repo.tag_lightweight("v1.0.0", &commit_obj, false).unwrap();
        repo.tag_lightweight("v2.1.0", &commit_obj, false).unwrap();
        repo.tag_lightweight("not-semver", &commit_obj, false)
            .unwrap();

        let tags = list_version_tags(&repo).unwrap();
        assert_eq!(tags.len(), 2);
        assert_eq!(tags[0].0.to_string(), "1.0.0");
        assert_eq!(tags[1].0.to_string(), "2.1.0");
    }

    #[test]
    fn test_list_version_tags_strips_v_prefix() {
        let tmp = tempfile::TempDir::new().unwrap();
        let repo = Repository::init(tmp.path()).unwrap();
        let sig = git2::Signature::now("test", "test@test.com").unwrap();

        std::fs::write(tmp.path().join("f.txt"), "data").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(std::path::Path::new("f.txt")).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let oid = repo
            .commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .unwrap();
        let obj = repo.find_object(oid, None).unwrap();

        // Tag without v prefix should also parse
        repo.tag_lightweight("3.0.0", &obj, false).unwrap();

        let tags = list_version_tags(&repo).unwrap();
        assert_eq!(tags.len(), 1);
        assert_eq!(tags[0].0.to_string(), "3.0.0");
    }
}
