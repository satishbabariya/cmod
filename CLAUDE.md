# CLAUDE.md — AI Assistant Guide for cmod

## Project Overview

**cmod** is a Cargo-inspired, Git-native package and build tool for modern C++20+ modules. It provides dependency resolution, build orchestration, workspace management, and caching — all without a central package registry.

**Status:** Initial Rust implementation (Phase 0-2). The Cargo workspace compiles and has 48 passing unit tests. The 21 RFCs and design documents under `docs/` remain the canonical specification.

**Implementation language:** Rust (with LLVM/Clang C++ APIs for build hooks).

## Repository Structure

```
cmod/
├── Cargo.toml                             # Workspace root
├── Cargo.lock                             # Rust dependency lockfile
├── CLAUDE.md                              # This file
├── todo.txt                               # Project-level TODO notes
├── .gitignore                             # Ignore rules (C++, Rust, IDE artifacts)
├── crates/                                # Rust implementation
│   ├── cmod-cli/                          # CLI binary (cmod command)
│   │   └── src/
│   │       ├── main.rs                    # Entry point, clap argument parsing
│   │       └── commands/                  # Subcommand implementations
│   │           ├── init.rs                # cmod init
│   │           ├── add.rs                 # cmod add
│   │           ├── remove.rs              # cmod remove
│   │           ├── resolve.rs             # cmod resolve
│   │           ├── build.rs               # cmod build
│   │           ├── test.rs                # cmod test
│   │           ├── update.rs              # cmod update
│   │           ├── deps.rs                # cmod deps
│   │           ├── cache.rs               # cmod cache status/clean
│   │           └── verify.rs              # cmod verify
│   ├── cmod-core/                         # Core types and config
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── config.rs                  # Global session context (Config)
│   │       ├── error.rs                   # CmodError enum + exit codes
│   │       ├── lockfile.rs                # cmod.lock parsing/writing
│   │       ├── manifest.rs                # cmod.toml parsing/writing
│   │       └── types.rs                   # ModuleId, BuildType, Profile, etc.
│   ├── cmod-resolver/                     # Dependency resolution
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── git.rs                     # Git operations (clone, fetch, tags)
│   │       ├── resolver.rs                # Resolution algorithm + lockfile generation
│   │       └── version.rs                 # Semver constraint parsing + solving
│   ├── cmod-build/                        # Build orchestration
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── compiler.rs                # CompilerBackend trait + ClangBackend
│   │       ├── graph.rs                   # ModuleGraph DAG + topological sort
│   │       ├── plan.rs                    # BuildPlan IR generation
│   │       └── runner.rs                  # Build execution + source discovery
│   ├── cmod-cache/                        # Artifact caching
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── cache.rs                   # ArtifactCache (store/get/evict/clean)
│   │       └── key.rs                     # CacheKey computation (SHA-256)
│   └── cmod-workspace/                    # Workspace management
│       └── src/
│           ├── lib.rs
│           └── workspace.rs               # WorkspaceManager (load/validate/add member)
├── docs/                                  # Design specifications
│   ├── cmod_readme_vision.md
│   ├── cmod_architecture_diagram.md
│   ├── cmod_cli_ux_command_specification.md
│   ├── cmod_reference_implementation_skeleton.md
│   ├── cmod_implementation_roadmap.md
│   ├── cmod_vs_existing_tools.md
│   ├── why_cmod_exists_pitch_doc.md
│   └── rfc/                               # 21 RFCs (see RFC Tiers below)
└── tests/                                 # Integration tests (future)
```

## Build & Test Commands

```bash
cargo check              # Type-check all crates
cargo build              # Compile all crates
cargo test               # Run all 48 unit tests
cargo build --release    # Release build
cargo run -- <subcommand>  # Run the CLI (e.g., cargo run -- init)
```

## Key Design Decisions

- **Git is the registry.** Module identity is bound to Git URLs (e.g., `github.com/fmtlib/fmt`). No central package server.
- **LLVM/Clang first.** Uses `clang-scan-deps` for module dependency discovery. GCC/MSVC support planned later via compiler abstraction.
- **Lockfiles are mandatory.** `cmod.lock` pins exact commit hashes and toolchain versions for reproducible builds.
- **Modules are first-class.** C++20 modules, partitions, and BMIs (Binary Module Interfaces) — not header-based compilation.
- **Build graph known upfront.** The full module DAG is resolved before any compilation begins.

## Architecture

The system is a layered pipeline:

```
User / IDE → CLI → Dependency Resolver → Workspace Manager → Build Orchestrator → LLVM/Clang → Artifact Cache → Security/Verification
```

Key data flows:
1. **Resolution:** `cmod.toml` → dependency graph → `cmod.lock`
2. **Build:** lockfile → build DAG → Clang invocations → artifacts
3. **Cache:** cache key → local cache → remote cache (optional)

## Crate Responsibilities

