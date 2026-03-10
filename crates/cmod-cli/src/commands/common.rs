//! Shared utilities for dependency artifact discovery across CLI commands.
//!
//! Functions in this module are used by `build`, `test`, `compile-commands`,
//! and `plan` to locate vendored/resolved dependency artifacts on disk.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use cmod_build::runner;
use cmod_core::config::Config;
use cmod_core::lockfile::{LockedPackage, Lockfile};

/// Collected artifacts from a built dependency.
#[derive(Debug, Default, Clone)]
pub struct DepArtifacts {
    /// Module name → PCM file path.
    pub pcms: HashMap<String, PathBuf>,
    /// Object and static library files for linking.
    pub objs: Vec<PathBuf>,
    /// Include directories (`-I` paths) from dependencies.
    pub include_dirs: Vec<PathBuf>,
}

impl DepArtifacts {
    pub fn merge(&mut self, other: &DepArtifacts) {
        self.pcms.extend(other.pcms.clone());
        self.objs.extend(other.objs.clone());
        self.include_dirs.extend(other.include_dirs.clone());
    }
}

/// Find a dependency on disk, checking `vendor/` first, then `build/deps/`.
///
/// The vendor directory uses real path separators (e.g., `vendor/github.com/user/repo`),
/// while the deps directory uses sanitized names (e.g., `build/deps/github.com_user_repo`).
pub fn find_dep_on_disk(vendor_dir: &Path, deps_dir: &Path, pkg_name: &str) -> Option<PathBuf> {
    // Try vendor/ first (uses real path separators as written by `cmod vendor`)
    let vendor_path = vendor_dir.join(pkg_name);
    if vendor_path.exists() {
        return Some(vendor_path);
    }

    // Try build/deps/ (uses sanitized underscores as written by the resolver)
    let sanitized = pkg_name.replace(['/', '\\'], "_");
    let deps_path = deps_dir.join(&sanitized);
    if deps_path.exists() {
        return Some(deps_path);
    }

    None
}

/// Ensure a git dependency is present on disk, cloning it if necessary.
///
/// First checks `vendor/` and `build/deps/` via `find_dep_on_disk()`. If the dep
/// is not found, clones it from the lockfile's `repo` URL and checks out the
/// locked `commit` hash. Returns the path if the dep has a `cmod.toml`, or `None`.
pub fn ensure_dep_on_disk(
    pkg: &LockedPackage,
    vendor_dir: &Path,
    deps_dir: &Path,
    shell: &cmod_core::shell::Shell,
) -> Result<Option<PathBuf>, cmod_core::error::CmodError> {
    // Try finding it on disk first
    if let Some(d) = find_dep_on_disk(vendor_dir, deps_dir, &pkg.name) {
        if d.join("cmod.toml").exists() {
            // For deps in build/deps/ (not vendor), verify the checked-out commit
            // matches the lockfile to avoid reusing stale checkouts.
            let is_in_deps_dir = d.starts_with(deps_dir);
            if is_in_deps_dir {
                if let Some(expected_commit) = &pkg.commit {
                    let matches = git2::Repository::open(&d)
                        .and_then(|repo| repo.head()?.peel_to_commit().map(|c| c.id()))
                        .map(|head_oid| head_oid.to_string() == *expected_commit)
                        .unwrap_or(false);
                    if !matches {
                        // Stale checkout — remove so fetch_repo gets a clean directory
                        let _ = std::fs::remove_dir_all(&d);
                    } else {
                        return Ok(Some(d));
                    }
                } else {
                    return Ok(Some(d));
                }
            } else {
                return Ok(Some(d));
            }
        }
    }

    // Extract repo URL and commit from lockfile
    let repo_url = match &pkg.repo {
        Some(url) => url.clone(),
        None => return Ok(None),
    };

    // Skip path dependencies — they must already exist on disk
    if repo_url.starts_with("path:") {
        return Ok(None);
    }

    let commit_hex = match &pkg.commit {
        Some(c) => c.clone(),
        None => return Ok(None),
    };

    // Clone to build/deps/ using sanitized name
    let sanitized = pkg.name.replace(['/', '\\'], "_");
    let dest = deps_dir.join(&sanitized);

    shell.status(
        "Fetching",
        format!("{} ({})", pkg.name, &commit_hex[..8.min(commit_hex.len())]),
    );

    std::fs::create_dir_all(deps_dir)?;
    let repo = cmod_resolver::git::fetch_repo(&repo_url, &dest)?;

    let oid =
        git2::Oid::from_str(&commit_hex).map_err(|e| cmod_core::error::CmodError::GitError {
            reason: format!("invalid commit hash '{}': {}", commit_hex, e),
        })?;

    cmod_resolver::git::checkout_commit(&repo, oid)?;

    if dest.join("cmod.toml").exists() {
        Ok(Some(dest))
    } else {
        Ok(None)
    }
}

