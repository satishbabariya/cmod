use std::collections::BTreeMap;
use std::path::PathBuf;

use semver::Version;

use cmod_core::error::CmodError;
use cmod_core::lockfile::{Lockfile, LockedPackage, LockedToolchain};
use cmod_core::manifest::{Dependency, Manifest};

use crate::git;
use crate::version;

/// Dependency resolver.
///
/// Resolves all dependencies declared in a manifest to exact versions,
/// fetching Git repositories and matching version constraints.
pub struct Resolver {
    /// Directory where dependency repos are cloned/fetched.
    deps_dir: PathBuf,
}

/// The result of resolving a single dependency.
#[derive(Debug, Clone)]
pub struct ResolvedDep {
    /// Dependency key (e.g., `github.com/fmtlib/fmt`).
    pub name: String,
    /// Exact resolved version.
    pub version: Version,
    /// Git repo URL.
    pub repo_url: String,
    /// Exact commit OID.
    pub commit: String,
    /// Content hash of the sources at that commit.
    pub hash: String,
    /// Local path where the repo is checked out.
    pub local_path: PathBuf,
    /// Transitive dependencies of this package.
    pub deps: Vec<String>,
}

impl Resolver {
    pub fn new(deps_dir: PathBuf) -> Self {
        Resolver { deps_dir }
    }

    /// Resolve all dependencies from a manifest, producing a lockfile.
    ///
    /// If a lockfile already exists and `locked` is true, validates that
    /// locked versions satisfy current constraints (but does not re-resolve).
    pub fn resolve(
        &self,
        manifest: &Manifest,
        existing_lock: Option<&Lockfile>,
        locked: bool,
        offline: bool,
    ) -> Result<Lockfile, CmodError> {
        if locked {
            if let Some(lock) = existing_lock {
                self.validate_lockfile(manifest, lock)?;
                return Ok(lock.clone());
            } else {
                return Err(CmodError::LockfileNotFound);
            }
        }

        let mut lockfile = Lockfile::new();
        let mut resolved = BTreeMap::new();

        // Resolve each dependency
        for (name, dep) in &manifest.dependencies {
            self.resolve_dep(
                name,
                dep,
                manifest,
                existing_lock,
                offline,
                &mut resolved,
            )?;
        }

        // Build lockfile from resolved deps
        for (name, dep) in &resolved {
            lockfile.upsert_package(LockedPackage {
                name: name.clone(),
                version: dep.version.to_string(),
                source: Some("git".to_string()),
                repo: Some(dep.repo_url.clone()),
                commit: Some(dep.commit.clone()),
                hash: Some(dep.hash.clone()),
                toolchain: manifest.toolchain.as_ref().map(|tc| LockedToolchain {
                    compiler: tc.compiler.as_ref().map(|c| c.to_string()),
                    version: tc.version.clone(),
                    stdlib: tc.stdlib.clone(),
                    target: tc.target.clone(),
                }),
                targets: BTreeMap::new(),
                deps: dep.deps.clone(),
            });
        }

        Ok(lockfile)
    }

