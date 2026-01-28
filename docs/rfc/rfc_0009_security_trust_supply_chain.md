# RFC-0009: Security, Trust & Supply Chain

## Status
Draft

## Summary
This RFC defines the security, trust, and supply chain model for **cmod**, covering signed commits, artifact verification, and trusted mirrors. The goal is to ensure that modules and dependencies are **authentic, untampered, and verifiable** in any build environment.

---

## Motivation

C++ module ecosystems are increasingly distributed via Git repositories. Risks include:
- Malicious commits
- Compromised dependencies
- CI cache poisoning
- Supply chain attacks

cmod must provide mechanisms to guarantee **module integrity and provenance**.

---

## Security Goals

1. Authenticate module sources
2. Verify integrity of compiled artifacts
3. Ensure reproducible builds are trustworthy
4. Support enterprise and air-gapped environments

---

## Signed Commits & Modules

- All modules should have **commit signatures (GPG/SSH)**
- Lockfile captures the commit hash and signature verification status
- Verification occurs at `cmod resolve` or `cmod fetch`

Lockfile example:
```toml
[[package]]
name = "github.com/acme/math"
version = "1.4.2"
commit = "a13f9c2"
signature = "ABCDEF123456..."
verified = true
```

Rules:
- Builds fail if a required signature cannot be verified
- Unsigned or untrusted commits require explicit override

---

## Artifact Integrity

- Cache keys include a **SHA256 hash of compiled artifacts**
- Before reuse, artifacts are validated against the hash
- CI caches may include signed artifact manifests

---

## Trusted Mirrors

- Organizations may maintain private mirrors for internal modules
- Mirrors are configured via `cmod config`
- Mirror usage respects trust policies and signature validation

---

## Supply Chain Policies

- Developers may define minimum trust levels per dependency
- Policies include:
  - Only signed commits
  - Verified builds
  - Required ABI/Compiler constraints

---

## Air-Gapped Builds

- Vendor mode (`vendor/`) is fully compatible with signed modules
- No network access required if all artifacts and signatures are vendored
- Lockfile guarantees reproducibility offline

---

## CI Integration

- CI pipelines can validate signatures and hashes
- Failed validation aborts builds
- Optional: cached verification reports

---

## Open Questions

- Which signature schemes to support beyond GPG/SSH?
- Should artifact signing be mandatory for all modules?
- How to manage cross-toolchain verification (clang vs gcc)?

---

## Next RFCs

- RFC-0010: IDE Integration & Developer Experience (syntax-aware module browsing, auto-completion, real-time verification)
- RFC-0011: Optional Precompiled Module Distribution (safe sharing with signature verification and cache integration)

