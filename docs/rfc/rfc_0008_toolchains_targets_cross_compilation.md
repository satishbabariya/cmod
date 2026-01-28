# RFC-0008: Toolchains, Targets & Cross-Compilation

## Status
Draft

## Summary
This RFC defines how **cmod** manages toolchains, platform targets, and cross-compilation, ensuring reproducible builds across architectures, operating systems, and compilers.

Goals:
- Explicit toolchain selection
- Deterministic builds across hosts and targets
- Safe cross-compilation
- Clear integration with module graph and cache

---

## Motivation

C++ modules are sensitive to compiler, standard library, ABI, and target triple. Without explicit toolchain semantics:
- Builds may silently break
- PCMs may be incompatible
- Cross-compilation is error-prone

cmod explicitly encodes toolchain identity in lockfiles and build graphs.

---

## Toolchain Model

A **Toolchain** is a tuple:

```
Toolchain {
  compiler: string      // clang-18, gcc-13
  version: string       // 18.1.0
  cxxflags: string[]    // flags
  stdlib: string        // libc++, libstdc++
  abi: string           // itanium, msvc
  target: string        // x86_64-apple-darwin, aarch64-linux-gnu
  sysroot: path         // optional
}
```

Rules:
- Immutable once selected for a build
- Encoded in `cmod.lock`
- Influences cache keys

---

## Target Triples

- Uses LLVM-style target triples
- Host vs target distinction
- Default: host triple = target triple
- Cross-compilation requires explicit toolchain triple

Examples:
```
x86_64-apple-darwin
arm64-apple-ios
aarch64-linux-gnu
```

---

## Cross-Compilation Rules

- Root module may be built for multiple targets
- Each target uses independent build plan
- Cache is target-specific
- BMIs are not portable across targets

### Example
```
Target1: x86_64-linux-gnu
Target2: aarch64-linux-gnu
```
Each target has its own graph and cache entries.

---

## Lockfile Integration

`cmod.lock` entries now include toolchain identity:

```toml
[[package]]
name = "github.com/acme/math"
version = "1.4.2"
commit = "a13f9c2"
compiler = "clang-18"
stdlib = "libc++"
target = "macos-arm64"
sysroot = "/opt/toolchains/macos-arm64"
```

Rules:
- Lockfile guarantees reproducible builds
- Cross-target builds must generate separate entries per target

---

## Cache Considerations

- Cache keys include target triple and compiler identity
- Prevents invalid reuse of BMIs across architectures
- Enables safe CI cache sharing across builds of same target

---

## Incremental Builds Across Toolchains

- Build graph is duplicated per toolchain
- Dependencies may share source hash but have separate BMI/object caches
- Incrementality is preserved per target

---

## CI / Multi-Host Builds

- Multiple toolchains may be configured per CI job
- Each job can safely rebuild modules for its target
- Lockfile ensures reproducibility
- Optional: cache sharing between hosts via content-addressed artifact hashes

---

## Open Questions

- Should sysroot and flags be fully canonicalized in lockfile?
- How to handle standard library ABI differences for cross-compilation?
- Should cmod support toolchain overrides per module vs global per build?

---

## Next RFCs

- RFC-0009: Security, Trust & Supply Chain (signed commits, checksum verification)
- RFC-0010: IDE Integration & Developer Experience
- RFC-0011: Precompiled Module Distribution (optional, safe sharing)

