//! Security policy enforcement for cmod.
//!
//! Evaluates the `[security]` manifest section against lockfile packages
//! to enforce signature requirements, source restrictions, and trust levels.

use cmod_core::error::CmodError;
use cmod_core::lockfile::LockedPackage;
use cmod_core::manifest::Security;

use crate::trust::TrustDb;
use crate::verify::{verify_locked_package, SignatureStatus};

/// A security policy derived from the `[security]` section of `cmod.toml`.
#[derive(Debug, Clone)]
pub struct SecurityPolicy {
    /// Signature requirement: "none", "warn", "require".
    pub signature_policy: SignatureRequirement,
    /// Allowed source URL patterns (glob-style). Empty = allow all.
    pub allowed_sources: Vec<String>,
    /// Whether to verify content hashes.
    pub verify_checksums: bool,
    /// Trusted source patterns from the manifest.
    pub trusted_sources: Vec<String>,
}

/// Signature enforcement level.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SignatureRequirement {
    /// No signature checking.
    None,
    /// Warn on unsigned/untrusted commits but don't fail.
    Warn,
    /// Require valid signatures on all dependencies.
    Require,
}

impl SignatureRequirement {
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "require" => SignatureRequirement::Require,
            "warn" => SignatureRequirement::Warn,
            _ => SignatureRequirement::None,
        }
    }
}

/// Result of enforcing a policy on a single package.
#[derive(Debug, Clone)]
pub struct PolicyViolation {
    pub package: String,
    pub severity: ViolationSeverity,
    pub reason: String,
}

/// Severity of a policy violation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ViolationSeverity {
    /// Non-blocking warning.
    Warning,
    /// Blocking error.
    Error,
}

impl SecurityPolicy {
    /// Create a policy from the manifest's `[security]` section.
    pub fn from_manifest(security: Option<&Security>) -> Self {
        match security {
            Some(sec) => SecurityPolicy {
                signature_policy: sec
                    .signature_policy
                    .as_deref()
                    .map(SignatureRequirement::parse)
                    .unwrap_or(SignatureRequirement::None),
                allowed_sources: sec.trusted_sources.clone(),
                verify_checksums: sec.verify_checksums.unwrap_or(false),
                trusted_sources: sec.trusted_sources.clone(),
            },
            None => SecurityPolicy::default(),
        }
    }

    /// Enforce this policy against all locked packages.
    ///
    /// Returns a list of policy violations. Depending on the violation severity,
    /// the caller decides whether to abort or just warn.
    pub fn enforce(
        &self,
        packages: &[LockedPackage],
        deps_dir: &std::path::Path,
        trust_db: Option<&TrustDb>,
    ) -> Vec<PolicyViolation> {
        let mut violations = Vec::new();

        for pkg in packages {
            // 1. Check source restrictions
            if !self.allowed_sources.is_empty() {
                if let Some(ref repo_url) = pkg.repo {
                    if !self.source_is_allowed(repo_url) {
                        violations.push(PolicyViolation {
                            package: pkg.name.clone(),
                            severity: ViolationSeverity::Error,
                            reason: format!(
                                "source '{}' is not in the allowed sources list",
                                repo_url
                            ),
                        });
                    }
                }
            }

            // 2. Check trust database
            if let Some(db) = trust_db {
                if !db.is_trusted(&pkg.name) {
                    violations.push(PolicyViolation {
                        package: pkg.name.clone(),
                        severity: ViolationSeverity::Warning,
                        reason: "package is not in the trust database".to_string(),
                    });
                }
            }

            // 3. Check content hash presence when checksums are required
            if self.verify_checksums && pkg.hash.is_none() && pkg.source.as_deref() == Some("git") {
                violations.push(PolicyViolation {
                    package: pkg.name.clone(),
                    severity: ViolationSeverity::Warning,
                    reason: "no content hash recorded; re-run `cmod resolve`".to_string(),
                });
            }

            // 4. Check signature policy
            if self.signature_policy != SignatureRequirement::None {
                let severity = match self.signature_policy {
                    SignatureRequirement::Require => ViolationSeverity::Error,
                    SignatureRequirement::Warn => ViolationSeverity::Warning,
                    SignatureRequirement::None => continue,
                };

                let repo_path = deps_dir.join(&pkg.name);
                if !repo_path.exists() {
                    // Can't check signature if repo not checked out
                    continue;
                }

                match verify_locked_package(pkg, &repo_path, true) {
                    Ok(result) => match &result.signature_status {
                        SignatureStatus::Valid { .. } => {
                            // OK — valid signature
                        }
                        SignatureStatus::Untrusted { signer } => {
                            violations.push(PolicyViolation {
                                package: pkg.name.clone(),
                                severity,
                                reason: format!("signed by untrusted key: {}", signer),
                            });
                        }
                        SignatureStatus::Unsigned => {
                            violations.push(PolicyViolation {
                                package: pkg.name.clone(),
                                severity,
                                reason: "commit is unsigned".to_string(),
                            });
                        }
                        SignatureStatus::Invalid { reason } => {
                            violations.push(PolicyViolation {
                                package: pkg.name.clone(),
                                severity: ViolationSeverity::Error,
                                reason: format!("invalid signature: {}", reason),
                            });
                        }
                    },
                    Err(e) => {
                        violations.push(PolicyViolation {
                            package: pkg.name.clone(),
                            severity,
                            reason: format!("could not verify signature: {}", e),
                        });
                    }
                }
            }
        }

        violations
    }

