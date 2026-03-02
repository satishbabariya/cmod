# Security Policy

## Reporting a Vulnerability

**Please do NOT report security vulnerabilities through public GitHub issues.**

Instead, use [GitHub Security Advisories](https://github.com/satishbabariya/cmod/security/advisories/new) to report vulnerabilities privately.

### What to Include

- Description of the vulnerability
- Steps to reproduce
- Affected versions and components
- Potential impact
- Suggested fix (if any)

### Scope

The following are in scope for security reports:

- The `cmod` CLI tool and all crates (`cmod-core`, `cmod-resolver`, `cmod-build`, `cmod-cache`, `cmod-workspace`, `cmod-security`)
- Supply-chain integrity features (lockfiles, verification, caching)
- Git operations and dependency resolution
- Build orchestration and Clang invocation

### Response Timeline

- **48 hours** — acknowledgment of your report
- **1 week** — initial assessment and severity classification
- **30 days** — target for a fix or mitigation

We will coordinate disclosure with you and credit reporters in release notes unless anonymity is requested.