/// Topologically sort lockfile packages so dependencies are built before dependents.
///
/// Uses a DFS-based topological sort on the `deps` field of each package.
pub fn topo_sort_packages(packages: &[LockedPackage]) -> Vec<&LockedPackage> {
    let name_to_idx: HashMap<&str, usize> = packages
        .iter()
        .enumerate()
        .map(|(i, p)| (p.name.as_str(), i))
        .collect();

    let mut visited = vec![false; packages.len()];
    let mut order: Vec<&LockedPackage> = Vec::new();

    fn visit<'a>(
        idx: usize,
        packages: &'a [LockedPackage],
        name_to_idx: &HashMap<&str, usize>,
        visited: &mut Vec<bool>,
        order: &mut Vec<&'a LockedPackage>,
    ) {
        if visited[idx] {
            return;
        }
        visited[idx] = true;

        // Visit dependencies first
        for dep_name in &packages[idx].deps {
            if let Some(&dep_idx) = name_to_idx.get(dep_name.as_str()) {
                visit(dep_idx, packages, name_to_idx, visited, order);
            }
        }

        order.push(&packages[idx]);
    }

    for i in 0..packages.len() {
        visit(i, packages, &name_to_idx, &mut visited, &mut order);
    }

    order
}

/// Auto-detect conventional include directories for a dependency.
///
/// Checks for `include/`, `inc/`, and declared `[build].include_dirs`.
/// Returns absolute paths.
pub fn detect_include_dirs(dep_dir: &Path, config: &Config) -> Vec<PathBuf> {
    let mut dirs = Vec::new();

    // Check for conventional include/ directory
    let include_dir = dep_dir.join("include");
    if include_dir.is_dir() {
        dirs.push(include_dir);
    }

    // Check for inc/ directory (less common convention)
    let inc_dir = dep_dir.join("inc");
    if inc_dir.is_dir() {
        dirs.push(inc_dir);
    }

    // Add declared include_dirs from the dep's manifest [build] section
    if let Some(ref build) = config.manifest.build {
        for dir in &build.include_dirs {
            let abs = dep_dir.join(dir);
            if abs.is_dir() && !dirs.contains(&abs) {
                dirs.push(abs);
            }
        }
    }

    dirs
}

/// Collect already-built artifacts (PCMs, objects, include dirs) for all lockfile
/// git dependencies without triggering a build.
///
/// This is useful for commands like `compile-commands` and `test` that need to
/// reference dependency artifacts without building them.
pub fn collect_dep_artifacts(config: &Config, lockfile: &Lockfile) -> DepArtifacts {
    let mut result = DepArtifacts::default();

    let vendor_dir = config.root.join("vendor");
    let deps_dir = config.deps_dir();
    let ordered = topo_sort_packages(&lockfile.packages);

    for pkg in &ordered {
        if pkg.source.as_deref() != Some("git") {
            continue;
        }

        let dep_dir = match find_dep_on_disk(&vendor_dir, &deps_dir, &pkg.name) {
            Some(d) if d.join("cmod.toml").exists() => d,
            _ => continue,
        };

        // Load dep config to determine build dir and include dirs
        let dep_config = match Config::load(&dep_dir) {
            Ok(mut c) => {
                c.profile = config.profile;
                c
            }
            Err(_) => continue,
        };

        // Collect include directories
        let inc_dirs = detect_include_dirs(&dep_dir, &dep_config);
        result.include_dirs.extend(inc_dirs);

        // Collect PCM files from the dep's build directory
        let dep_build_dir = dep_config.build_dir();
        let pcm_dir = dep_build_dir.join("pcm");
        if pcm_dir.exists() {
            let dep_sources = runner::discover_sources_multi(
                &dep_config.src_dirs(),
                &dep_config.exclude_patterns(),
            )
            .unwrap_or_default();
            for source in &dep_sources {
                if let Ok(Some(mod_name)) = runner::extract_module_name(source) {
                    let sanitized = mod_name.replace(['.', ':', '/'], "_");
                    let pcm_path = pcm_dir.join(format!("{}.pcm", sanitized));
                    if pcm_path.exists() {
                        result.pcms.insert(mod_name, pcm_path);
                    }
                }
            }
        }

        // Collect object files
        let obj_dir = dep_build_dir.join("obj");
        if obj_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&obj_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("o") {
                        result.objs.push(path);
                    }
                }
            }
        }

        // Collect static library artifacts
        if dep_build_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&dep_build_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("a") {
                        result.objs.push(path);
                    }
                }
            }
        }
    }

    result
}
