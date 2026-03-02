use semver::{Version, VersionReq};

use cmod_core::error::CmodError;

/// Parse a cmod version constraint string into a semver::VersionReq.
///
/// Supports:
///   - `^1.2`  → `>=1.2.0, <2.0.0`
///   - `~1.2`  → `>=1.2.0, <1.3.0`
///   - `>=1.0,<2.0`  → exact range
///   - `1.4`   → `>=1.4.0, <1.5.0`
///   - `1.4.2` → `=1.4.2`
///   - `*`     → any version
pub fn parse_version_req(constraint: &str) -> Result<VersionReq, CmodError> {
    let constraint = constraint.trim();

    // Strip leading 'v' if present (e.g., "v1.2.3")
    let constraint = constraint.strip_prefix('v').unwrap_or(constraint);

    if constraint == "*" {
        return Ok(VersionReq::STAR);
    }

    VersionReq::parse(constraint).map_err(|e| CmodError::UnresolvableConstraints {
        name: constraint.to_string(),
        reason: format!("invalid version constraint: {}", e),
    })
}

/// Parse an exact version string.
pub fn parse_version(version_str: &str) -> Result<Version, CmodError> {
    let version_str = version_str.trim();
    let version_str = version_str.strip_prefix('v').unwrap_or(version_str);

    Version::parse(version_str).map_err(|e| CmodError::UnresolvableConstraints {
        name: version_str.to_string(),
        reason: format!("invalid version: {}", e),
    })
}

/// Check if a version satisfies a constraint.
pub fn version_matches(version: &Version, req: &VersionReq) -> bool {
    req.matches(version)
}

/// Given a set of available versions and a constraint, select the highest matching version.
pub fn resolve_best_version(available: &[Version], req: &VersionReq) -> Option<Version> {
    let mut matching: Vec<&Version> = available.iter().filter(|v| req.matches(v)).collect();
    matching.sort();
    matching.last().cloned().cloned()
}

/// Generate a pseudo-version for an untagged commit.
///
/// Format: `v0.0.0-YYYYMMDD-<short_commit>`
pub fn pseudo_version(date: &str, commit_short: &str) -> String {
    format!("0.0.0-{}-{}", date, commit_short)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_caret() {
        let req = parse_version_req("^1.2").unwrap();
        let v120 = Version::parse("1.2.0").unwrap();
        let v130 = Version::parse("1.3.0").unwrap();
        let v200 = Version::parse("2.0.0").unwrap();
        assert!(req.matches(&v120));
        assert!(req.matches(&v130));
        assert!(!req.matches(&v200));
    }

    #[test]
    fn test_parse_tilde() {
        let req = parse_version_req("~1.2").unwrap();
        let v121 = Version::parse("1.2.1").unwrap();
        let v130 = Version::parse("1.3.0").unwrap();
        assert!(req.matches(&v121));
        assert!(!req.matches(&v130));
    }

    #[test]
    fn test_parse_exact() {
        let req = parse_version_req("=1.4.2").unwrap();
        let exact = Version::parse("1.4.2").unwrap();
        let other = Version::parse("1.4.3").unwrap();
        assert!(req.matches(&exact));
        assert!(!req.matches(&other));
    }

    #[test]
    fn test_strip_v_prefix() {
        let req = parse_version_req("v1.2.0").unwrap();
        let v = Version::parse("1.2.0").unwrap();
        assert!(req.matches(&v));
    }

    #[test]
    fn test_resolve_best_version() {
        let versions = vec![
            Version::parse("1.0.0").unwrap(),
            Version::parse("1.2.0").unwrap(),
            Version::parse("1.5.0").unwrap(),
            Version::parse("2.0.0").unwrap(),
        ];
        let req = parse_version_req("^1.0").unwrap();
        let best = resolve_best_version(&versions, &req);
        assert_eq!(best, Some(Version::parse("1.5.0").unwrap()));
    }

    #[test]
    fn test_pseudo_version() {
        let pv = pseudo_version("20260128", "a1b2c3d");
        assert_eq!(pv, "0.0.0-20260128-a1b2c3d");
    }
}
