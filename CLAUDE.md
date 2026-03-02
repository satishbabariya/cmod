# CLAUDE.md — AI Assistant Guide for cmod

## Project Overview

**cmod** is a Cargo-inspired, Git-native package and build tool for modern C++20+ modules. It provides dependency resolution, build orchestration, workspace management, and caching — all without a central package registry.

**Status:** Initial Rust implementation (Phase 0-2). The Cargo workspace compiles and has 270+ passing tests. The 21 RFCs and design documents under `docs/` remain the canonical specification.

**Implementation language:** Rust (with LLVM/Clang C++ APIs for build hooks).

## Repository Structure

```
cmod/
├── Cargo.toml                             # Workspace root
├── Cargo.lock                             # Rust dependency lockfile
├── CLAUDE.md                              # This file
├── README.md                              # Project page
├── LICENSE                                # Apache-2.0
├── CONTRIBUTING.md                        # Contributor guide
├── SECURITY.md                            # Security policy
├── CHANGELOG.md                           # Release notes
├── rust-toolchain.toml                    # Pinned Rust toolchain
├── rustfmt.toml                           # Formatter config
├── clippy.toml                            # Linter config
├── .gitignore                             # Ignore rules
├── .github/                               # GitHub configuration
│   ├── workflows/
│   │   ├── ci.yml                         # CI: fmt, clippy, test, msrv
│   │   └── release.yml                    # Binary release on tag push
│   ├── ISSUE_TEMPLATE/
│   │   ├── bug_report.yml
│   │   └── feature_request.yml
│   └── pull_request_template.md
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
│   │           ├── run.rs                 # cmod run
│   │           ├── clean.rs               # cmod clean
│   │           ├── update.rs              # cmod update
│   │           ├── deps.rs                # cmod deps
│   │           ├── cache.rs               # cmod cache
│   │           ├── verify.rs              # cmod verify
│   │           ├── graph.rs               # cmod graph
│   │           ├── audit.rs               # cmod audit
│   │           ├── status.rs              # cmod status
│   │           ├── explain.rs             # cmod explain
│   │           ├── toolchain.rs           # cmod toolchain
│   │           ├── vendor.rs              # cmod vendor
│   │           ├── lint.rs                # cmod lint
│   │           ├── fmt.rs                 # cmod fmt
│   │           ├── search.rs              # cmod search
│   │           ├── workspace.rs           # cmod workspace
│   │           ├── sbom.rs                # cmod sbom
│   │           ├── publish.rs             # cmod publish
│   │           ├── compile_commands.rs    # cmod compile-commands
│   │           ├── tidy.rs                # cmod tidy
│   │           ├── check.rs              # cmod check
│   │           └── plugin.rs              # cmod plugin
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
│   ├── cmod-workspace/                    # Workspace management
│   │   └── src/
│   │       ├── lib.rs
│   │       └── workspace.rs               # WorkspaceManager (load/validate/add member)
│   └── cmod-security/                     # Supply-chain integrity
│       └── src/
│           ├── lib.rs
│           ├── trust.rs                   # TOFU trust model
│           ├── verify.rs                  # Hash/signature verification
│           └── policy.rs                  # Security policy enforcement
├── docs/                                  # Design specifications
│   ├── cmod_readme_vision.md
│   ├── cmod_architecture_diagram.md
│   ├── cmod_cli_ux_command_specification.md
│   ├── cmod_reference_implementation_skeleton.md
│   ├── cmod_implementation_roadmap.md
│   ├── cmod_vs_existing_tools.md
│   ├── why_cmod_exists_pitch_doc.md
│   └── rfc/                               # 21 RFCs (see RFC Tiers below)
└── tests/                                 # Integration tests
```

## Build & Test Commands

```bash
cargo check                                       # Type-check all crates
cargo build                                        # Compile all crates
cargo test                                         # Run all tests
cargo clippy --all-targets -- -D warnings          # Lint all code
cargo fmt --all --check                            # Check formatting
cargo build --release                              # Release build
cargo run -- <subcommand>                          # Run the CLI
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
| `cmod-security` | `TrustStore`, `Verifier`, `SecurityPolicy` | TOFU trust, hash/signature verification, policy enforcement |

## CLI Commands

### Core Workflow

| Command | Description |
|---|---|
| `cmod init [--workspace]` | Initialize a new module or workspace |
| `cmod build [--release] [--jobs N]` | Build the current module or workspace |
| `cmod test [--release]` | Build and run tests |
| `cmod run [--release] [-- args]` | Build and run the project binary |
| `cmod clean` | Remove build artifacts |

### Dependency Management

| Command | Description |
|---|---|
| `cmod add <dep>[@version]` | Add a dependency |
| `cmod remove <name>` | Remove a dependency |
| `cmod resolve` | Resolve dependencies and generate/update lockfile |
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

### Cache, Security & Packaging

| Command | Description |
|---|---|
| `cmod cache status\|clean\|gc\|push\|pull` | Manage the build cache |
| `cmod verify [--signatures]` | Verify integrity and security |
| `cmod audit` | Audit dependencies for security issues |
| `cmod sbom [--output <file>]` | Generate a Software Bill of Materials |
| `cmod publish [--dry-run]` | Publish a release (create a Git tag) |

### Workspace & Project

| Command | Description |
|---|---|
| `cmod workspace list\|add\|remove` | Manage workspace members |
| `cmod status` | Show project status overview |
| `cmod check` | Validate module naming and structure |
| `cmod toolchain show\|check` | Manage the active toolchain |
| `cmod plugin list\|run` | Manage plugins |

### Global Flags

`--locked`, `--offline`, `--verbose`, `--target <triple>`, `--features <list>`, `--no-default-features`, `--no-cache`, `--untrusted`

### Exit Codes

`0` success, `1` build failure, `2` resolution error, `3` security violation.

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
- All cross-crate dependencies flow downward: `cli → {resolver, build, cache, workspace, security} → core`.
- `cmod-core` has no internal crate dependencies and is the foundation.
- Run `cargo test` after making changes. All tests must pass.
- Run `cargo check` before committing to catch compilation errors early.
- Run `cargo clippy --all-targets -- -D warnings` to catch lint issues.
- Run `cargo fmt --all --check` to verify formatting.

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
