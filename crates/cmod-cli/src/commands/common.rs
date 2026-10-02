//! Shared utilities for dependency artifact discovery across CLI commands.
//!
//! Functions in this module are used by `build`, `test`, `compile-commands`,
//! and `plan` to locate vendored/resolved dependency artifacts on disk.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use cmod_build::runner;
use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::lockfile::{LockedPackage, Lockfile};
use cmod_core::types::sanitize_package_name_for_path;

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
/// Both directories name a package's directory after it with path
/// separators replaced (`github.com_user_repo`), as `cmod vendor` and the
/// resolver write it. `vendor/github.com/user/repo`, laid out by hand, is
/// found too.
pub fn find_dep_on_disk(vendor_dir: &Path, deps_dir: &Path, pkg_name: &str) -> Option<PathBuf> {
    let sanitized = sanitize_package_name_for_path(pkg_name);
    [
        vendor_dir.join(&sanitized),
        vendor_dir.join(pkg_name),
        deps_dir.join(&sanitized),
    ]
    .into_iter()
    .find(|path| path.exists())
}

/// The file `cmod vendor` writes in each package it vendors: the commit it
/// exported and the SHA-256 of every file, so a build can tell the vendored
/// copy is still that commit.
pub const VENDOR_CHECKSUM_FILE: &str = ".cmod-checksum.json";

/// The contents of [`VENDOR_CHECKSUM_FILE`].
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub struct VendorChecksum {
    /// The locked commit the files were exported from.
    pub commit: String,
    /// Path relative to the package (with `/`) → SHA-256 of its contents.
    pub files: BTreeMap<String, String>,
}

/// Hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    hex::encode(Sha256::digest(bytes))
}

/// The first eight characters of a commit, for messages.
fn short_commit(commit: &str) -> &str {
    &commit[..8.min(commit.len())]
}

/// Check that the vendored copy of `pkg` in `dir` is its locked commit: the
/// commit [`VENDOR_CHECKSUM_FILE`] records, with every file it lists
/// unchanged, or for a vendored git checkout its HEAD. A copy without
/// either, or a package with no locked commit, is taken as it is.
pub fn verify_vendored(pkg: &LockedPackage, dir: &Path) -> Result<(), CmodError> {
    let Some(expected) = pkg.commit.as_deref() else {
        return Ok(());
    };
    let stale = |actual: &str| {
        CmodError::Other(format!(
            "vendored {} is commit {}, but cmod.lock locks {}; run `cmod vendor --sync`",
            pkg.name,
            short_commit(actual),
            short_commit(expected)
        ))
    };

    let checksum_path = dir.join(VENDOR_CHECKSUM_FILE);
    if checksum_path.exists() {
        let checksum: VendorChecksum =
            serde_json::from_str(&std::fs::read_to_string(&checksum_path)?).map_err(|e| {
                CmodError::Other(format!("unreadable {}: {}", checksum_path.display(), e))
            })?;
        if checksum.commit != expected {
            return Err(stale(&checksum.commit));
        }
        let changed: Vec<&str> = checksum
            .files
            .iter()
            .filter(|(rel, sum)| {
                std::fs::read(dir.join(rel))
                    .map(|bytes| sha256_hex(&bytes))
                    .ok()
                    != Some((*sum).clone())
            })
            .map(|(rel, _)| rel.as_str())
            .collect();
        if !changed.is_empty() {
            let shown: Vec<&str> = changed.iter().take(5).copied().collect();
            let more = match changed.len() - shown.len() {
                0 => String::new(),
                n => format!(" and {} more", n),
            };
            return Err(CmodError::SecurityViolation {
                reason: format!(
                    "vendored {} differs from commit {}: {}{} changed or missing; run `cmod vendor --sync` to restore it",
                    pkg.name,
                    short_commit(expected),
                    shown.join(", "),
                    more
                ),
            });
        }
        return Ok(());
    }

    if dir.join(".git").exists() {
        let head = git2::Repository::open(dir)
            .and_then(|repo| repo.head()?.peel_to_commit().map(|c| c.id().to_string()))
            .map_err(|e| CmodError::GitError {
                reason: format!("vendored {}: {}", pkg.name, e),
            })?;
        if head != expected {
            return Err(stale(&head));
        }
    }
    Ok(())
}

