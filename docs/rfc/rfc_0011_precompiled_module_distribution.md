# RFC-0011: Optional Precompiled Module Distribution

## Status
Draft

## Summary
This RFC defines **optional precompiled module distribution** for **cmod**, allowing safe sharing of Binary Module Interfaces (BMIs) and object artifacts across repositories or teams, while ensuring security, reproducibility, and compatibility with lockfiles and caches.

Goals:
- Enable fast rebuilds without recompiling modules
- Maintain security and trust guarantees
- Avoid ABI/Toolchain mismatches
- Integrate seamlessly with cmod cache and IDEs

---

## Motivation

C++ module builds can be slow for large projects. Precompiling commonly used modules (BMIs) and sharing them can:
- Reduce build times in CI and local development
- Enable teams to avoid recompiling third-party modules
- Speed up IDE features (code completion, navigation)

Constraints:
- BMIs are compiler- and platform-specific
- Must be verifiable and trusted (RFC-0009)
- Lockfile must accurately reflect used artifacts

---

## Distribution Model

### Artifact Packaging
- Precompiled modules packaged per **module + version + toolchain + target triple**
- Optional JSON metadata including:
  - Source commit hash
  - Compiler version
  - Target triple
  - Stdlib and ABI
  - Signature

### Repository Model
- Distribution is **Git-based or HTTP/S**
- No central server required
- Optional mirrors supported

### Cache Integration
- Downloaded BMIs stored in local cache (`~/.cache/cmod/<module>/<hash>`)
- Verified before reuse using signature and content hash

---

## Build Process Integration

1. Resolve dependencies (RFC-0006) using `cmod.lock`
2. Check local cache for matching precompiled modules
3. If missing, fetch from remote distribution
4. Verify integrity (hash + signature)
5. Reuse BMI in build graph (RFC-0007)

---

## Compatibility Rules

- Toolchain and target triple must match
- Compiler version must satisfy lockfile constraints
- Stdlib and ABI must match declared module metadata
- Cross-target BMIs are forbidden

---

## Security & Trust

- Signed BMIs verified according to RFC-0009
- Unverified artifacts are rejected unless explicitly overridden
- Optionally, enforce read-only remote cache mode for CI

---

## IDE & Developer Experience

- IDEs can prefetch BMIs for code completion
- Modules with cached BMIs show instant availability
- Verification errors surfaced in IDE for clarity

---

## Open Questions

- Should partial BMI sharing be allowed (e.g., partitions only)?
- Best practice for versioning of distributed BMIs?
- Should cmod support delta or incremental BMI updates?

---

## Next RFCs
- RFC-0012: Advanced Build Strategies & Performance Optimizations
- RFC-0013: Distributed Builds & Remote Execution