use std::collections::BTreeMap;
use std::path::PathBuf;

use semver::Version;

use cmod_core::error::CmodError;
use cmod_core::lockfile::{Lockfile, LockedPackage, LockedToolchain};
use cmod_core::manifest::{Dependency, Manifest};

use crate::features::{resolve_features, should_include_dep};
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
        self.resolve_with_features(manifest, existing_lock, locked, offline, &[], false)
    }

    /// Resolve all dependencies with feature flags.
    ///
    /// `requested_features` are explicitly enabled features (from --features).
    /// `no_default_features` disables the `[features] default = [...]` set.
    pub fn resolve_with_features(
        &self,
        manifest: &Manifest,
        existing_lock: Option<&Lockfile>,
        locked: bool,
        offline: bool,
        requested_features: &[String],
        no_default_features: bool,
    ) -> Result<Lockfile, CmodError> {
        if locked {
            if let Some(lock) = existing_lock {
                self.validate_lockfile(manifest, lock)?;
                return Ok(lock.clone());
            } else {
                return Err(CmodError::LockfileNotFound);
            }
        }

        // Resolve features to determine which optional deps are activated
        let resolved_features =
            resolve_features(manifest, requested_features, no_default_features)?;

        let mut lockfile = Lockfile::new();
        let mut resolved = BTreeMap::new();

        // Resolve each dependency, filtering optional deps that are not activated
        for (name, dep) in &manifest.dependencies {
            if !should_include_dep(name, dep, &resolved_features) {
                continue;
            }
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
            // Collect features activated for this dep
            let dep_features: Vec<String> = resolved_features
                .dep_features
                .get(name)
                .map(|fs| fs.iter().cloned().collect())
                .unwrap_or_default();

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
                features: dep_features,
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

#[cfg(test)]
mod tests {
    use super::*;
    use cmod_core::manifest::{DetailedDependency, Package};
    use std::collections::BTreeMap;

    fn minimal_manifest() -> Manifest {
        Manifest {
            package: Package {
                name: "test".to_string(),
                version: "0.1.0".to_string(),
                edition: None,
                description: None,
                authors: vec![],
                license: None,
                repository: None,
                homepage: None,
            },
            module: None,
            dependencies: BTreeMap::new(),
            dev_dependencies: BTreeMap::new(),
            build_dependencies: BTreeMap::new(),
            features: BTreeMap::new(),
            compat: None,
            toolchain: None,
            build: None,
            test: None,
            workspace: None,
            cache: None,
            metadata: None,
            security: None,
            publish: None,
            target: BTreeMap::new(),
        }
    }

    #[test]
    fn test_resolve_empty_deps() {
        let tmp = tempfile::TempDir::new().unwrap();
        let resolver = Resolver::new(tmp.path().to_path_buf());
        let manifest = minimal_manifest();

        let lockfile = resolver.resolve(&manifest, None, false, false).unwrap();
        assert!(lockfile.is_empty());
    }

    #[test]
    fn test_resolve_locked_without_lockfile() {
        let tmp = tempfile::TempDir::new().unwrap();
        let resolver = Resolver::new(tmp.path().to_path_buf());
        let manifest = minimal_manifest();

        let result = resolver.resolve(&manifest, None, true, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_locked_with_valid_lockfile() {
        let tmp = tempfile::TempDir::new().unwrap();
        let resolver = Resolver::new(tmp.path().to_path_buf());
        let mut manifest = minimal_manifest();
        manifest.dependencies.insert(
            "test_dep".to_string(),
            Dependency::Simple("^1.0".to_string()),
        );

        let mut lockfile = Lockfile::new();
        lockfile.upsert_package(LockedPackage {
            name: "test_dep".to_string(),
            version: "1.2.0".to_string(),
            source: Some("git".to_string()),
            repo: Some("https://example.com/test_dep".to_string()),
            commit: Some("abc123".to_string()),
            hash: Some("sha256:deadbeef".to_string()),
            toolchain: None,
            targets: BTreeMap::new(),
            deps: vec![],
            features: vec![],
        });

        let result = resolver
            .resolve(&manifest, Some(&lockfile), true, false)
            .unwrap();
        assert_eq!(result.packages.len(), 1);
        assert_eq!(result.packages[0].version, "1.2.0");
    }

    #[test]
    fn test_resolve_locked_with_outdated_lockfile() {
        let tmp = tempfile::TempDir::new().unwrap();
        let resolver = Resolver::new(tmp.path().to_path_buf());
        let mut manifest = minimal_manifest();
        manifest.dependencies.insert(
            "test_dep".to_string(),
            Dependency::Simple("^2.0".to_string()),
        );

        let mut lockfile = Lockfile::new();
        lockfile.upsert_package(LockedPackage {
            name: "test_dep".to_string(),
            version: "1.2.0".to_string(),
            source: None,
            repo: None,
            commit: None,
            hash: None,
            toolchain: None,
            targets: BTreeMap::new(),
            deps: vec![],
            features: vec![],
        });

        let result = resolver.resolve(&manifest, Some(&lockfile), true, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_path_dependency() {
        let tmp = tempfile::TempDir::new().unwrap();
        let resolver = Resolver::new(tmp.path().to_path_buf());
        let mut manifest = minimal_manifest();
        manifest.dependencies.insert(
            "local_lib".to_string(),
            Dependency::Detailed(DetailedDependency {
                version: Some("0.1.0".to_string()),
                git: None,
                branch: None,
                rev: None,
                tag: None,
                path: Some(PathBuf::from("./libs/local")),
                features: vec![],
                optional: false,
                default_features: true,
                workspace: false,
            }),
        );

        let lockfile = resolver.resolve(&manifest, None, false, false).unwrap();
        assert_eq!(lockfile.packages.len(), 1);
        assert_eq!(lockfile.packages[0].name, "local_lib");
    }

    #[test]
    fn test_resolve_offline_fails_for_git_dep() {
        let tmp = tempfile::TempDir::new().unwrap();
        let resolver = Resolver::new(tmp.path().to_path_buf());
        let mut manifest = minimal_manifest();
        manifest.dependencies.insert(
            "github.com/test/dep".to_string(),
            Dependency::Simple("^1.0".to_string()),
        );

        let result = resolver.resolve(&manifest, None, false, true);
        assert!(result.is_err());
    }

    #[test]
    fn test_add_dependency() {
        let tmp = tempfile::TempDir::new().unwrap();
        let resolver = Resolver::new(tmp.path().to_path_buf());
        let mut manifest = minimal_manifest();

        let dep = Dependency::Detailed(DetailedDependency {
            version: Some("0.1.0".to_string()),
            git: None,
            branch: None,
            rev: None,
            tag: None,
            path: Some(PathBuf::from("./local")),
            features: vec![],
            optional: false,
            default_features: true,
            workspace: false,
        });

        let lockfile = resolver
            .add_dependency(&mut manifest, "my_dep".to_string(), dep, None)
            .unwrap();

        assert!(manifest.dependencies.contains_key("my_dep"));
        assert_eq!(lockfile.packages.len(), 1);
    }

    #[test]
    fn test_add_duplicate_dependency() {
        let tmp = tempfile::TempDir::new().unwrap();
        let resolver = Resolver::new(tmp.path().to_path_buf());
        let mut manifest = minimal_manifest();

        manifest.dependencies.insert(
            "existing".to_string(),
            Dependency::Simple("^1.0".to_string()),
        );

        let dep = Dependency::Simple("^2.0".to_string());
        let result =
            resolver.add_dependency(&mut manifest, "existing".to_string(), dep, None);

        assert!(result.is_err());
    }

    #[test]
    fn test_remove_dependency() {
        let mut manifest = minimal_manifest();
        manifest.dependencies.insert(
            "pkg".to_string(),
            Dependency::Simple("^1.0".to_string()),
        );

        Resolver::remove_dependency(&mut manifest, "pkg").unwrap();
        assert!(manifest.dependencies.is_empty());
    }

    #[test]
    fn test_remove_nonexistent_dependency() {
        let mut manifest = minimal_manifest();
        let result = Resolver::remove_dependency(&mut manifest, "nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_reuses_locked_version() {
        let tmp = tempfile::TempDir::new().unwrap();
        let resolver = Resolver::new(tmp.path().to_path_buf());
        let mut manifest = minimal_manifest();
        manifest.dependencies.insert(
            "dep_a".to_string(),
            Dependency::Detailed(DetailedDependency {
                version: Some("0.1.0".to_string()),
                git: None,
                branch: None,
                rev: None,
                tag: None,
                path: Some(PathBuf::from("./a")),
                features: vec![],
                optional: false,
                default_features: true,
                workspace: false,
            }),
        );

        // First resolve
        let lock1 = resolver.resolve(&manifest, None, false, false).unwrap();
        assert_eq!(lock1.packages.len(), 1);

        // Second resolve with existing lock — should reuse
        let lock2 = resolver
            .resolve(&manifest, Some(&lock1), false, false)
            .unwrap();
        assert_eq!(lock2.packages.len(), 1);
        assert_eq!(lock2.packages[0].version, lock1.packages[0].version);
    }

    #[test]
    fn test_resolve_with_features_filters_optional_deps() {
        let tmp = tempfile::TempDir::new().unwrap();
        let resolver = Resolver::new(tmp.path().to_path_buf());
        let mut manifest = minimal_manifest();

        // Add a required dep
        manifest.dependencies.insert(
            "required".to_string(),
            Dependency::Detailed(DetailedDependency {
                version: Some("0.1.0".to_string()),
                git: None,
                branch: None,
                rev: None,
                tag: None,
                path: Some(PathBuf::from("./required")),
                features: vec![],
                optional: false,
                default_features: true,
                workspace: false,
            }),
        );

        // Add an optional dep
        manifest.dependencies.insert(
            "optional_dep".to_string(),
            Dependency::Detailed(DetailedDependency {
                version: Some("0.1.0".to_string()),
                git: None,
                branch: None,
                rev: None,
                tag: None,
                path: Some(PathBuf::from("./optional")),
                features: vec![],
                optional: true,
                default_features: true,
                workspace: false,
            }),
        );

        // Without features — optional dep should be excluded
        let lock = resolver
            .resolve_with_features(&manifest, None, false, false, &[], false)
            .unwrap();
        assert_eq!(lock.packages.len(), 1);
        assert_eq!(lock.packages[0].name, "required");
    }

    #[test]
    fn test_resolve_with_features_includes_activated_optional() {
        let tmp = tempfile::TempDir::new().unwrap();
        let resolver = Resolver::new(tmp.path().to_path_buf());
        let mut manifest = minimal_manifest();

        // Add an optional dep
        manifest.dependencies.insert(
            "optional_dep".to_string(),
            Dependency::Detailed(DetailedDependency {
                version: Some("0.1.0".to_string()),
                git: None,
                branch: None,
                rev: None,
                tag: None,
                path: Some(PathBuf::from("./optional")),
                features: vec![],
                optional: true,
                default_features: true,
                workspace: false,
            }),
        );

        // Add feature that activates optional dep
        let mut features = BTreeMap::new();
        features.insert(
            "extra".to_string(),
            vec!["dep:optional_dep".to_string()],
        );
        manifest.features = features;

        // With --features extra — optional dep should be included
        let lock = resolver
            .resolve_with_features(
                &manifest,
                None,
                false,
                false,
                &["extra".to_string()],
                false,
            )
            .unwrap();
        assert_eq!(lock.packages.len(), 1);
        assert_eq!(lock.packages[0].name, "optional_dep");
    }

    #[test]
    fn test_resolve_stores_features_in_lockfile() {
        let tmp = tempfile::TempDir::new().unwrap();
        let resolver = Resolver::new(tmp.path().to_path_buf());
        let mut manifest = minimal_manifest();

        // Add a dep with features requested
        manifest.dependencies.insert(
            "math".to_string(),
            Dependency::Detailed(DetailedDependency {
                version: Some("0.1.0".to_string()),
                git: None,
                branch: None,
                rev: None,
                tag: None,
                path: Some(PathBuf::from("./math")),
                features: vec!["simd".to_string()],
                optional: false,
                default_features: true,
                workspace: false,
            }),
        );

        let lock = resolver
            .resolve_with_features(&manifest, None, false, false, &[], false)
            .unwrap();
        assert_eq!(lock.packages.len(), 1);
        assert!(lock.packages[0].features.contains(&"simd".to_string()));
    }
}