    /// Resolve a single dependency.
    fn resolve_dep(
        &self,
        name: &str,
        dep: &Dependency,
        _manifest: &Manifest,
        existing_lock: Option<&Lockfile>,
        offline: bool,
        resolved: &mut BTreeMap<String, ResolvedDep>,
    ) -> Result<(), CmodError> {
        // Skip if already resolved (handles diamond dependencies)
        if resolved.contains_key(name) {
            return Ok(());
        }

        // Check if this is a path dependency
        if dep.is_path() {
            return self.resolve_path_dep(name, dep, resolved);
        }

        let url = Manifest::resolve_dep_url(name, dep);
        let repo_dir = self.dep_repo_dir(name);

        // Check if we have a locked version we can reuse
        if let Some(lock) = existing_lock {
            if let Some(locked_pkg) = lock.find_package(name) {
                if let Some(version_req_str) = dep.version_req() {
                    let req = version::parse_version_req(version_req_str)?;
                    if let Ok(locked_ver) = version::parse_version(&locked_pkg.version) {
                        if req.matches(&locked_ver) {
                            // Locked version still satisfies constraint, reuse it
                            resolved.insert(
                                name.to_string(),
                                ResolvedDep {
                                    name: name.to_string(),
                                    version: locked_ver,
                                    repo_url: url,
                                    commit: locked_pkg
                                        .commit
                                        .clone()
                                        .unwrap_or_default(),
                                    hash: locked_pkg.hash.clone().unwrap_or_default(),
                                    local_path: repo_dir,
                                    deps: locked_pkg.deps.clone(),
                                },
                            );
                            return Ok(());
                        }
                    }
                }
            }
        }

        if offline {
            return Err(CmodError::GitError {
                reason: format!(
                    "cannot fetch '{}' in offline mode; no cached version available",
                    name
                ),
            });
        }

        // Fetch or clone the repository
        let repo = git::fetch_repo(&url, &repo_dir)?;

        // Determine the version based on dep specification
        let (resolved_version, commit_oid) = match dep {
            Dependency::Detailed(d) if d.rev.is_some() => {
                let rev = d.rev.as_ref().unwrap();
                let oid = git::resolve_commit(&repo, rev)?;
                let date = git::commit_date(&repo, oid)?;
                let pv = version::pseudo_version(&date, &git::short_hash(&oid));
                let ver = Version::parse(&pv).map_err(|e| CmodError::UnresolvableConstraints {
                    name: name.to_string(),
                    reason: format!("pseudo-version parse error: {}", e),
                })?;
                (ver, oid)
            }
            Dependency::Detailed(d) if d.branch.is_some() => {
                let branch = d.branch.as_ref().unwrap();
                let oid = git::resolve_branch(&repo, branch)?;
                let date = git::commit_date(&repo, oid)?;
                let pv = version::pseudo_version(&date, &git::short_hash(&oid));
                let ver = Version::parse(&pv).map_err(|e| CmodError::UnresolvableConstraints {
                    name: name.to_string(),
                    reason: format!("pseudo-version parse error: {}", e),
                })?;
                (ver, oid)
            }
            _ => {
                // Resolve by version constraint against tags
                let version_req_str = dep.version_req().ok_or_else(|| {
                    CmodError::UnresolvableConstraints {
                        name: name.to_string(),
                        reason: "no version constraint specified".to_string(),
                    }
                })?;
                let req = version::parse_version_req(version_req_str)?;
                let tags = git::list_version_tags(&repo)?;
                let available: Vec<Version> = tags.iter().map(|(v, _)| v.clone()).collect();

                let best = version::resolve_best_version(&available, &req).ok_or_else(|| {
                    CmodError::UnresolvableConstraints {
                        name: name.to_string(),
                        reason: format!(
                            "no version matching '{}' found (available: {:?})",
                            version_req_str, available
                        ),
                    }
                })?;

                let oid = tags
                    .iter()
                    .find(|(v, _)| v == &best)
                    .map(|(_, oid)| *oid)
                    .unwrap();

                (best, oid)
            }
        };

        // Compute content hash
        let content_hash = git::content_hash_at_commit(&repo, commit_oid)?;

        // Checkout the resolved commit
        git::checkout_commit(&repo, commit_oid)?;

        // Check for transitive dependencies by reading the dep's cmod.toml
        let dep_manifest_path = repo_dir.join("cmod.toml");
        let mut transitive_deps = Vec::new();
        if dep_manifest_path.exists() {
            if let Ok(dep_manifest) = Manifest::load(&dep_manifest_path) {
                for (trans_name, trans_dep) in &dep_manifest.dependencies {
                    transitive_deps.push(trans_name.clone());
                    // Recursively resolve transitive dependencies
                    self.resolve_dep(
                        trans_name,
                        trans_dep,
                        &dep_manifest,
                        existing_lock,
                        offline,
                        resolved,
                    )?;
                }
            }
        }

        resolved.insert(
            name.to_string(),
            ResolvedDep {
                name: name.to_string(),
                version: resolved_version,
                repo_url: url,
                commit: commit_oid.to_string(),
                hash: content_hash,
                local_path: repo_dir,
                deps: transitive_deps,
            },
        );

        Ok(())
    }

    /// Resolve a path dependency (local, no Git).
    fn resolve_path_dep(
        &self,
        name: &str,
        dep: &Dependency,
        resolved: &mut BTreeMap<String, ResolvedDep>,
    ) -> Result<(), CmodError> {
        let path = match dep {
            Dependency::Detailed(d) => d.path.as_ref().unwrap().clone(),
            _ => unreachable!(),
        };

        let version_str = dep.version_req().unwrap_or("0.0.0");
        let version = version::parse_version(version_str).unwrap_or(Version::new(0, 0, 0));

        resolved.insert(
            name.to_string(),
            ResolvedDep {
                name: name.to_string(),
                version,
                repo_url: format!("path:{}", path.display()),
                commit: "local".to_string(),
                hash: "local".to_string(),
                local_path: path,
                deps: vec![],
            },
        );

        Ok(())
    }

    /// Validate that a lockfile satisfies the current manifest's constraints.
    fn validate_lockfile(
        &self,
        manifest: &Manifest,
        lockfile: &Lockfile,
    ) -> Result<(), CmodError> {
        for (name, dep) in &manifest.dependencies {
            let locked = lockfile.find_package(name).ok_or_else(|| {
                CmodError::LockfileOutdated
            })?;

            if let Some(req_str) = dep.version_req() {
                let req = version::parse_version_req(req_str)?;
                let locked_ver = version::parse_version(&locked.version)?;
                if !req.matches(&locked_ver) {
                    return Err(CmodError::LockfileOutdated);
                }
            }
        }

        Ok(())
    }

    /// Compute the local directory for a dependency's repo clone.
    fn dep_repo_dir(&self, name: &str) -> PathBuf {
        // Sanitize the name for filesystem use
        let sanitized = name.replace('/', "_").replace('\\', "_");
        self.deps_dir.join(sanitized)
    }

    /// Add a single dependency to an existing manifest and re-resolve.
    pub fn add_dependency(
        &self,
        manifest: &mut Manifest,
        dep_key: String,
        dep: Dependency,
        existing_lock: Option<&Lockfile>,
    ) -> Result<Lockfile, CmodError> {
        if manifest.dependencies.contains_key(&dep_key) {
            return Err(CmodError::DependencyAlreadyExists { name: dep_key });
        }

        manifest.dependencies.insert(dep_key, dep);
        self.resolve(manifest, existing_lock, false, false)
    }

    /// Remove a dependency from the manifest.
    pub fn remove_dependency(
        manifest: &mut Manifest,
        dep_key: &str,
    ) -> Result<(), CmodError> {
        if manifest.dependencies.remove(dep_key).is_none() {
            return Err(CmodError::DependencyNotFound {
                name: dep_key.to_string(),
            });
        }
        Ok(())
    }
}
