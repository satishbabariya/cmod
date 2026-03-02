use std::path::Path;

use git2::{Oid, Repository};
use semver::Version;

use cmod_core::error::CmodError;

/// Fetch or open a Git repository.
///
/// If `dest` exists and is already a Git repo, fetches updates.
/// Otherwise, clones from `url`.
pub fn fetch_repo(url: &str, dest: &Path) -> Result<Repository, CmodError> {
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
fn clone_repo(url: &str, dest: &Path) -> Result<Repository, CmodError> {
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
pub fn short_hash(oid: &Oid) -> String {
    oid.to_string()[..8].to_string()
}

/// Get the date of a commit as YYYYMMDD.
pub fn commit_date(repo: &Repository, oid: Oid) -> Result<String, CmodError> {
    let commit = repo.find_commit(oid).map_err(|e| CmodError::GitError {
        reason: format!("commit not found: {}", e),
    })?;

    let time = commit.time();
    let secs = time.seconds();
    let dt =
        chrono::DateTime::from_timestamp(secs, 0).unwrap_or_else(|| chrono::DateTime::UNIX_EPOCH);
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
