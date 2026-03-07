# cmod — The Package Manager C++ Deserves

> Cargo-inspired. Git-native. Module-first. Built for C++20 and beyond.

---

## C++ Has Modules Now. Your Build Tool Should Know It.

cmod is a modern package and build tool for C++20 modules. It combines Cargo's developer experience with Git's decentralized model to give C++ developers what they've been waiting for: a single, integrated tool that handles dependencies, builds, caching, and security — without a central registry.

---

## Why cmod?

### One Tool, Complete Workflow

```bash
cmod init my_project                        # Create
cmod add github.com/fmtlib/fmt@10.0        # Add dependencies
cmod build                                  # Build
cmod test                                   # Test
cmod run                                    # Run
```

No CMakeLists.txt. No conanfile.py. No WORKSPACE files. Just `cmod.toml` and go.

### Git Is Your Registry

Dependencies are Git URLs. Publishing is pushing a tag. No accounts, no approvals, no vendor lock-in.

```toml
[dependencies]
"github.com/fmtlib/fmt" = ">=10.0"
"github.com/nlohmann/json" = "^3.11"
"gitlab.com/your-org/internal-lib" = "~2.0"
```

### Modules Are First-Class

cmod understands C++20 modules, partitions, and BMIs natively. It automatically discovers module dependencies with `clang-scan-deps` and builds a full dependency graph before compilation. Modules compile once, rebuild only when changed.

### Deterministic by Default

Mandatory lockfiles pin exact Git commits and toolchain versions. `cmod build --locked` guarantees the same build, every time, everywhere.

### Secure by Design

Hash verification. Trust-on-first-use. Signature checking. SBOM generation. Dependency auditing. Security isn't an add-on — it's built in.

---

## Quick Comparison

| | cmod | CMake | Conan | vcpkg | Bazel |
|---|:---:|:---:|:---:|:---:|:---:|
| C++20 modules | **Native** | Experimental | No | Partial | Partial |
| Package management | **Built-in** | None | Registry | Registry | Rules |
| Mandatory lockfile | **Yes** | No | No | No | Implicit |
| Config format | **TOML** | CMake DSL | Python | JSON | Starlark |
| Dependencies via | **Git** | Manual | Central server | Central repo | HTTP archives |
| Security built-in | **Yes** | No | Partial | Partial | Hermetic |
| Setup complexity | **Low** | Medium | Medium-High | Medium | High |

---

## The Complete CLI

### Build & Run
```
cmod init          Create a project
cmod build         Build (debug or --release)
cmod test          Run tests
cmod run           Run the binary
cmod clean         Remove artifacts
```

### Dependencies
```
cmod add           Add a dependency
cmod remove        Remove a dependency
cmod resolve       Generate/update lockfile
cmod update        Update to latest compatible versions
cmod deps          Inspect the dependency graph
cmod vendor        Vendor for offline builds
```

### Build Intelligence
```
cmod graph         Visualize module DAG (dot/json)
cmod explain       Why would this module rebuild?
cmod compile-commands   Generate compile_commands.json
cmod lint          Run clang-tidy
cmod fmt           Run clang-format
```

### Security & Compliance
```
cmod verify        Verify dependency integrity
cmod audit         Audit for security issues
cmod sbom          Generate Software Bill of Materials
```

### Workspace & Cache
```
cmod workspace     Manage monorepo members
cmod cache         Manage artifact cache
cmod status        Project overview
cmod toolchain     Manage compiler toolchain
```

**30+ commands.** One tool. The complete C++ development lifecycle.

---

## Configuration That Makes Sense

```toml
[package]
name = "my_project"
version = "1.0.0"
edition = "2024"
authors = ["Your Name <you@example.com>"]
license = "MIT"

[module]
name = "com.github.yourname.my_project"
root = "src/my_project.cppm"

[dependencies]
"github.com/fmtlib/fmt" = ">=10.0"
"github.com/nlohmann/json" = "^3.11"

[toolchain]
compiler = "clang"
version = ">=18.0"
std = "c++23"

[build]
type = "binary"
optimization = "2"
```

15 lines. Human-readable. No DSL to learn.

---

## Architecture

Built in Rust. Modular by design. Each layer has one job.

```
CLI → Resolver → Workspace → Build Orchestrator → LLVM/Clang → Cache → Security
```

- **cmod-core** — Config, manifest, lockfile, error model
- **cmod-resolver** — Git fetch, semver solving, lockfile generation
- **cmod-build** — Module DAG, build planning, Clang invocation
- **cmod-cache** — Content-addressed caching (SHA-256)
- **cmod-workspace** — Monorepo management
- **cmod-security** — Verification, trust, policy enforcement

---

## Status

| Phase | Status |
|-------|--------|
| Foundations — manifest, resolver, lockfile, CLI | **Complete** |
| Builds — LLVM/Clang backend, module DAG, build runner | **Complete** |
| Scale — workspace, local cache | **Complete** |
| Distributed — remote cache, BMI distribution, distributed workers | In Progress |
| Security — cryptographic signing, auditing, SBOM, policy enforcement | In Progress |
| Ecosystem — LSP server, plugin sandboxing, module registry, features | In Progress |

**737+ tests passing.** 8 focused crates. 21 design RFCs.

---

## Get Started

```bash
git clone https://github.com/nickshouse/cmod.git
cd cmod
cargo build --release

# Your first project
cmod init hello && cd hello
cmod build && cmod run
```

---

## Open Source

Apache-2.0 licensed. Built in public. Contributions welcome.

[GitHub](https://github.com/nickshouse/cmod) | [Examples](https://github.com/nickshouse/cmod/tree/main/examples) | [Documentation](https://github.com/nickshouse/cmod/tree/main/docs) | [Contributing](https://github.com/nickshouse/cmod/blob/main/CONTRIBUTING.md)

---

*C++ is powerful. Its tooling should be too.*
