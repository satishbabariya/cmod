use std::path::PathBuf;

use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::lockfile::Lockfile;
use cmod_core::manifest::{Dependency, DetailedDependency, Manifest};
use cmod_core::shell::Shell;
use cmod_resolver::Resolver;

/// Run `cmod add <dep>` — add a dependency.
#[allow(clippy::too_many_arguments)]
pub fn run(
    dep: String,
    git: Option<String>,
    branch: Option<String>,
    rev: Option<String>,
    path: Option<String>,
    features: Vec<String>,
    _locked: bool,
    offline: bool,
    shell: &Shell,
) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let mut config = Config::load(&cwd)?;

    // Validate the dependency specifier is not empty
    let trimmed = dep.trim();
    if trimmed.is_empty() || trimmed == "@" {
        return Err(CmodError::Other(
            "dependency specifier cannot be empty".to_string(),
        ));
    }

    // Parse the dep specifier: "github.com/fmtlib/fmt@^10.2" or "github.com/fmtlib/fmt"
    let (dep_key, version_constraint) = parse_dep_specifier(&dep);

    // Check if dependency already exists
    if let Some(existing) = config.manifest.dependencies.get(&dep_key) {
        let new_version = version_constraint.as_deref();
        let existing_version = existing.version_req();

        if new_version.is_some() && new_version != existing_version {
            shell.warn(format!(
                "dependency '{}' already exists with version '{}', updating to '{}'",
                dep_key,
                existing_version.unwrap_or("*"),
                new_version.unwrap_or("*")
            ));
            // Remove the old entry so resolver can re-add
            config.manifest.dependencies.remove(&dep_key);
        } else {
            shell.status(
                "Unchanged",
                format!(
                    "dependency '{}' (version: {})",
                    dep_key,
                    existing_version.unwrap_or("*")
                ),
            );
            return Ok(());
        }
    }

    // Build the Dependency object
    let dependency = if path.is_some()
        || git.is_some()
        || branch.is_some()
        || rev.is_some()
        || !features.is_empty()
    {
        Dependency::Detailed(DetailedDependency {
            version: version_constraint,
            git,
            branch,
            rev,
            tag: None,
            path: path.map(PathBuf::from),
            features,
            optional: false,
            default_features: true,
            workspace: false,
        })
    } else if let Some(ver) = version_constraint {
        Dependency::Simple(ver)
    } else {
        Dependency::Simple("*".to_string())
    };

    // Validate Git URL is reachable (unless offline or path dep)
    if !offline && !dependency.is_path() {
        let url = Manifest::resolve_dep_url(&dep_key, &dependency);
        validate_git_url(&url)?;
    }

    // Load existing lockfile if present
    let existing_lock = Lockfile::load(&config.lockfile_path).ok();

    // Resolve activated features for compiler defines
    let mut resolver = Resolver::new(config.deps_dir());
    let lockfile = resolver.add_dependency(
        &mut config.manifest,
        dep_key.clone(),
        dependency,
        existing_lock.as_ref(),
    )?;

    // Save updated manifest and lockfile
    config.manifest.save(&config.manifest_path)?;
    lockfile.save(&config.lockfile_path)?;

    if let Some(pkg) = lockfile.find_package(&dep_key) {
        shell.status("Adding", format!("'{}' v{}", dep_key, pkg.version));
    } else {
        shell.status("Adding", format!("'{}'", dep_key));
    }

    Ok(())
}

/// Parse a dependency specifier like `github.com/fmtlib/fmt@^10.2`.
fn parse_dep_specifier(spec: &str) -> (String, Option<String>) {
    if let Some(idx) = spec.find('@') {
        let key = spec[..idx].to_string();
        let version = spec[idx + 1..].to_string();
        (key, Some(version))
    } else {
        (spec.to_string(), None)
    }
}

/// Validate that a Git URL is reachable by attempting to list remote refs.
fn validate_git_url(url: &str) -> Result<(), CmodError> {
    // Use git2 to check if the remote is reachable
    match git2::Repository::init_bare(tempfile::TempDir::new()?.path()) {
        Ok(repo) => {
            let mut remote = repo
                .remote_anonymous(url)
                .map_err(|e| CmodError::GitError {
                    reason: format!("invalid Git URL '{}': {}", url, e),
                })?;

            // Try to connect to the remote (read-only)
            match remote.connect(git2::Direction::Fetch) {
                Ok(()) => {
                    let _ = remote.disconnect();
                    Ok(())
                }
                Err(e) => Err(CmodError::GitError {
                    reason: format!(
                        "cannot reach '{}': {}. Use --offline to skip this check",
                        url, e
                    ),
                }),
            }
        }
        Err(e) => Err(CmodError::Other(format!("failed to validate URL: {}", e))),
    }
}
