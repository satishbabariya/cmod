# RFC-0003: Lockfiles & Reproducible Builds

## Status
Draft

## Summary
This RFC defines the **lockfile format and reproducible build guarantees** for **cmod**. It ensures that dependency resolution, module versions, toolchains, and build inputs produce deterministic and repeatable builds across machines, CI systems, and time.

---

## Motivation

C++ builds are notoriously non-deterministic due to:
- Implicit dependency resolution
- Toolchain drift
- Platform-specific behavior
- Transitive dependency changes

Without a lockfile, the same project can produce different binaries on different machines or at different times.

---

## Goals

- Deterministic dependency resolution
- Stable builds across environments
- CI-friendly and cacheable builds
- Minimal friction for developers

## Non-Goals

- Bit-for-bit identical binaries across all platforms
- Replacing system package managers
- Hiding toolchain differences

---

## Lockfile Overview

The lockfile is named `cmod.lock`.

- Generated automatically by `cmod resolve` or `cmod build`
- Never manually edited
- Committed to version control

Single-module and workspace projects both use the same format.

---

## Lockfile Contents

Example (simplified):

```toml
version = 1

[[package]]
name = "github.com/acme/math-core"
version = "1.4.2"
source = "git"
repo = "https://github.com/acme/math-core"
commit = "a8f3c21"
hash = "sha256:..."

[package.toolchain]
compiler = "clang"
version = "18.1.0"
stdlib = "libc++"

[package.targets]
x86_64-linux-gnu = {}
arm64-macos = {}
```

### Recorded Data
- Exact module version and git commit
- Content hash of module sources
- Toolchain identity (compiler + stdlib)
- Target triples
- Feature flags enabled

---

## Resolution Rules

- Lockfile takes precedence over `cmod.toml`
- Missing entries trigger resolution
- Conflicts cause hard failures
- Optional dependencies resolved only if enabled

---

## Reproducibility Guarantees

When using a committed lockfile:
- Same dependency graph
- Same source revisions
- Same compiler and standard library
- Same build configuration

This guarantees **structural reproducibility**, even if binary output may vary due to platform or compiler internals.

---

## Workspace Behavior

- One `cmod.lock` per workspace root
- Captures all member modules
- Resolution performed once for entire workspace

---

## Lockfile Updates

- `cmod update` updates dependencies
- Supports selective updates:
  - `cmod update fmt`
  - `cmod update --patch`

- Old lockfiles remain valid

---

## CI Integration

Recommended CI flow:

```
cmod verify
cmod build --locked
```

- `--locked` forbids resolution changes
- Fails if lockfile is out of date

---

## Comparison

| System | Lockfile |
|------|---------|
| Cargo | Cargo.lock |
| npm | package-lock.json |
| Go | go.sum |

---

## Backward Compatibility

- Projects without lockfiles still build
- Warnings issued in CI contexts
- Lockfiles are opt-in but recommended

---

## Open Questions

- Should toolchain versions be strictly enforced?
- Should lockfiles support multiple toolchains?
- How should system libraries be represented?

---

## Next RFCs

- RFC-0005: Binary Artifacts & Distributed Build Caches
- RFC-0009: Security, Signing & Supply-Chain Integrity
