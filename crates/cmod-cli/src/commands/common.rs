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

/// Map each module that `sources` declare to its BMI in `bmi_dir`.
///
/// `bmi_ext` must come from the backend that built `bmi_dir`
/// (`CompilerBackend::bmi_extension`): Clang writes `.pcm`, GCC `.gcm` and
/// MSVC `.ifc`. Modules whose BMI is missing are left out.
pub fn collect_module_bmis(
    bmi_dir: &Path,
    sources: &[PathBuf],
    bmi_ext: &str,
) -> HashMap<String, PathBuf> {
    let mut bmis = HashMap::new();
    if !bmi_dir.exists() {
        return bmis;
    }
    for source in sources {
        if let Ok(Some(mod_name)) = runner::extract_module_name(source) {
            let sanitized = mod_name.replace(['.', ':', '/'], "_");
            let bmi_path = bmi_dir.join(format!("{}.{}", sanitized, bmi_ext));
            if bmi_path.exists() {
                bmis.insert(mod_name, bmi_path);
            }
        }
    }
    bmis
}

/// Build an HTTP remote-cache client honoring the manifest's `[cache]`
/// settings: `auth_token_env` (bearer token read from the environment),
/// `timeout`, and `retries`.
pub fn remote_cache_client(
    config: &Config,
    url: &str,
    mode: cmod_cache::RemoteCacheMode,
) -> cmod_cache::HttpRemoteCache {
    let cache_cfg = config.manifest.cache.as_ref();
    let token = cache_cfg
        .and_then(|c| c.auth_token_env.as_deref())
        .and_then(|name| std::env::var(name).ok());

    let mut client = cmod_cache::HttpRemoteCache::new(url, mode).with_auth_token(token);
    if let Some(secs) = cache_cfg.and_then(|c| c.timeout) {
        client = client.with_timeout(std::time::Duration::from_secs(secs));
    }
    if let Some(retries) = cache_cfg.and_then(|c| c.retries) {
        client = client.with_retries(retries);
    }
    client
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
/// The dependency's checkout, if it is on disk at the locked commit (or
/// vendored). Unlike [`ensure_dep_on_disk`], never fetches or removes
/// anything: for dry runs.
pub fn locked_checkout_on_disk(
    pkg: &LockedPackage,
    vendor_dir: &Path,
    deps_dir: &Path,
) -> Option<PathBuf> {
    let d = find_dep_on_disk(vendor_dir, deps_dir, &pkg.name)?;
    if !d.join("cmod.toml").exists() {
        return None;
    }
    match &pkg.commit {
        Some(expected) if d.starts_with(deps_dir) => git2::Repository::open(&d)
            .and_then(|repo| repo.head()?.peel_to_commit().map(|c| c.id()))
            .ok()
            .filter(|head| head.to_string() == *expected)
            .map(|_| d),
        _ => Some(d),
    }
}

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

/// Collect already-built artifacts (PCMs, objects, include dirs) from path dependencies.
///
/// Walks `[dependencies]` entries with `path = "..."`, loads their config,
/// and collects BMIs, static archives, and include dirs. Dependencies are built
/// with the root's compiler, so `bmi_ext` is the root backend's extension.
pub fn collect_path_dep_artifacts(config: &Config, bmi_ext: &str) -> DepArtifacts {
    let mut result = DepArtifacts::default();

    for dep in config.manifest.dependencies.values() {
        let dep_path = match dep.path() {
            Some(p) => config.root.join(p),
            None => continue,
        };

        if !dep_path.join("cmod.toml").exists() {
            continue;
        }

        let dep_config = match Config::load(&dep_path) {
            Ok(mut c) => {
                c.profile = config.profile;
                c
            }
            Err(_) => continue,
        };

        // Include directories
        let inc_dirs = detect_include_dirs(&dep_path, &dep_config);
        result.include_dirs.extend(inc_dirs);

        // PCM files
        let dep_build_dir = dep_config.build_dir();
        let dep_sources =
            runner::discover_sources_multi(&dep_config.src_dirs(), &dep_config.exclude_patterns())
                .unwrap_or_default();
        result.pcms.extend(collect_module_bmis(
            &dep_build_dir.join("pcm"),
            &dep_sources,
            bmi_ext,
        ));

        // Prefer .a archives over individual .o files
        let mut has_archive = false;
        if dep_build_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&dep_build_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("a") {
                        result.objs.push(path);
                        has_archive = true;
                    }
                }
            }
        }

        if !has_archive {
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
        }
    }

    result
}