    /// Check if a source URL matches the allowed sources patterns.
    fn source_is_allowed(&self, url: &str) -> bool {
        if self.allowed_sources.is_empty() {
            return true;
        }

        for pattern in &self.allowed_sources {
            if source_matches_pattern(url, pattern) {
                return true;
            }
        }

        false
    }

    /// Check if this policy has any enforcement enabled.
    pub fn is_active(&self) -> bool {
        self.signature_policy != SignatureRequirement::None
            || !self.allowed_sources.is_empty()
            || self.verify_checksums
    }

    /// Return true if any violations are errors (blocking).
    pub fn has_errors(violations: &[PolicyViolation]) -> bool {
        violations
            .iter()
            .any(|v| v.severity == ViolationSeverity::Error)
    }

    /// Convert violations to a CmodError if there are any errors.
    pub fn to_error(violations: &[PolicyViolation]) -> Result<(), CmodError> {
        let errors: Vec<&PolicyViolation> = violations
            .iter()
            .filter(|v| v.severity == ViolationSeverity::Error)
            .collect();

        if errors.is_empty() {
            Ok(())
        } else {
            let messages: Vec<String> = errors
                .iter()
                .map(|v| format!("{}: {}", v.package, v.reason))
                .collect();
            Err(CmodError::SecurityViolation {
                reason: messages.join("; "),
            })
        }
    }
}

impl Default for SecurityPolicy {
    fn default() -> Self {
        SecurityPolicy {
            signature_policy: SignatureRequirement::None,
            allowed_sources: Vec::new(),
            verify_checksums: false,
            trusted_sources: Vec::new(),
        }
    }
}