| Crate | Key Types | Responsibility |
|---|---|---|
| `cmod-core` | `Config`, `Manifest`, `Lockfile`, `CmodError`, `ModuleId` | Config loading, TOML parsing, error model, core types |
| `cmod-cli` | `Cli`, `Commands` | clap-based CLI, subcommand dispatch |
| `cmod-resolver` | `Resolver`, `ResolvedDep` | Git fetch, semver solving, lockfile generation |
| `cmod-build` | `ModuleGraph`, `BuildPlan`, `BuildRunner`, `ClangBackend` | DAG construction, Clang invocation, build execution |
| `cmod-cache` | `ArtifactCache`, `CacheKey` | Content-addressed caching, SHA-256 keys |
| `cmod-workspace` | `WorkspaceManager`, `WorkspaceMember` | Monorepo loading, unified deps, member management |

## CLI Commands

| Command | Description | Implementation |
|---|---|---|
| `cmod init [--workspace]` | Initialize a new module or workspace | `commands/init.rs` |
| `cmod add <dep>[@version]` | Add a dependency | `commands/add.rs` |
| `cmod remove <name>` | Remove a dependency | `commands/remove.rs` |
| `cmod resolve` | Resolve deps → lockfile | `commands/resolve.rs` |
| `cmod build [--release]` | Build module/workspace | `commands/build.rs` |
| `cmod test [--release]` | Build and run tests | `commands/test.rs` |
| `cmod update [name]` | Update dependencies | `commands/update.rs` |
| `cmod deps [--tree]` | Inspect dependency graph | `commands/deps.rs` |
| `cmod cache status\|clean` | Manage build cache | `commands/cache.rs` |
| `cmod verify` | Verify integrity | `commands/verify.rs` |

Global flags: `--locked`, `--offline`, `--verbose`, `--target <triple>`

Exit codes: `0` success, `1` build failure, `2` resolution error, `3` security violation.

## Configuration Format

`cmod.toml` (see `docs/rfc/rfc_unified_cmod_schema.md` for full spec):

```toml
[package]       # name, version, edition, authors, license
[module]        # module name (reverse-domain Git path), root file
[dependencies]  # Git URL = version constraint
[toolchain]     # compiler, version, C++ standard, stdlib, target
[build]         # type, optimization, LTO, parallelism
[workspace]     # member modules (for monorepos)
```

Module names follow reverse-domain Git path format: `com.github.user.my_math`.

## Implementation Roadmap

| Phase | Status | Key Deliverables |
|---|---|---|
| 0 — Foundations | **Implemented** | `cmod.toml` parser, Git resolver, lockfile, CLI commands |
| 1 — Builds | **Implemented** | LLVM/Clang backend, module DAG, build plan IR, build runner |
| 2 — Scale | **Implemented** | Workspace manager, local cache, cache keys |
| 3 — Distributed | Planned | Remote cache protocol, artifact upload/download |
| 4 — Security | Planned | Signature verification, `--locked --verify` modes |
| 5 — Ecosystem | Planned | LSP integration, plugin SDK, visualization tools |

## RFC Tiers

RFCs are organized by priority tier. When contributing, respect this ordering:

- **Core (must implement first):** RFC-0001 through RFC-0004, RFC-UNIFIED
- **Tier 1 (essential features):** RFC-0005 through RFC-0008
- **Tier 2 (developer experience):** RFC-0009, RFC-0010
- **Tier 3 (advanced):** RFC-0011 through RFC-0014
- **Tier 4 (ecosystem):** RFC-0015 through RFC-0019

## Conventions for AI Assistants

### Working with the implementation
- The implementation is in Rust, organized as a Cargo workspace under `crates/`.
- Follow Cargo-idiomatic Rust conventions (snake_case, standard module layout).
- Each crate has a focused responsibility — do not merge or split crates without updating this doc.
- All cross-crate dependencies flow downward: `cli → {resolver, build, cache, workspace} → core`.
- `cmod-core` has no internal crate dependencies and is the foundation.
- Run `cargo test` after making changes. All 48 tests must pass.
- Run `cargo check` before committing to catch compilation errors early.

### Working with documentation
- All design specifications live under `docs/`. Do not create specifications elsewhere.
- RFCs follow the naming pattern `rfc_NNNN_<descriptive_name>.md` under `docs/rfc/`.
- Cross-reference RFCs by number (e.g., "as defined in RFC-0002") when referencing design decisions.
- The unified schema (`rfc_unified_cmod_schema.md`) is the canonical `cmod.toml` reference — keep it in sync with any schema changes in other RFCs.

### General guidelines
- Keep documentation concise and structured with Markdown headings and tables.
- Maintain consistency between the roadmap, RFCs, architecture docs, and implementation.
- The `.gitignore` covers C++, Rust, IDE, and build artifacts — update it when adding new tooling.
- Prefer extending existing modules over creating new files.
- `cmod-security` crate is planned but not yet created — create it when implementing Phase 4.