/// Collect already-built artifacts (PCMs, objects, include dirs) for all lockfile
/// git dependencies without triggering a build.
///
/// This is useful for commands like `compile-commands` and `test` that need to
/// reference dependency artifacts without building them.
///
/// `bmi_ext` is the root backend's BMI extension, as for
/// [`collect_path_dep_artifacts`].
pub fn collect_dep_artifacts(config: &Config, lockfile: &Lockfile, bmi_ext: &str) -> DepArtifacts {
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
        let dep_sources =
            runner::discover_sources_multi(&dep_config.src_dirs(), &dep_config.exclude_patterns())
                .unwrap_or_default();
        result.pcms.extend(collect_module_bmis(
            &dep_build_dir.join("pcm"),
            &dep_sources,
            bmi_ext,
        ));

        // Collect linkable artifacts: prefer .a archives over individual .o files
        let mut has_archive = false;
        if dep_build_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&dep_build_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("a") {
                        result.objs.push(path);
                        has_archive = true;
                    }
                }
            }
        }

        if !has_archive {
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
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn git_package(commit: &str) -> LockedPackage {
        serde_json::from_value(serde_json::json!({
            "name": "github.com/acme/dep",
            "version": "1.0.0",
            "source": "git",
            "commit": commit,
        }))
        .unwrap()
    }

    /// A checkout of `github.com/acme/dep` in `deps_dir` with one commit.
    fn checkout(deps_dir: &Path) -> (PathBuf, String) {
        let dir = deps_dir.join("github.com_acme_dep");
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("cmod.toml"), "[package]\nname = \"dep\"\n").unwrap();
        let repo = git2::Repository::init(&dir).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("cmod.toml")).unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = git2::Signature::now("t", "t@example.com").unwrap();
        let commit = repo
            .commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .unwrap();
        (dir, commit.to_string())
    }

    #[test]
    fn locked_checkout_on_disk_finds_the_locked_commit() {
        let tmp = TempDir::new().unwrap();
        let deps = tmp.path().join("deps");
        let (dir, commit) = checkout(&deps);
        assert_eq!(
            locked_checkout_on_disk(&git_package(&commit), &tmp.path().join("vendor"), &deps),
            Some(dir)
        );
    }

    /// A dry run must neither fetch a missing checkout nor delete a stale
    /// one, which `ensure_dep_on_disk` does.
    #[test]
    fn locked_checkout_on_disk_reports_missing_and_stale_without_touching_them() {
        let tmp = TempDir::new().unwrap();
        let deps = tmp.path().join("deps");
        let vendor = tmp.path().join("vendor");
        let pkg = git_package("0000000000000000000000000000000000000000");
        assert_eq!(locked_checkout_on_disk(&pkg, &vendor, &deps), None);
        assert!(!deps.exists());

        let (dir, _) = checkout(&deps);
        assert_eq!(locked_checkout_on_disk(&pkg, &vendor, &deps), None);
        assert!(dir.join("cmod.toml").exists(), "stale checkout was removed");
    }

    /// A root package with a path dependency `dep` whose build left one BMI,
    /// `build/debug/pcm/dep_core.<ext>`, for module `dep.core`.
    fn root_with_built_path_dep(ext: &str) -> (TempDir, Config) {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().join("app");
        let dep = tmp.path().join("dep");
        std::fs::create_dir_all(root.join("src")).unwrap();
        std::fs::create_dir_all(dep.join("src")).unwrap();
        std::fs::create_dir_all(dep.join("build/debug/pcm")).unwrap();
        std::fs::write(
            root.join("cmod.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n\n\
             [toolchain]\ncompiler = \"gcc\"\n\n\
             [dependencies]\ndep = { path = \"../dep\" }\n",
        )
        .unwrap();
        std::fs::write(
            dep.join("cmod.toml"),
            "[package]\nname = \"dep\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        std::fs::write(dep.join("src/core.cppm"), "export module dep.core;\n").unwrap();
        std::fs::write(dep.join(format!("build/debug/pcm/dep_core.{}", ext)), "").unwrap();
        let config = Config::load(&root).unwrap();
        (tmp, config)
    }

    #[test]
    fn path_dep_bmis_use_the_backend_extension() {
        let (tmp, config) = root_with_built_path_dep("gcm");
        let artifacts = collect_path_dep_artifacts(&config, "gcm");
        assert_eq!(
            artifacts.pcms.get("dep.core"),
            Some(&tmp.path().join("app/../dep/build/debug/pcm/dep_core.gcm")),
        );
    }

    #[test]
    fn path_dep_bmis_ignore_other_extensions() {
        let (_tmp, config) = root_with_built_path_dep("pcm");
        let artifacts = collect_path_dep_artifacts(&config, "gcm");
        assert!(artifacts.pcms.is_empty(), "{:?}", artifacts.pcms);
    }
}