/// Tracked files of the git checkout in `dir` that differ from its HEAD
/// (build outputs and other untracked files aside), relative to `dir`.
pub fn modified_tracked_files(dir: &Path) -> Result<Vec<String>, CmodError> {
    let repo = git2::Repository::open(dir).map_err(|e| CmodError::GitError {
        reason: format!("failed to open {}: {}", dir.display(), e),
    })?;
    let mut options = git2::StatusOptions::new();
    options.include_untracked(false).include_ignored(false);
    let statuses = repo
        .statuses(Some(&mut options))
        .map_err(|e| CmodError::GitError {
            reason: format!("failed to read the status of {}: {}", dir.display(), e),
        })?;
    Ok(statuses
        .iter()
        .filter(|entry| entry.status() != git2::Status::CURRENT)
        .filter_map(|entry| entry.path().map(String::from))
        .collect())
}

/// Files directly in `dir` with extension `ext`, sorted. Sorted because the
/// order becomes a link command's order: unsorted, the link key changes
/// from one build to the next and the output is not reproducible.
pub fn files_with_extension(dir: &Path, ext: &str) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.extension().and_then(|e| e.to_str()) == Some(ext))
        .collect();
    files.sort();
    files
}

/// What a built dependency contributes to a link: its `.a` archives, or
/// its `.o` objects when it produced no archive. Sorted.
pub fn linkable_artifacts(build_dir: &Path) -> Vec<PathBuf> {
    let archives = files_with_extension(build_dir, "a");
    if !archives.is_empty() {
        return archives;
    }
    files_with_extension(&build_dir.join("obj"), "o")
}

/// Whether the build fetches `pkg` when it is not on disk, as
/// [`ensure_dep_on_disk`] does: a git URL and a locked commit. Path
/// dependencies (`path:` URLs) are not fetched.
pub fn is_fetched(pkg: &LockedPackage) -> bool {
    pkg.commit.is_some()
        && pkg
            .repo
            .as_deref()
            .is_some_and(|url| !url.starts_with("path:"))
}

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
        _ => verify_vendored(pkg, &d).ok().map(|_| d),
    }
}

