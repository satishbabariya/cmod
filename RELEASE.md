# cmod v0.1.0-alpha.1

**The first public release of cmod — a Cargo-inspired, Git-native package and build tool for modern C++20 modules.**

> C++ has modules now. It deserves a build tool that knows it.

This is an alpha release intended for early adopters and feedback. APIs and CLI behavior may change before the stable `v0.1.0` release.

---

## Highlights

- **C++20 modules as first-class citizens** — not headers, not textual includes. cmod understands modules, partitions, and Binary Module Interfaces (BMIs) natively.
- **Git is the registry** — dependencies are Git URLs. No central package server, no account required. `cmod add github.com/fmtlib/fmt@^10.0` just works.
- **Deterministic builds** — mandatory lockfiles pin exact commit hashes and toolchain versions. Every build is reproducible.
- **30+ CLI commands** — from `cmod init` to `cmod sbom`, covering the full development lifecycle with a Cargo-like UX.
- **LSP server** — IDE integration with document symbols, references, code actions, and build status notifications.
- **Plugin SDK** — extend cmod with sandboxed plugins using a JSON IPC protocol.

## Installation

### Pre-built binaries

```bash
# Latest release (auto-detects platform)
curl -sSf https://raw.githubusercontent.com/satishbabariya/cmod/main/install.sh | sh

# Specific version
curl -sSf https://raw.githubusercontent.com/satishbabariya/cmod/main/install.sh | sh -s -- --version v0.1.0-alpha.1
```

### From source

```bash
git clone https://github.com/satishbabariya/cmod.git
cd cmod
cargo install --path crates/cmod-cli
```

### Requirements

- **LLVM/Clang 17+** for C++ module compilation
- **Rust 1.74+** if building from source

## Download

Pre-built binaries are available for the following platforms:

| Platform | Architecture | Download |
|----------|-------------|----------|
| Linux | x86_64 (glibc) | `cmod-v0.1.0-alpha.1-x86_64-unknown-linux-gnu.tar.gz` |
| Linux | ARM64 (glibc) | `cmod-v0.1.0-alpha.1-aarch64-unknown-linux-gnu.tar.gz` |
| Linux | x86_64 (musl, static) | `cmod-v0.1.0-alpha.1-x86_64-unknown-linux-musl.tar.gz` |
| macOS | x86_64 (Intel) | `cmod-v0.1.0-alpha.1-x86_64-apple-darwin.tar.gz` |
| macOS | ARM64 (Apple Silicon) | `cmod-v0.1.0-alpha.1-aarch64-apple-darwin.tar.gz` |
| Windows | x86_64 | `cmod-v0.1.0-alpha.1-x86_64-pc-windows-msvc.zip` |
| Windows | ARM64 | `cmod-v0.1.0-alpha.1-aarch64-pc-windows-msvc.zip` |

SHA-256 checksums are provided in `checksums-v0.1.0-alpha.1.sha256`.

## What's Included

### Core (`cmod-core`)

- `cmod.toml` manifest parser with full schema validation
- `cmod.lock` lockfile format for reproducible builds
- Configuration loading and error model with structured exit codes (`0` success, `1` build failure, `2` resolution error, `3` security violation)
- Core types: `ModuleId`, `BuildType`, `Profile`, `Manifest`, `Lockfile`

### CLI (`cmod-cli`)

30+ subcommands organized by workflow:

**Project lifecycle:**
`init`, `build`, `test`, `run`, `clean`, `status`, `check`

**Dependency management:**
`add`, `remove`, `resolve`, `update`, `deps`, `tidy`, `vendor`, `search`

**Build tools:**
`graph`, `explain`, `plan`, `compile-commands`, `emit-cmake`, `lint`, `fmt`

**Cache & security:**
`cache`, `verify`, `audit`, `sbom`, `publish`

**Workspace & tooling:**
`workspace`, `toolchain`, `plugin`, `lsp`

### Dependency Resolver (`cmod-resolver`)

- Git-based dependency resolution — clone, fetch, and resolve from any Git URL
- Semver constraint solving with support for `^`, `~`, `>=`, `=`, and wildcard ranges
- Lockfile generation with exact commit hashes
- Offline mode support with `--offline` flag

### Build Orchestrator (`cmod-build`)

