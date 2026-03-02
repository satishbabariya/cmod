# cmod

**Cargo-inspired, Git-native package and build tool for modern C++20 modules.**

[![CI](https://github.com/satishbabariya/cmod/actions/workflows/ci.yml/badge.svg)](https://github.com/satishbabariya/cmod/actions/workflows/ci.yml)
[![License: Apache-2.0](https://img.shields.io/badge/License-Apache_2.0-blue.svg)](LICENSE)

> C++ has modules now. It deserves a build tool that knows it.
> cmod brings Cargo's reliability and Go's simplicity to C++20 — without a central registry.

## Why cmod?

- **No standard package manager for C++.** CMake generates build files but doesn't manage dependencies. Conan and vcpkg distribute binaries with a header-first mindset. None are module-native.
- **Header-based builds are fragile and slow.** Textual inclusion causes redundant parsing, macro pollution, and non-deterministic builds.
- **C++20 modules change everything.** Modules are compiled once into Binary Module Interfaces (BMIs), enabling fast incremental builds — but the ecosystem tooling never caught up.
- **Existing tools require too much ceremony.** cmod gives you `init`, `add`, `build` — and gets out of the way.

## Features

- **Git is the registry** — module identity is a Git URL, no central package server required
- **C++20 modules first** — native support for modules, partitions, and BMIs
- **Deterministic builds** — mandatory lockfiles pin exact commit hashes and toolchain versions
- **Fast incremental builds** — cached BMIs, content-addressed artifact cache, parallel compilation
- **Workspace support** — monorepo management with unified dependency resolution
- **LLVM/Clang integration** — uses `clang-scan-deps` for automatic module dependency discovery
- **Security built-in** — hash verification, signature checking, trust-on-first-use (TOFU) model
- **30+ CLI commands** — from `init` to `sbom`, covering the full development lifecycle

## Quick Start

### Prerequisites

- **Rust 1.74+** — [rustup.rs](https://rustup.rs/)
- **LLVM/Clang 17+** — for C++ module compilation

### Install from Source

```bash
git clone https://github.com/satishbabariya/cmod.git
cd cmod
cargo install --path crates/cmod-cli
```

### Create a Module

```bash
cmod init my_project
cd my_project
```

### Add Dependencies

```bash
cmod add github.com/fmtlib/fmt@^10.0
cmod add github.com/acme/math-core
```

### Build and Test

```bash
cmod build
cmod test
cmod build --release
```

### Examples

See the [examples/](examples/) directory for complete working projects:

| Example | Description |
|---|---|
| [hello](examples/hello/) | Minimal binary, no dependencies |
| [library](examples/library/) | Static library with module partitions |
| [with-deps](examples/with-deps/) | Git dependencies (fmt + json) |
| [workspace](examples/workspace/) | Multi-member monorepo |
| [path-deps](examples/path-deps/) | Local path dependencies |

## Configuration

`cmod.toml` is your project manifest:

```toml
[package]
name = "my_project"
version = "0.1.0"
edition = "2024"
authors = ["Your Name"]
license = "Apache-2.0"

[module]
name = "com.github.user.my_project"
root = "src/lib.cppm"

[dependencies]
"github.com/fmtlib/fmt" = "^10.0"
"github.com/acme/math-core" = "^1.2"

[toolchain]
compiler = "clang"
version = ">=17"
standard = "c++23"

[build]
type = "library"
optimization = "O2"
```

See `docs/rfc/rfc_unified_cmod_schema.md` for the full schema specification.

## CLI Reference

### Core Workflow

| Command | Description |
|---|---|
| `cmod init [--workspace]` | Initialize a new module or workspace |
| `cmod build [--release]` | Build the current module or workspace |
| `cmod test [--release]` | Build and run tests |
| `cmod run [--release]` | Build and run the project binary |
| `cmod clean` | Remove build artifacts |

### Dependency Management

| Command | Description |
|---|---|
| `cmod add <dep>[@version]` | Add a dependency |
| `cmod remove <name>` | Remove a dependency |
| `cmod resolve` | Resolve dependencies and generate lockfile |
| `cmod update [name] [--patch]` | Update dependencies |
| `cmod deps [--tree] [--why <name>]` | Inspect the dependency graph |
| `cmod tidy [--apply]` | Remove unused dependencies |
| `cmod vendor [--sync]` | Vendor dependencies for offline builds |
| `cmod search <query>` | Search for modules by name |

### Build Tools

| Command | Description |
|---|---|
| `cmod graph [--format dot\|json]` | Visualize the module dependency graph |
| `cmod explain <module>` | Explain why a module would be rebuilt |
| `cmod plan` | Output the build plan as JSON |
| `cmod compile-commands` | Generate `compile_commands.json` for IDE integration |
| `cmod emit-cmake` | Export a `CMakeLists.txt` for CMake interop |
| `cmod lint` | Lint C++ source files |
| `cmod fmt [--check]` | Format C++ source files via clang-format |

### Cache and Security

| Command | Description |
|---|---|
| `cmod cache status\|clean\|gc` | Manage the build cache |
| `cmod cache push\|pull` | Sync with remote cache |
| `cmod verify [--signatures]` | Verify integrity and security |
| `cmod audit` | Audit dependencies for security issues |
| `cmod sbom [--output <file>]` | Generate a Software Bill of Materials |

### Workspace and Project

| Command | Description |
|---|---|
| `cmod workspace list\|add\|remove` | Manage workspace members |
| `cmod status` | Show project status overview |
| `cmod check` | Validate module naming and structure |
| `cmod publish [--dry-run]` | Publish a release (create a Git tag) |
| `cmod toolchain show\|check` | Manage the active toolchain |
| `cmod plugin list\|run` | Manage plugins |

### Global Flags

| Flag | Description |
|---|---|
| `--locked` | Use lockfile strictly; fail if outdated |
| `--offline` | Disable network access |
| `--verbose` / `-v` | Enable verbose output |
| `--target <triple>` | Override the target triple |
| `--features <list>` | Enable specific features |
| `--no-default-features` | Disable default features |
| `--no-cache` | Skip build cache |
| `--untrusted` | Skip TOFU trust verification |

### Exit Codes

| Code | Meaning |
|---|---|
| `0` | Success |
| `1` | Build failure |
| `2` | Resolution error |
| `3` | Security violation |

## Architecture

```
User / IDE ──► CLI ──► Dependency Resolver ──► Workspace Manager
                                                      │
              ┌───────────────────────────────────────┘
              ▼
       Build Orchestrator ──► LLVM/Clang ──► Artifact Cache ──► Security
```

cmod is implemented as a Rust workspace with focused crates:

| Crate | Responsibility |
|---|---|
| `cmod-core` | Core types, config parsing, error model, `cmod.toml`/`cmod.lock` formats |
| `cmod-cli` | CLI frontend, clap-based argument parsing, subcommand dispatch |
| `cmod-resolver` | Git-based dependency resolution, semver solving, lockfile generation |
| `cmod-build` | Module DAG construction, build plan IR, Clang invocation, parallel execution |
| `cmod-cache` | Content-addressed artifact caching with SHA-256 keys |
| `cmod-workspace` | Monorepo support, unified dependencies, cross-member builds |
| `cmod-security` | Hash verification, signature checking, TOFU trust model |

Key data flows:
1. **Resolution:** `cmod.toml` → dependency graph → `cmod.lock`
2. **Build:** lockfile → module DAG → Clang invocations → artifacts
3. **Cache:** cache key (SHA-256) → local cache → remote cache (optional)

## How It Compares

| Feature | cmod | CMake | Conan | vcpkg | Bazel |
|---|---|---|---|---|---|
| C++20 modules | Native | No | No | No | Partial |
| Git-native deps | Yes | No | No | No | No |
| Lockfiles | Yes | No | Partial | Partial | Yes |
| Monorepo support | Yes | Partial | No | No | Yes |
| Remote cache | Yes | No | No | No | Yes |
| Central registry | No (Git) | No | Yes | Yes | Partial |
| Setup complexity | Low | High | Medium | Medium | High |

**vs CMake** — cmod manages modules and dependencies; CMake generates build files.
**vs Conan/vcpkg** — cmod is source-first and module-native; no global binary registry.
**vs Bazel** — cmod is lightweight and Git-native; no proprietary rule language.

## Roadmap

| Phase | Status | Deliverables |
|---|---|---|
| 0 — Foundations | Done | `cmod.toml` parser, Git resolver, lockfile, CLI |
| 1 — Builds | Done | LLVM/Clang backend, module DAG, build runner |
| 2 — Scale | Done | Workspace manager, local cache, cache keys |
| 3 — Distributed | Planned | Remote cache protocol, artifact upload/download |
| 4 — Security | Planned | Signature verification, `--locked --verify` modes |
| 5 — Ecosystem | Planned | LSP integration, plugin SDK, visualization tools |

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and guidelines.

## License

Licensed under the [Apache License 2.0](LICENSE).