/// Match a URL against a glob-like source pattern.
///
/// Supports:
/// - `github.com/*` → matches any repo on github.com
/// - `github.com/myorg/*` → matches any repo under myorg
/// - `*` → matches everything
/// - Exact URL match
fn source_matches_pattern(url: &str, pattern: &str) -> bool {
    if pattern == "*" {
        return true;
    }

    // Normalize: strip https:// prefix for comparison
    let url_normalized = url
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/')
        .trim_end_matches(".git");

    let pattern_normalized = pattern
        .trim_start_matches("https://")
        .trim_start_matches("http://")
        .trim_end_matches('/');

    // Handle trailing wildcard
    if let Some(prefix) = pattern_normalized.strip_suffix('*') {
        let prefix = prefix.trim_end_matches('/');
        return url_normalized.starts_with(prefix);
    }

    // Exact match
    url_normalized == pattern_normalized
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signature_requirement_from_str() {
        assert_eq!(
            SignatureRequirement::parse("require"),
            SignatureRequirement::Require
        );
        assert_eq!(
            SignatureRequirement::parse("warn"),
            SignatureRequirement::Warn
        );
        assert_eq!(
            SignatureRequirement::parse("none"),
            SignatureRequirement::None
        );
        assert_eq!(
            SignatureRequirement::parse("anything"),
            SignatureRequirement::None
        );
    }

    #[test]
    fn test_source_matches_pattern() {
        assert!(source_matches_pattern(
            "https://github.com/fmtlib/fmt",
            "github.com/*"
        ));
        assert!(source_matches_pattern(
            "https://github.com/myorg/mylib",
            "github.com/myorg/*"
        ));
        assert!(!source_matches_pattern(
            "https://gitlab.com/myorg/mylib",
            "github.com/*"
        ));
        assert!(source_matches_pattern("https://github.com/fmtlib/fmt", "*"));
        assert!(source_matches_pattern(
            "https://github.com/fmtlib/fmt.git",
            "github.com/fmtlib/fmt"
        ));
    }

    #[test]
    fn test_default_policy_is_inactive() {
        let policy = SecurityPolicy::default();
        assert!(!policy.is_active());
    }

    #[test]
    fn test_policy_from_manifest() {
        let sec = Security {
            signing_key: None,
            signing_backend: None,
            verify_checksums: Some(true),
            trusted_sources: vec!["github.com/*".to_string()],
            signature_policy: Some("require".to_string()),
            oidc_issuer: None,
            certificate_identity: None,
        };
        let policy = SecurityPolicy::from_manifest(Some(&sec));
        assert!(policy.is_active());
        assert_eq!(policy.signature_policy, SignatureRequirement::Require);
        assert!(policy.verify_checksums);
        assert_eq!(policy.allowed_sources.len(), 1);
    }

    #[test]
    fn test_enforce_allowed_sources() {
        let policy = SecurityPolicy {
            allowed_sources: vec!["github.com/myorg/*".to_string()],
            ..SecurityPolicy::default()
        };

        let packages = vec![
            LockedPackage {
                name: "good-dep".to_string(),
                version: "1.0.0".to_string(),
                source: Some("git".to_string()),
                repo: Some("https://github.com/myorg/good-dep".to_string()),
                commit: None,
                hash: None,
                toolchain: None,
                targets: std::collections::BTreeMap::new(),
                deps: vec![],
                features: vec![],
            },
            LockedPackage {
                name: "bad-dep".to_string(),
                version: "1.0.0".to_string(),
                source: Some("git".to_string()),
                repo: Some("https://gitlab.com/evil/bad-dep".to_string()),
                commit: None,
                hash: None,
                toolchain: None,
                targets: std::collections::BTreeMap::new(),
                deps: vec![],
                features: vec![],
            },
        ];

        let violations = policy.enforce(&packages, std::path::Path::new("/tmp"), None);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].package, "bad-dep");
        assert_eq!(violations[0].severity, ViolationSeverity::Error);
    }

    #[test]
    fn test_enforce_checksum_warning() {
        let policy = SecurityPolicy {
            verify_checksums: true,
            ..SecurityPolicy::default()
        };

        let packages = vec![LockedPackage {
            name: "no-hash".to_string(),
            version: "1.0.0".to_string(),
            source: Some("git".to_string()),
            repo: None,
            commit: None,
            hash: None,
            toolchain: None,
            targets: std::collections::BTreeMap::new(),
            deps: vec![],
            features: vec![],
        }];

        let violations = policy.enforce(&packages, std::path::Path::new("/tmp"), None);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].severity, ViolationSeverity::Warning);
    }

    #[test]
    fn test_has_errors() {
        let warnings_only = vec![PolicyViolation {
            package: "pkg".to_string(),
            severity: ViolationSeverity::Warning,
            reason: "test".to_string(),
        }];
        assert!(!SecurityPolicy::has_errors(&warnings_only));

        let with_error = vec![PolicyViolation {
            package: "pkg".to_string(),
            severity: ViolationSeverity::Error,
            reason: "test".to_string(),
        }];
        assert!(SecurityPolicy::has_errors(&with_error));
    }

    #[test]
    fn test_to_error() {
        let no_errors = vec![PolicyViolation {
            package: "pkg".to_string(),
            severity: ViolationSeverity::Warning,
            reason: "test".to_string(),
        }];
        assert!(SecurityPolicy::to_error(&no_errors).is_ok());

        let with_error = vec![PolicyViolation {
            package: "pkg".to_string(),
            severity: ViolationSeverity::Error,
            reason: "not allowed".to_string(),
        }];
        let err = SecurityPolicy::to_error(&with_error);
        assert!(err.is_err());
    }
}
