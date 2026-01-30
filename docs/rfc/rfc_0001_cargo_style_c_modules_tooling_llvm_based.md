# RFC-0001: Cargo-style Tooling for C++ Modules (LLVM-based)

## Status
Draft

## Summary
This RFC proposes **`cmod`** (working name): a Cargo-inspired, cross-platform tool for building, managing, and distributing **C++20+ modules** using **LLVM/Clang** as the reference frontend.

The goal is to make C++ modules feel as ergonomic as Rust crates:

```bash
cmod init
cmod build
cmod add fmt
cmod test
cmod publish
```

while respecting C++ realities: multiple compilers, ABI variance, and source-based distribution.

---

## Motivation

### Problems Today

1. No standard way to **discover, build, and cache C++ modules**
2. BMI/PCM files are:
   - Compiler-specific
   - Version-specific
   - Build-flag-specific
3. Existing package managers treat modules as an afterthought
4. Build systems (CMake) leak complexity to users

### Why LLVM?

LLVM already provides:
- Clang module scanning (`-fmodule-output`, dependency scanning)
- Explicit module interfaces (`.ixx`, `.cppm`)
- Stable tooling libraries (`libTooling`, `clang-scan-deps`)

LLVM is the **only realistic neutral foundation**.

---

## Goals

- Cargo-like UX for C++ modules
- First-class module dependency graph
- Deterministic builds
- Source-based distribution
- Multi-platform (Linux/macOS/Windows)
- Compiler-pluggable (Clang first, GCC/MSVC later)

## Non-Goals

- Solving C++ ABI compatibility
- Replacing CMake immediately
- Shipping precompiled binaries by default

---

## High-Level Architecture

```
┌─────────┐
│  cmod   │ CLI / UX
└────┬────┘
     │
┌────▼────────────┐
│ Build Orchestr. │
└────┬────────────┘
     │
┌────▼────────────┐
│ Module Resolver │◄── clang-scan-deps
└────┬────────────┘
     │
┌────▼────────────┐
│ LLVM/Clang FE   │
└────┬────────────┘
     │
┌────▼────────────┐
│ Cache (BMIs)    │
└─────────────────┘
```

---

## Package Model

### Package = Crate-like Unit

```
my_math/
├── cmod.toml
├── src/
│   ├── lib.cppm
│   ├── algebra.cppm
│   └── detail.cpp
└── tests/
```

### `cmod.toml`

See **RFC-UNIFIED** for the complete schema specification. Example minimal configuration:

```toml
[package]
name = "my_math"
version = "0.1.0"
edition = "2030"

[module]
name = "com.github.user.my_math"
root = "src/lib.cppm"

[dependencies]
github.com/fmtlib/fmt = "^10.2"

[toolchain]
compiler = "clang"
cxx_standard = 23
```

---

## Module Graph Resolution

1. Parse module interfaces (`export module X`)
2. Build a **global module graph**
3. Enforce:
   - No cycles between interface units
   - Clear partitions (`X:core`)

Graph is resolved **before compilation**, unlike CMake.

---

## Build Pipeline (Clang)

1. `clang-scan-deps` → JSON module graph
2. Topological sort
3. Compile module interfaces → PCM
4. Compile implementation units
5. Link targets

All PCM paths are **content-addressed**.

---

## Cache Design

### Cache Key

```
hash(
  module-source,
  compiler-id,
  compiler-version,
  flags,
  stdlib,
  target-triple
)
```

### Cache Layout

```
~/.cmod/cache/
└── clang-18.1.2/
    └── x86_64-apple-darwin/
        └── <hash>.pcm
```

Safe, deterministic, and parallel-friendly.

---

## Dependency Distribution

### Decentralized Git-based Modules (Go-style)

`cmod` **does not use a centralized registry**.

Instead, every dependency is a **Git repository** identified by its import path, similar to Go modules.

Example:

```toml
[dependencies]
fmt = { git = "https://github.com/fmtlib/fmt", version = "v10.2.1" }
math = { git = "https://git.satyavis.com/math/core", rev = "a1b2c3d" }
```

Key properties:

- No central authority
- Git is the distribution layer
- Tags, commits, or branches define versions
- Works with GitHub, GitLab, self-hosted, SSH

---

### Module Identity = Import Path

A module's **canonical identity** is its Git URL + module name:

```cpp
export module github.fmtlib.fmt;
export module com.satyavis.math.core;
```

Rules:
- Reverse-domain prefix derived from Git URL
- Full repository path included
- Prevents global name collisions
- Stable across forks

---

### Version Resolution

- Semantic versioning via Git tags (`v1.2.3`)
- Pseudo-versions for commits (like Go):

```
v0.0.0-20260128-a1b2c3d
```

- `cmod.lock` pins exact commits

---

### Offline & Vendoring

```bash
cmod vendor
```

- Dependencies copied into `vendor/`
- Fully hermetic builds
- Required for long-term reproducibility

---


## Compiler Abstraction

```text
CompilerBackend
├── scan_deps()
├── build_pcm()
├── build_obj()
└── link()
```

Initial backend:
- Clang (Tier 1)

Planned:
- GCC (Tier 2)
- MSVC (Tier 2)

---

## Interop with CMake

- `cmod build --emit-cmake`
- `cmod vendor`

Allows incremental adoption.

---

## Security Model

- Registry signatures
- Hash-locked dependencies
- No post-install scripts by default

---

## Comparison

| Tool | Modules-native | UX | LLVM-based |
|----|----|----|----|
| CMake | ❌ | ⚠️ | ❌ |
| vcpkg | ⚠️ | 🙂 | ❌ |
| build2 | ✅ | ⚠️ | ❌ |
| **cmod** | ✅ | ⭐⭐⭐⭐ | ✅ |

---

## Open Questions

- Standard module naming conventions
- Cross-compiler PCM reuse (likely impossible)
- Binary distribution tiers

---

## Future Work

- Language server integration
- `cmod fmt`, `cmod doc`
- WASM target support

---

## Inspiration

- Cargo (Rust)
- SwiftPM
- build2
- clang modules

---

## Conclusion

C++ modules need **opinionated tooling**.
LLVM gives us the foundation.

`cmod` aims to do for C++ modules what Cargo did for Rust crates.

