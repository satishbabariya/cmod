# cmod

**cmod** is a Git-native, decentralized package and build tool for **modern C++ modules**, inspired by Cargo’s reliability and Go’s simplicity—without a central registry.

---

## Vision

C++ deserves a first-class module ecosystem that is:

- **Native to the language** (C++20 modules, not header hacks)
- **Fast at scale** (BMIs, caching, incremental builds)
- **Deterministic** (lockfiles, pinned toolchains)
- **Decentralized** (Git as the source of truth)
- **Secure by design** (verifiable supply chain)

cmod is not "yet another build system". It is a **module-aware orchestration layer** that sits above existing compilers and integrates deeply with LLVM-based toolchains.

---

## Core Principles

1. **Git is the registry**
   - No central package server
   - Module identity = repository URL

2. **Modules are the unit of composition**
   - C++20 modules, partitions, and BMIs

3. **Reproducibility is mandatory**
   - Lockfiles, pinned commits, explicit toolchains

4. **Performance matters**
   - Cached BMIs, distributed build artifacts

5. **Security is opt-in but first-class**
   - Signing, verification, cache integrity

---

## What cmod Is

- A module-aware dependency resolver
- A build orchestrator (not a compiler)
- A workspace & monorepo manager
- A foundation for tooling and IDEs

## What cmod Is Not

- A central package registry
- A replacement for LLVM/Clang
- A monolithic build system like Bazel

---

## High-Level Workflow

```
cmod init
cmod add github.com/acme/math-core
cmod build
```

Under the hood:
- Resolves Git-based modules
- Builds C++ modules via Clang
- Caches BMIs and artifacts
- Produces reproducible builds

---

## Status

🚧 **Experimental / RFC-driven**

The project is currently defined by RFCs (0001–0022). Implementation will follow the finalized specs.

---

## License

TBD (Apache-2.0 or MIT recommended)

