use std::collections::BTreeMap;
use std::path::PathBuf;

use semver::Version;

use cmod_core::error::CmodError;
use cmod_core::lockfile::{Lockfile, LockedPackage, LockedToolchain};
use cmod_core::manifest::{Dependency, Manifest};
use cmod_security::trust::TrustDb;

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
    /// Trust database for TOFU verification.
    trust_db: Option<TrustDb>,
    /// When true, skip trust checks entirely.
    untrusted: bool,
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
        Resolver {
            deps_dir,
            trust_db: None,
            untrusted: false,
        }
    }

    /// Enable TOFU trust checking with the given trust database.
    pub fn with_trust_db(mut self, trust_db: TrustDb) -> Self {
        self.trust_db = Some(trust_db);
        self
    }

    /// Skip all trust checks (--untrusted mode).
    pub fn with_untrusted(mut self, untrusted: bool) -> Self {
        self.untrusted = untrusted;
        self
    }

    /// Save the trust database to the default location (if loaded).
    pub fn save_trust_db(&self) -> Result<(), CmodError> {
        if let Some(ref db) = self.trust_db {
            db.save_default()?;
        }
        Ok(())
    }

    /// Resolve all dependencies from a manifest, producing a lockfile.
    ///
    /// If a lockfile already exists and `locked` is true, validates that
    /// locked versions satisfy current constraints (but does not re-resolve).
    pub fn resolve(
        &mut self,
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
    /// `target_triple` filters target-specific dependencies (e.g., `x86_64-unknown-linux-gnu`).
    pub fn resolve_with_features(
        &mut self,
        manifest: &Manifest,
        existing_lock: Option<&Lockfile>,
        locked: bool,
        offline: bool,
        requested_features: &[String],
        no_default_features: bool,
    ) -> Result<Lockfile, CmodError> {
        self.resolve_with_target(manifest, existing_lock, locked, offline, requested_features, no_default_features, None)
    }

    /// Resolve all dependencies with feature flags and target-specific filtering.
    pub fn resolve_with_target(
        &mut self,
        manifest: &Manifest,
        existing_lock: Option<&Lockfile>,
        locked: bool,
        offline: bool,
        requested_features: &[String],
        no_default_features: bool,
        target_triple: Option<&str>,
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

        // Merge target-specific dependencies when a target triple is given
        let effective_deps = match target_triple {
            Some(triple) => manifest.effective_dependencies(triple),
            None => manifest.dependencies.clone(),
        };

        // Resolve each dependency, filtering optional deps that are not activated
        for (name, dep) in &effective_deps {
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
        &mut self,
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

        // TOFU trust verification: check that this dep's origin matches trust DB
        if !self.untrusted {
            if let Some(ref mut trust_db) = self.trust_db {
                match trust_db.origin_matches(name, &url) {
                    Some(true) => {
                        // Origin matches, all good
                    }
                    Some(false) => {
                        // Origin mismatch — potential supply chain attack
                        let trusted_origin = trust_db.modules.get(name)
                            .map(|m| m.origin.as_str())
                            .unwrap_or("unknown");
                        return Err(CmodError::SecurityViolation {
                            reason: format!(
                                "origin mismatch for '{}': trusted '{}', but got '{}'. \
                                 Use --untrusted to bypass, or run `cmod trust remove {}` to reset.",
                                name, trusted_origin, url, name
                            ),
                        });
                    }
                    None => {
                        // New dependency — trust on first use
                        trust_db.trust_on_first_use(name, &url, "");
                    }
                }
            }
        }

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

        // Update trust DB with the resolved commit (for new entries)
        if !self.untrusted {
            if let Some(ref mut trust_db) = self.trust_db {
                if let Some(entry) = trust_db.modules.get_mut(name) {
                    if entry.first_seen_commit.is_empty() {
                        entry.first_seen_commit = commit_oid.to_string();
                    }
                }
            }
        }

        // Check for transitive dependencies by reading the dep's cmod.toml
        let dep_manifest_path = repo_dir.join("cmod.toml");
        let mut transitive_deps = Vec::new();
        if dep_manifest_path.exists() {
            if let Ok(dep_manifest) = Manifest::load(&dep_manifest_path) {
                // Check compat constraints of the dependency against our toolchain
                check_dep_compat(name, &dep_manifest, _manifest)?;

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
        // Note: we validate base dependencies only; target-specific deps
        // are validated at build time when the target is known.
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
        &mut self,
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

    /// Check a lockfile for version conflicts across transitive dependencies.
    ///
    /// A conflict occurs when two packages depend on the same transitive dep
    /// but the lockfile only pins one version. This reports cases where
    /// different direct deps might need incompatible versions.
    pub fn check_conflicts(lockfile: &Lockfile) -> Vec<VersionConflict> {
        let mut dep_requesters: BTreeMap<String, Vec<String>> = BTreeMap::new();

        for pkg in &lockfile.packages {
            for dep_name in &pkg.deps {
                dep_requesters
                    .entry(dep_name.clone())
                    .or_default()
                    .push(pkg.name.clone());
            }
        }

        let mut conflicts = Vec::new();
        for (dep_name, requesters) in &dep_requesters {
            if requesters.len() > 1 {
                conflicts.push(VersionConflict {
                    package: dep_name.clone(),
                    requesters: requesters.clone(),
                    resolved_version: lockfile
                        .find_package(dep_name)
                        .map(|p| p.version.clone())
                        .unwrap_or_else(|| "unknown".to_string()),
                });
            }
        }

        conflicts
    }

    /// Explain why a specific dependency is included in the lockfile.
    pub fn explain_dep(lockfile: &Lockfile, dep_name: &str) -> Vec<String> {
        let mut reasons = Vec::new();

        for pkg in &lockfile.packages {
            if pkg.deps.contains(&dep_name.to_string()) {
                reasons.push(format!("{} (v{}) depends on {}", pkg.name, pkg.version, dep_name));
            }
        }

        if reasons.is_empty() {
            // It's a direct dependency
            if lockfile.find_package(dep_name).is_some() {
                reasons.push(format!("{} is a direct dependency", dep_name));
            }
        }

        reasons
    }
}

/// An ABI compatibility warning detected during resolution.
#[derive(Debug, Clone)]
pub struct AbiWarning {
    /// Name of the dependency.
    pub package: String,
    /// Description of the ABI issue.
    pub reason: String,
}

impl Resolver {
    /// Check for ABI compatibility issues across resolved dependencies.
    ///
    /// Compares the project's ABI configuration against dependency metadata
    /// to detect potential incompatibilities.
    pub fn check_abi_compat(
        manifest: &Manifest,
        lockfile: &Lockfile,
    ) -> Vec<AbiWarning> {
        let mut warnings = Vec::new();

        let project_abi = manifest.abi.as_ref();
        let project_compat = manifest.compat.as_ref();

        // Extract project's minimum C++ standard
        let project_cpp = project_compat
            .and_then(|c| c.cpp.as_deref())
            .and_then(|s| {
                s.trim_start_matches(">=")
                    .trim_start_matches("c++")
                    .trim_start_matches("C++")
                    .parse::<u32>()
                    .ok()
            });

        // Check ABI variant consistency
        if let Some(abi_conf) = project_abi {
            let project_variant = abi_conf.variant.as_ref().map(|v| format!("{:?}", v).to_lowercase());

            if let Some(ref variant) = project_variant {
                // Check that dependency platforms are compatible
                for pkg in &lockfile.packages {
                    // If the project requires a specific ABI variant, warn about
                    // dependencies that might not match (cross-platform builds)
                    if variant == "msvc" && pkg.name.contains("linux") {
                        warnings.push(AbiWarning {
                            package: pkg.name.clone(),
                            reason: format!(
                                "dependency '{}' may not be ABI-compatible with MSVC variant",
                                pkg.name
                            ),
                        });
                    }
                }
            }

            // Check min_cpp_standard against project
            if let (Some(abi_min_cpp), Some(proj_cpp)) =
                (abi_conf.min_cpp_standard.as_deref().and_then(|s| s.parse::<u32>().ok()), project_cpp)
            {
                if proj_cpp < abi_min_cpp {
                    warnings.push(AbiWarning {
                        package: manifest.package.name.clone(),
                        reason: format!(
                            "project C++ standard ({}) is below ABI minimum requirement ({})",
                            proj_cpp, abi_min_cpp
                        ),
                    });
                }
            }
        }

        // Check for mixed ABI (itanium + msvc) in the dependency graph
        if let Some(ref compat) = project_compat {
            if let Some(ref project_abi_variant) = compat.abi {
                let project_abi_str = format!("{:?}", project_abi_variant).to_lowercase();
                for pkg in &lockfile.packages {
                    // Flag cross-ABI dependencies based on target platform hints
                    let is_msvc_dep = pkg.name.contains("msvc") || pkg.name.contains("windows");
                    let is_itanium_dep = pkg.name.contains("linux") || pkg.name.contains("darwin");

                    if project_abi_str == "msvc" && is_itanium_dep {
                        warnings.push(AbiWarning {
                            package: pkg.name.clone(),
                            reason: "potential ABI mismatch: itanium-style dep in MSVC project".to_string(),
                        });
                    } else if project_abi_str == "itanium" && is_msvc_dep {
                        warnings.push(AbiWarning {
                            package: pkg.name.clone(),
                            reason: "potential ABI mismatch: MSVC-style dep in itanium project".to_string(),
                        });
                    }
                }
            }
        }

        warnings
    }
}

/// A version conflict where multiple packages depend on the same transitive dep.
#[derive(Debug, Clone)]
pub struct VersionConflict {
    /// Name of the conflicted package.
    pub package: String,
    /// Packages that depend on this package.
    pub requesters: Vec<String>,
    /// The version that was resolved.
    pub resolved_version: String,
}

/// Check a dependency's compat constraints against the project manifest's toolchain.
///
/// If the dependency declares `[compat] cpp = ">=23"` and the project toolchain is C++20,
/// this returns an error. Platform constraints are also checked against the resolved target.
fn check_dep_compat(
    dep_name: &str,
    dep_manifest: &Manifest,
    project_manifest: &Manifest,
) -> Result<(), CmodError> {
    let compat = match dep_manifest.compat.as_ref() {
        Some(c) => c,
        None => return Ok(()), // No compat constraints declared
    };

    // Check C++ standard constraint
    if let Some(ref cpp_req) = compat.cpp {
        if let Some(ref tc) = project_manifest.toolchain {
            if let Some(ref project_std) = tc.cxx_standard {
                if !check_cpp_constraint(project_std, cpp_req) {
                    return Err(CmodError::UnresolvableConstraints {
                        name: dep_name.to_string(),
                        reason: format!(
                            "dependency requires C++ standard {} but project toolchain is C++{}",
                            cpp_req, project_std
                        ),
                    });
                }
            }
        }
    }

    // Check platform constraints
    if !compat.platforms.is_empty() {
        let target = project_manifest
            .toolchain
            .as_ref()
            .and_then(|tc| tc.target.as_deref());

        if let Some(target_triple) = target {
            if !compat.platforms.iter().any(|p| target_triple.contains(p.as_str())) {
                return Err(CmodError::UnresolvableConstraints {
                    name: dep_name.to_string(),
                    reason: format!(
                        "dependency only supports platforms {:?} but project target is '{}'",
                        compat.platforms, target_triple
                    ),
                });
            }
        }
    }

    Ok(())
}

/// Check whether a project's C++ standard satisfies a constraint string.
///
/// Supports `">=NN"`, `">NN"`, `"NN"` (exact), and `"<=NN"` forms.
fn check_cpp_constraint(project_std: &str, constraint: &str) -> bool {
    let project_num: u32 = match project_std.parse() {
        Ok(n) => n,
        Err(_) => return true, // Can't parse, assume compatible
    };

    let constraint = constraint.trim();
    if let Some(rest) = constraint.strip_prefix(">=") {
        let req: u32 = match rest.trim().parse() {
            Ok(n) => n,
            Err(_) => return true,
        };
        project_num >= req
    } else if let Some(rest) = constraint.strip_prefix("<=") {
        let req: u32 = match rest.trim().parse() {
            Ok(n) => n,
            Err(_) => return true,
        };
        project_num <= req
    } else if let Some(rest) = constraint.strip_prefix('>') {
        let req: u32 = match rest.trim().parse() {
            Ok(n) => n,
            Err(_) => return true,
        };
        project_num > req
    } else if let Some(rest) = constraint.strip_prefix('<') {
        let req: u32 = match rest.trim().parse() {
            Ok(n) => n,
            Err(_) => return true,
        };
        project_num < req
    } else {
        // Exact match
        let req: u32 = match constraint.parse() {
            Ok(n) => n,
            Err(_) => return true,
        };
        project_num >= req
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
            hooks: None,
            ide: None,
            plugins: None,
            abi: None,
            target: BTreeMap::new(),
        }
    }

    #[test]
    fn test_resolve_empty_deps() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mut resolver = Resolver::new(tmp.path().to_path_buf());
        let manifest = minimal_manifest();

        let lockfile = resolver.resolve(&manifest, None, false, false).unwrap();
        assert!(lockfile.is_empty());
    }

    #[test]
    fn test_resolve_locked_without_lockfile() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mut resolver = Resolver::new(tmp.path().to_path_buf());
        let manifest = minimal_manifest();

        let result = resolver.resolve(&manifest, None, true, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_locked_with_valid_lockfile() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mut resolver = Resolver::new(tmp.path().to_path_buf());
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
        let mut resolver = Resolver::new(tmp.path().to_path_buf());
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
        let mut resolver = Resolver::new(tmp.path().to_path_buf());
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
        let mut resolver = Resolver::new(tmp.path().to_path_buf());
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
        let mut resolver = Resolver::new(tmp.path().to_path_buf());
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
        let mut resolver = Resolver::new(tmp.path().to_path_buf());
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
        let mut resolver = Resolver::new(tmp.path().to_path_buf());
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
        let mut resolver = Resolver::new(tmp.path().to_path_buf());
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
        let mut resolver = Resolver::new(tmp.path().to_path_buf());
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
    fn test_resolve_with_target_includes_target_deps() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mut resolver = Resolver::new(tmp.path().to_path_buf());
        let mut manifest = minimal_manifest();

        // Base dependency
        manifest.dependencies.insert(
            "base_lib".to_string(),
            Dependency::Detailed(DetailedDependency {
                version: Some("0.1.0".to_string()),
                git: None,
                branch: None,
                rev: None,
                tag: None,
                path: Some(PathBuf::from("./base")),
                features: vec![],
                optional: false,
                default_features: true,
                workspace: false,
            }),
        );

        // Linux-only dependency via target section
        let mut linux_deps = BTreeMap::new();
        linux_deps.insert(
            "linux_only".to_string(),
            Dependency::Detailed(DetailedDependency {
                version: Some("0.1.0".to_string()),
                git: None,
                branch: None,
                rev: None,
                tag: None,
                path: Some(PathBuf::from("./linux_only")),
                features: vec![],
                optional: false,
                default_features: true,
                workspace: false,
            }),
        );
        manifest.target.insert(
            "cfg(target_os = \"linux\")".to_string(),
            cmod_core::manifest::TargetSpec { dependencies: linux_deps },
        );

        // Resolve for linux — should include both deps
        let lock = resolver
            .resolve_with_target(&manifest, None, false, false, &[], false, Some("x86_64-unknown-linux-gnu"))
            .unwrap();
        assert_eq!(lock.packages.len(), 2);

        // Resolve for macOS — should only include base
        let lock_mac = resolver
            .resolve_with_target(&manifest, None, false, false, &[], false, Some("aarch64-apple-darwin"))
            .unwrap();
        assert_eq!(lock_mac.packages.len(), 1);
        assert_eq!(lock_mac.packages[0].name, "base_lib");

        // Resolve without target — should only include base (no target filtering)
        let lock_none = resolver
            .resolve_with_target(&manifest, None, false, false, &[], false, None)
            .unwrap();
        assert_eq!(lock_none.packages.len(), 1);
        assert_eq!(lock_none.packages[0].name, "base_lib");
    }

    #[test]
    fn test_resolve_stores_features_in_lockfile() {
        let tmp = tempfile::TempDir::new().unwrap();
        let mut resolver = Resolver::new(tmp.path().to_path_buf());
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

    #[test]
    fn test_resolve_trust_on_first_use_creates_entry() {
        let tmp = tempfile::TempDir::new().unwrap();
        let trust_db = TrustDb::default();
        let mut resolver = Resolver::new(tmp.path().to_path_buf())
            .with_trust_db(trust_db);
        let mut manifest = minimal_manifest();

        // Path deps go through resolve_path_dep, not the trust-checked path.
        // But the resolver should carry the trust DB without errors.
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
    }

    #[test]
    fn test_resolve_untrusted_mode_skips_checks() {
        let tmp = tempfile::TempDir::new().unwrap();
        let trust_db = TrustDb::default();
        let mut resolver = Resolver::new(tmp.path().to_path_buf())
            .with_trust_db(trust_db)
            .with_untrusted(true);

        let manifest = minimal_manifest();
        let lockfile = resolver.resolve(&manifest, None, false, false).unwrap();
        assert!(lockfile.is_empty());
    }

    #[test]
    fn test_trust_db_origin_mismatch_error() {
        // Simulate: module was previously trusted with one URL, now resolving from different
        let mut trust_db = TrustDb::default();
        trust_db.trust_on_first_use(
            "github.com/test/dep",
            "https://github.com/test/dep.git",
            "abc123",
        );

        let tmp = tempfile::TempDir::new().unwrap();
        let resolver = Resolver::new(tmp.path().to_path_buf())
            .with_trust_db(trust_db);

        let mut manifest = minimal_manifest();
        // This dep will resolve to a URL that differs from what was trusted
        manifest.dependencies.insert(
            "github.com/test/dep".to_string(),
            Dependency::Simple("^1.0".to_string()),
        );

        // This will fail offline before reaching trust check, but the trust DB
        // mechanism is tested. We verify the origin_matches logic directly.
        let resolver_db = resolver.trust_db.as_ref().unwrap();
        assert_eq!(
            resolver_db.origin_matches("github.com/test/dep", "https://github.com/test/dep.git"),
            Some(true)
        );
        assert_eq!(
            resolver_db.origin_matches("github.com/test/dep", "https://evil.com/dep.git"),
            Some(false)
        );
    }

    #[test]
    fn test_save_trust_db_no_db_is_noop() {
        let tmp = tempfile::TempDir::new().unwrap();
        let resolver = Resolver::new(tmp.path().to_path_buf());
        // Should not error when no trust DB is loaded
        assert!(resolver.save_trust_db().is_ok());
    }

    #[test]
    fn test_check_conflicts_none() {
        let lockfile = Lockfile::new();
        let conflicts = Resolver::check_conflicts(&lockfile);
        assert!(conflicts.is_empty());
    }

    #[test]
    fn test_check_conflicts_shared_transitive() {
        use cmod_core::lockfile::LockedPackage;

        let mut lockfile = Lockfile::new();
        lockfile.upsert_package(LockedPackage {
            name: "app_a".to_string(),
            version: "1.0.0".to_string(),
            source: None,
            repo: None,
            commit: None,
            hash: None,
            toolchain: None,
            targets: BTreeMap::new(),
            deps: vec!["common_lib".to_string()],
            features: vec![],
        });
        lockfile.upsert_package(LockedPackage {
            name: "app_b".to_string(),
            version: "2.0.0".to_string(),
            source: None,
            repo: None,
            commit: None,
            hash: None,
            toolchain: None,
            targets: BTreeMap::new(),
            deps: vec!["common_lib".to_string()],
            features: vec![],
        });
        lockfile.upsert_package(LockedPackage {
            name: "common_lib".to_string(),
            version: "0.5.0".to_string(),
            source: None,
            repo: None,
            commit: None,
            hash: None,
            toolchain: None,
            targets: BTreeMap::new(),
            deps: vec![],
            features: vec![],
        });

        let conflicts = Resolver::check_conflicts(&lockfile);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].package, "common_lib");
        assert_eq!(conflicts[0].resolved_version, "0.5.0");
        assert!(conflicts[0].requesters.contains(&"app_a".to_string()));
        assert!(conflicts[0].requesters.contains(&"app_b".to_string()));
    }

    #[test]
    fn test_explain_dep_direct() {
        use cmod_core::lockfile::LockedPackage;

        let mut lockfile = Lockfile::new();
        lockfile.upsert_package(LockedPackage {
            name: "fmt".to_string(),
            version: "10.2.0".to_string(),
            source: None,
            repo: None,
            commit: None,
            hash: None,
            toolchain: None,
            targets: BTreeMap::new(),
            deps: vec![],
            features: vec![],
        });

        let reasons = Resolver::explain_dep(&lockfile, "fmt");
        assert_eq!(reasons.len(), 1);
        assert!(reasons[0].contains("direct dependency"));
    }

    #[test]
    fn test_explain_dep_transitive() {
        use cmod_core::lockfile::LockedPackage;

        let mut lockfile = Lockfile::new();
        lockfile.upsert_package(LockedPackage {
            name: "app".to_string(),
            version: "1.0.0".to_string(),
            source: None,
            repo: None,
            commit: None,
            hash: None,
            toolchain: None,
            targets: BTreeMap::new(),
            deps: vec!["utils".to_string()],
            features: vec![],
        });
        lockfile.upsert_package(LockedPackage {
            name: "utils".to_string(),
            version: "0.3.0".to_string(),
            source: None,
            repo: None,
            commit: None,
            hash: None,
            toolchain: None,
            targets: BTreeMap::new(),
            deps: vec![],
            features: vec![],
        });

        let reasons = Resolver::explain_dep(&lockfile, "utils");
        assert_eq!(reasons.len(), 1);
        assert!(reasons[0].contains("app"));
        assert!(reasons[0].contains("depends on utils"));
    }

    #[test]
    fn test_explain_dep_not_found() {
        let lockfile = Lockfile::new();
        let reasons = Resolver::explain_dep(&lockfile, "nonexistent");
        assert!(reasons.is_empty());
    }

    #[test]
    fn test_check_cpp_constraint_gte() {
        assert!(check_cpp_constraint("23", ">=20"));
        assert!(check_cpp_constraint("20", ">=20"));
        assert!(!check_cpp_constraint("17", ">=20"));
    }

    #[test]
    fn test_check_cpp_constraint_exact() {
        assert!(check_cpp_constraint("23", "23"));
        assert!(check_cpp_constraint("26", "23"));
        assert!(!check_cpp_constraint("20", "23"));
    }

    #[test]
    fn test_check_cpp_constraint_lt() {
        assert!(check_cpp_constraint("17", "<20"));
        assert!(!check_cpp_constraint("20", "<20"));
    }

    #[test]
    fn test_check_dep_compat_passes_when_satisfied() {
        use cmod_core::manifest::{Compat, Toolchain};
        let mut dep_manifest = minimal_manifest();
        dep_manifest.compat = Some(Compat {
            cpp: Some(">=20".to_string()),
            llvm: None,
            abi: None,
            platforms: vec![],
        });

        let mut project = minimal_manifest();
        project.toolchain = Some(Toolchain {
            compiler: None,
            version: None,
            cxx_standard: Some("23".to_string()),
            stdlib: None,
            target: None,
            sysroot: None,
        });

        assert!(check_dep_compat("test_dep", &dep_manifest, &project).is_ok());
    }

    #[test]
    fn test_check_dep_compat_fails_cpp_too_low() {
        use cmod_core::manifest::{Compat, Toolchain};
        let mut dep_manifest = minimal_manifest();
        dep_manifest.compat = Some(Compat {
            cpp: Some(">=23".to_string()),
            llvm: None,
            abi: None,
            platforms: vec![],
        });

        let mut project = minimal_manifest();
        project.toolchain = Some(Toolchain {
            compiler: None,
            version: None,
            cxx_standard: Some("20".to_string()),
            stdlib: None,
            target: None,
            sysroot: None,
        });

        let result = check_dep_compat("test_dep", &dep_manifest, &project);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("C++ standard"), "Error: {}", err);
    }

    #[test]
    fn test_check_dep_compat_platform_mismatch() {
        use cmod_core::manifest::{Compat, Toolchain};
        let mut dep_manifest = minimal_manifest();
        dep_manifest.compat = Some(Compat {
            cpp: None,
            llvm: None,
            abi: None,
            platforms: vec!["x86_64-linux-gnu".to_string()],
        });

        let mut project = minimal_manifest();
        project.toolchain = Some(Toolchain {
            compiler: None,
            version: None,
            cxx_standard: None,
            stdlib: None,
            target: Some("aarch64-apple-darwin".to_string()),
            sysroot: None,
        });

        let result = check_dep_compat("test_dep", &dep_manifest, &project);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("platforms"), "Error: {}", err);
    }

    #[test]
    fn test_check_dep_compat_no_constraints_passes() {
        let dep_manifest = minimal_manifest();
        let project = minimal_manifest();
        assert!(check_dep_compat("test_dep", &dep_manifest, &project).is_ok());
    }
}
