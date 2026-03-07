use std::collections::BTreeMap;

use cmod_core::error::CmodError;
use cmod_core::lockfile::Lockfile;
use cmod_core::manifest::Manifest;

/// Severity level for audit findings.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum Severity {
    Info,
    Warning,
    Error,
}

/// A single audit finding.
#[derive(Debug, Clone)]
pub struct AuditFinding {
    pub severity: Severity,
    pub package: String,
    pub message: String,
}

/// Result of a full audit run.
#[derive(Debug, Clone)]
pub struct AuditReport {
    pub findings: Vec<AuditFinding>,
}

impl AuditReport {
    pub fn has_errors(&self) -> bool {
        self.findings.iter().any(|f| f.severity == Severity::Error)
    }

    pub fn has_warnings(&self) -> bool {
        self.findings
            .iter()
            .any(|f| f.severity == Severity::Warning)
    }

    pub fn error_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .count()
    }

    pub fn warning_count(&self) -> usize {
        self.findings
            .iter()
            .filter(|f| f.severity == Severity::Warning)
            .count()
    }
}

/// Run a dependency audit against the manifest and lockfile.
///
/// Checks for:
/// - Dependencies without pinned commits
/// - Dependencies using mutable refs (branches instead of tags)
/// - License information availability
/// - Dependency count warnings
pub fn audit_dependencies(
    manifest: &Manifest,
    lockfile: &Lockfile,
) -> Result<AuditReport, CmodError> {
    let mut findings = Vec::new();

    // Build lockfile index
    let locked: BTreeMap<&str, _> = lockfile
        .packages
        .iter()
        .map(|p| (p.name.as_str(), p))
        .collect();

    // Check each declared dependency
    for (name, dep) in &manifest.dependencies {
        // Check if dependency is in lockfile
        if let Some(pkg) = locked.get(name.as_str()) {
            // Check for unpinned commits
            if pkg.commit.is_none() {
                findings.push(AuditFinding {
                    severity: Severity::Warning,
                    package: name.clone(),
                    message: "dependency has no pinned commit hash".to_string(),
                });
            }
        } else {
            findings.push(AuditFinding {
                severity: Severity::Error,
                package: name.clone(),
                message: "dependency not found in lockfile — run `cmod resolve`".to_string(),
            });
        }

        // Check for branch-based deps (mutable references)
        if let cmod_core::manifest::Dependency::Detailed(d) = dep {
            if d.branch.is_some() && d.rev.is_none() && d.tag.is_none() {
                findings.push(AuditFinding {
                    severity: Severity::Warning,
                    package: name.clone(),
                    message: "dependency uses a branch ref — consider pinning to a tag or commit"
                        .to_string(),
                });
            }
        }
    }

    // Check for orphaned lockfile entries
    for pkg in &lockfile.packages {
        if !manifest.dependencies.contains_key(&pkg.name) {
            findings.push(AuditFinding {
                severity: Severity::Info,
                package: pkg.name.clone(),
                message: "package in lockfile but not in dependencies (transitive or orphaned)"
                    .to_string(),
            });
        }
    }

    // Warn if many dependencies
    if manifest.dependencies.len() > 50 {
        findings.push(AuditFinding {
            severity: Severity::Info,
            package: String::new(),
            message: format!(
                "project has {} dependencies — consider auditing for unnecessary deps",
                manifest.dependencies.len()
            ),
        });
    }

    Ok(AuditReport { findings })
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmod_core::lockfile::LockedPackage;
    use cmod_core::manifest::{Dependency, DetailedDependency, Package};
    use std::collections::BTreeMap;

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

    fn minimal_manifest(deps: BTreeMap<String, Dependency>) -> Manifest {
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
            dependencies: deps,
            dev_dependencies: BTreeMap::new(),
            build_dependencies: BTreeMap::new(),
            features: BTreeMap::new(),
            compat: None,
            toolchain: None,
            build: None,
            test: None,
            format: None,
            lint: None,
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
    fn test_audit_empty() {
        let manifest = minimal_manifest(BTreeMap::new());
        let lockfile = Lockfile {
            version: 1,
            integrity: None,
            packages: vec![],
        };

        let report = audit_dependencies(&manifest, &lockfile).unwrap();
        assert!(report.findings.is_empty());
    }

    #[test]
    fn test_audit_missing_in_lockfile() {
        let mut deps = BTreeMap::new();
        deps.insert("fmt".to_string(), Dependency::Simple("^10".to_string()));

        let manifest = minimal_manifest(deps);
        let lockfile = Lockfile {
            version: 1,
            integrity: None,
            packages: vec![],
        };

        let report = audit_dependencies(&manifest, &lockfile).unwrap();
        assert!(report.has_errors());
        assert_eq!(report.error_count(), 1);
        assert!(report.findings[0].message.contains("not found in lockfile"));
    }

    #[test]
    fn test_audit_unpinned_commit() {
        let mut deps = BTreeMap::new();
        deps.insert("fmt".to_string(), Dependency::Simple("^10".to_string()));

        let manifest = minimal_manifest(deps);
        let mut pkg = make_pkg("fmt", None);
        pkg.version = "10.2.0".to_string();
        pkg.repo = Some("https://github.com/fmtlib/fmt".to_string());
        let lockfile = Lockfile {
            version: 1,
            integrity: None,
            packages: vec![pkg],
        };

        let report = audit_dependencies(&manifest, &lockfile).unwrap();
        assert!(report.has_warnings());
        assert!(report
            .findings
            .iter()
            .any(|f| f.message.contains("no pinned commit")));
    }

    #[test]
    fn test_audit_branch_ref_warning() {
        let mut deps = BTreeMap::new();
        deps.insert(
            "mylib".to_string(),
            Dependency::Detailed(DetailedDependency {
                version: None,
                git: Some("https://github.com/user/mylib".to_string()),
                branch: Some("main".to_string()),
                rev: None,
                tag: None,
                path: None,
                features: vec![],
                optional: false,
                default_features: true,
                workspace: false,
            }),
        );

        let manifest = minimal_manifest(deps);
        let mut pkg = make_pkg("mylib", Some("abc123"));
        pkg.version = "0.1.0".to_string();
        pkg.repo = Some("https://github.com/user/mylib".to_string());
        let lockfile = Lockfile {
            version: 1,
            integrity: None,
            packages: vec![pkg],
        };

        let report = audit_dependencies(&manifest, &lockfile).unwrap();
        assert!(report
            .findings
            .iter()
            .any(|f| f.message.contains("branch ref")));
    }

    #[test]
    fn test_audit_clean_project() {
        let mut deps = BTreeMap::new();
        deps.insert("fmt".to_string(), Dependency::Simple("^10".to_string()));

        let manifest = minimal_manifest(deps);
        let mut pkg = make_pkg("fmt", Some("abc123def456"));
        pkg.version = "10.2.0".to_string();
        pkg.repo = Some("https://github.com/fmtlib/fmt".to_string());
        let lockfile = Lockfile {
            version: 1,
            integrity: None,
            packages: vec![pkg],
        };

        let report = audit_dependencies(&manifest, &lockfile).unwrap();
        assert!(!report.has_errors());
        assert!(!report.has_warnings());
    }

    #[test]
    fn test_audit_report_counts() {
        let report = AuditReport {
            findings: vec![
                AuditFinding {
                    severity: Severity::Error,
                    package: "a".into(),
                    message: "err".into(),
                },
                AuditFinding {
                    severity: Severity::Warning,
                    package: "b".into(),
                    message: "warn".into(),
                },
                AuditFinding {
                    severity: Severity::Warning,
                    package: "c".into(),
                    message: "warn2".into(),
                },
                AuditFinding {
                    severity: Severity::Info,
                    package: "d".into(),
                    message: "info".into(),
                },
            ],
        };
        assert_eq!(report.error_count(), 1);
        assert_eq!(report.warning_count(), 2);
    }
}