- LLVM/Clang backend with `clang-scan-deps` for automatic module dependency discovery
- Module DAG construction with topological sort and cycle detection
- Build plan IR generation with compile commands export
- Parallel build execution with configurable job count
- Incremental builds — content-hash-based change detection, mtime fast path
- Source discovery for `.cppm`, `.cpp`, `.ixx`, `.mpp` files
- Distributed build support with worker pool scheduling

### Artifact Cache (`cmod-cache`)

- Content-addressed local cache with SHA-256 keys
- Eviction policies and garbage collection
- Remote cache protocol (HTTP) for artifact push/pull
- BMI (Binary Module Interface) distribution

### Workspace Manager (`cmod-workspace`)

- Monorepo support with `[workspace]` configuration
- Unified dependency resolution across workspace members
- Cross-member builds with shared PCM/object artifacts
- Member management: `workspace add`, `workspace remove`, `workspace list`

### Security (`cmod-security`)

- Trust-on-first-use (TOFU) model for dependency verification
- Hash verification for all downloaded artifacts
- Signature checking with GPG, SSH, and Sigstore support
- Security policy enforcement with `--locked` and `--verify` modes
- Git URL protocol validation (blocks `file://`, custom schemes)
- Path traversal protection in cache and module ID handling

### LSP Server (`cmod-lsp`)

- `textDocument/documentSymbol` — outline and breadcrumb view
- `textDocument/references` — find all module importers
- `textDocument/codeAction` — quick fixes for missing imports and syntax errors
- `cmod/buildStatus` — build status notifications on save
- `cmod/dependencies`, `cmod/criticalPath`, `cmod/cacheStatus` — custom query methods
- Diagnostic propagation through module DAG
- Module graph caching with 30s TTL

### Plugin SDK

- Sandboxed plugin execution with capability-based permissions
- JSON IPC protocol for plugin communication
- `plugin.toml` manifest with `min_cmod_version` validation
- Signed plugin verification
- Build hook integration via `plugin:` prefix
- Argument passing via `key=value` pairs

## Configuration

`cmod.toml` is the project manifest:

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

[toolchain]
compiler = "clang"
version = ">=17"
standard = "c++23"

[build]
type = "binary"
optimization = "O2"
```

## Examples

13 example projects are included:

| Example | Description |
|---------|-------------|
| `hello` | Minimal binary, no dependencies |
| `library` | Static library with module partitions |
| `shared-lib` | Shared/dynamic library |
| `with-deps` | Git dependencies (fmt + json) |
| `path-deps` | Local path dependencies |
| `nested-deps` | Transitive dependency resolution |
| `workspace` | Multi-member monorepo |
| `with-tests` | Project with test configuration |
| `multi-binary` | Multiple binary targets |
| `header-only` | Header-only library interop |
| `include-dirs` | Custom include directory configuration |
| `ixx-modules` | MSVC-style `.ixx` module files |
| `plugin` | Plugin system with JSON IPC |

## Documentation

- **16 user guides** covering getting started, configuration, dependencies, modules, building, caching, testing, workspaces, security, toolchains, publishing, IDE integration, plugins, and CLI reference
- **21 RFCs** providing complete design specifications for all features
- Architecture diagrams, implementation roadmap, and comparison with existing tools

## By the Numbers

- **8 Rust crates** in a Cargo workspace
- **~42,000 lines** of Rust
- **828 tests** passing across all platforms
- **7 target platforms** with pre-built binaries
- **21 RFCs** defining the complete specification
- **13 example projects** demonstrating all major features

## Known Limitations

This is an alpha release. The following are known limitations:

- **Clang-only** — GCC and MSVC compiler backends are not yet implemented
- **No crates.io publishing** — cmod is distributed via GitHub Releases and `cargo install` from source
- **Remote cache** — the HTTP cache protocol is implemented but not yet battle-tested at scale
- **Editor extensions** — VS Code and CLion plugins are in development but not yet published to marketplaces

## What's Next

- Stabilize CLI interface and configuration format
- GCC and MSVC compiler backend support
- Publish VS Code and CLion extensions
- Remote cache production hardening
- Community feedback and ecosystem growth

## Feedback

This is an early release and we welcome feedback:

- **Issues:** https://github.com/satishbabariya/cmod/issues
- **Discussions:** https://github.com/satishbabariya/cmod/discussions

---

**Full Changelog:** https://github.com/satishbabariya/cmod/commits/v0.1.0-alpha.1