/// Ensure a git dependency is present on disk, cloning it if necessary.
///
/// First checks `vendor/` and `build/deps/` via `find_dep_on_disk()`. A
/// vendored copy must be the locked commit ([`verify_vendored`]); a
/// checkout in `build/deps/` at another commit is replaced. A dependency
/// not on disk is cloned from the lockfile's `repo` URL at the locked
/// `commit`, except `offline`, which fails instead (and leaves a stale
/// checkout alone). Returns the path if the dep has a `cmod.toml`, or `None`.
pub fn ensure_dep_on_disk(
    pkg: &LockedPackage,
    vendor_dir: &Path,
    deps_dir: &Path,
    offline: bool,
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
                        if offline {
                            return Err(not_fetched_offline(pkg));
                        }
                        // Stale checkout — remove so fetch_repo gets a clean directory
                        let _ = std::fs::remove_dir_all(&d);
                    } else {
                        return Ok(Some(d));
                    }
                } else {
                    return Ok(Some(d));
                }
            } else {
                verify_vendored(pkg, &d)?;
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

    if offline {
        return Err(not_fetched_offline(pkg));
    }

    // Clone to build/deps/ using sanitized name
    let sanitized = sanitize_package_name_for_path(&pkg.name);
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

/// The error for a git dependency an `--offline` build would have to fetch.
fn not_fetched_offline(pkg: &LockedPackage) -> CmodError {
    CmodError::GitError {
        reason: format!(
            "{} is not vendored or checked out at commit {}, and --offline does not fetch; run `cmod vendor` (or build once without --offline)",
            pkg.name,
            short_commit(pkg.commit.as_deref().unwrap_or_default())
        ),
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
        result.objs.extend(linkable_artifacts(&dep_build_dir));
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
        result.objs.extend(linkable_artifacts(&dep_build_dir));
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
        index.write().unwrap();
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

    /// A vendored copy of `github.com/acme/dep` at `commit`, as `cmod
    /// vendor` writes it.
    fn vendored(vendor: &Path, commit: &str) -> PathBuf {
        let dir = vendor.join("github.com_acme_dep");
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(dir.join("cmod.toml"), "[package]\nname = \"dep\"\n").unwrap();
        std::fs::write(dir.join("src/lib.cppm"), "export module dep;\n").unwrap();
        let files = ["cmod.toml", "src/lib.cppm"]
            .iter()
            .map(|rel| {
                let bytes = std::fs::read(dir.join(rel)).unwrap();
                (rel.to_string(), sha256_hex(&bytes))
            })
            .collect();
        let checksum = VendorChecksum {
            commit: commit.to_string(),
            files,
        };
        std::fs::write(
            dir.join(VENDOR_CHECKSUM_FILE),
            serde_json::to_string(&checksum).unwrap(),
        )
        .unwrap();
        dir
    }

    const COMMIT: &str = "1111111111111111111111111111111111111111";

    #[test]
    fn find_dep_on_disk_finds_what_cmod_vendor_writes() {
        let tmp = TempDir::new().unwrap();
        let vendor = tmp.path().join("vendor");
        let deps = tmp.path().join("deps");
        let name = "github.com/acme/dep";
        assert_eq!(find_dep_on_disk(&vendor, &deps, name), None);

        let (checkout_dir, _) = checkout(&deps);
        assert_eq!(find_dep_on_disk(&vendor, &deps, name), Some(checkout_dir));

        // Laid out by hand, then as `cmod vendor` writes it: vendor/ wins.
        let by_hand = vendor.join("github.com/acme/dep");
        std::fs::create_dir_all(&by_hand).unwrap();
        assert_eq!(find_dep_on_disk(&vendor, &deps, name), Some(by_hand));
        let dir = vendored(&vendor, COMMIT);
        assert_eq!(find_dep_on_disk(&vendor, &deps, name), Some(dir));
    }

    #[test]
    fn verify_vendored_checks_the_commit_and_every_file() {
        let tmp = TempDir::new().unwrap();
        let dir = vendored(tmp.path(), COMMIT);
        assert!(verify_vendored(&git_package(COMMIT), &dir).is_ok());
        // A package without a locked commit is taken as it is.
        let mut unlocked = git_package(COMMIT);
        unlocked.commit = None;
        assert!(verify_vendored(&unlocked, &dir).is_ok());

        // Locked at another commit: stale.
        let other = "2222222222222222222222222222222222222222";
        let err = verify_vendored(&git_package(other), &dir).unwrap_err();
        assert!(
            err.to_string()
                .contains("is commit 11111111, but cmod.lock locks 22222222"),
            "{}",
            err
        );

        // Build outputs beside the files are fine; a changed or deleted file is not.
        std::fs::create_dir_all(dir.join("build/debug")).unwrap();
        std::fs::write(dir.join("build/debug/libdep.a"), "x").unwrap();
        assert!(verify_vendored(&git_package(COMMIT), &dir).is_ok());
        std::fs::write(dir.join("src/lib.cppm"), "export module evil;\n").unwrap();
        std::fs::remove_file(dir.join("cmod.toml")).unwrap();
        let err = verify_vendored(&git_package(COMMIT), &dir).unwrap_err();
        assert!(
            matches!(err, CmodError::SecurityViolation { .. }),
            "{:?}",
            err
        );
        assert!(
            err.to_string()
                .contains("cmod.toml, src/lib.cppm changed or missing"),
            "{}",
            err
        );
    }

    /// A vendored git checkout (as older `cmod vendor` left them) must be
    /// at the locked commit.
    #[test]
    fn verify_vendored_checks_the_head_of_a_vendored_checkout() {
        let tmp = TempDir::new().unwrap();
        let (dir, commit) = checkout(&tmp.path().join("vendor"));
        assert!(verify_vendored(&git_package(&commit), &dir).is_ok());
        assert!(verify_vendored(&git_package(COMMIT), &dir).is_err());
    }

    #[test]
    fn modified_tracked_files_ignores_untracked_files() {
        let tmp = TempDir::new().unwrap();
        let (dir, _) = checkout(tmp.path());
        std::fs::create_dir_all(dir.join("build")).unwrap();
        std::fs::write(dir.join("build/out.o"), "x").unwrap();
        assert!(modified_tracked_files(&dir).unwrap().is_empty());
        std::fs::write(dir.join("cmod.toml"), "changed").unwrap();
        assert_eq!(modified_tracked_files(&dir).unwrap(), vec!["cmod.toml"]);
    }

    /// Offline, a missing or stale checkout is an error: nothing is fetched,
    /// and the stale checkout is left where it is.
    #[test]
    fn ensure_dep_on_disk_offline_never_fetches_or_removes() {
        let tmp = TempDir::new().unwrap();
        let deps = tmp.path().join("deps");
        let vendor = tmp.path().join("vendor");
        let shell = cmod_core::shell::Shell::from_write(
            Box::new(std::io::sink()),
            cmod_core::shell::Verbosity::Quiet,
        );
        let mut pkg = git_package(COMMIT);
        pkg.repo = Some("https://github.com/acme/dep".to_string());

        let err = ensure_dep_on_disk(&pkg, &vendor, &deps, true, &shell).unwrap_err();
        assert!(
            err.to_string().contains("--offline does not fetch"),
            "{}",
            err
        );
        assert!(!deps.exists());

        let (dir, _) = checkout(&deps);
        assert!(ensure_dep_on_disk(&pkg, &vendor, &deps, true, &shell).is_err());
        assert!(dir.join("cmod.toml").exists(), "stale checkout was removed");

        // Vendored at the locked commit, it is used, offline or not.
        let vendored_dir = vendored(&vendor, COMMIT);
        assert_eq!(
            ensure_dep_on_disk(&pkg, &vendor, &deps, true, &shell).unwrap(),
            Some(vendored_dir)
        );
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

    #[test]
    fn is_fetched_excludes_path_dependencies() {
        let mut pkg = git_package("0123abcd");
        pkg.repo = Some("https://github.com/acme/dep".into());
        assert!(is_fetched(&pkg));
        pkg.repo = Some("path:libs/dep".into());
        assert!(!is_fetched(&pkg));
        pkg.repo = Some("https://github.com/acme/dep".into());
        pkg.commit = None;
        assert!(!is_fetched(&pkg));
    }

    #[test]
    fn linkable_artifacts_are_sorted_and_prefer_archives() {
        let tmp = TempDir::new().unwrap();
        let obj = tmp.path().join("obj");
        std::fs::create_dir_all(&obj).unwrap();
        for name in ["c.o", "a.o", "b.o", "a.o.d"] {
            std::fs::write(obj.join(name), "").unwrap();
        }
        assert_eq!(
            linkable_artifacts(tmp.path()),
            vec![obj.join("a.o"), obj.join("b.o"), obj.join("c.o")]
        );

        for name in ["libz.a", "liba.a"] {
            std::fs::write(tmp.path().join(name), "").unwrap();
        }
        assert_eq!(
            linkable_artifacts(tmp.path()),
            vec![tmp.path().join("liba.a"), tmp.path().join("libz.a")]
        );
        assert!(linkable_artifacts(&tmp.path().join("missing")).is_empty());
    }
}
