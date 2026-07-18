# CLAUDE.md — AI Assistant Guide for cmod

## Project Overview

**cmod** is a Cargo-inspired, Git-native package and build tool for modern C++20+ modules. It provides dependency resolution, build orchestration, workspace management, and caching — all without a central package registry.

**Status:** Rust implementation (Phase 0-5 complete), released as v0.1.0-alpha.3; v0.1.0-alpha.4 in progress (see milestone + umbrella issue #74). The Cargo workspace has 8 crates and 872 passing tests. The 21 RFCs and design documents under `docs/` remain the canonical specification.

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
├── RELEASE.md                             # Release process
├── install.sh                             # Binary install script
├── rust-toolchain.toml                    # Pinned Rust toolchain
├── rustfmt.toml                           # Formatter config
├── clippy.toml                            # Linter config
├── .github/                               # CI (ci.yml), releases (release.yml), issue templates
├── crates/                                # Rust implementation (8 crates)
│   ├── cmod-cli/                          # CLI binary (cmod command)
│   │   ├── src/
│   │   │   ├── main.rs                    # Entry point, clap parsing, subcommand dispatch
│   │   │   └── commands/                  # One file per subcommand, plus:
│   │   │       ├── common.rs              # Shared helpers: dep clone, topo sort, artifact collection
│   │   │       ├── build.rs               # cmod build + plan + emit-cmake + lifecycle hooks
│   │   │       ├── migrate.rs             # cmod migrate (from CMake)
│   │   │       ├── plugin.rs              # cmod plugin
│   │   │       ├── plugin_sandbox.rs      # Plugin sandbox enforcement
│   │   │       └── ...                    # init, add, remove, resolve, test, run, clean, update,
│   │   │                                  #   deps, cache, verify, graph, audit, status, explain,
│   │   │                                  #   toolchain, vendor, lint, fmt, search, workspace,
│   │   │                                  #   sbom, publish, compile_commands, tidy, check, util
│   │   └── tests/                         # Integration tests: cli_integration, e2e_validation,
│   │                                      #   example_projects, real_projects
│   ├── cmod-core/                         # Core types and config (no internal deps)
│   │   └── src/
│   │       ├── config.rs                  # Global session context (Config)
│   │       ├── error.rs                   # CmodError enum + exit codes
│   │       ├── lockfile.rs                # cmod.lock parsing/writing + integrity hash
│   │       ├── manifest.rs                # cmod.toml parsing/writing + cfg() evaluator
│   │       ├── shell.rs                   # Colored status output (Shell, Verbosity)
│   │       └── types.rs                   # ModuleId, BuildType, Profile, ToolchainSpec, etc.
│   ├── cmod-resolver/                     # Dependency resolution
│   │   └── src/
│   │       ├── conditional.rs             # Transitive feature propagation, cfg() deps
│   │       ├── features.rs                # Feature resolution + cycle detection
│   │       ├── git.rs                     # Git operations (clone, fetch, tags, content hash)
│   │       ├── registry.rs                # Git-hosted search index + publish governance
│   │       ├── resolver.rs                # Resolution algorithm + lockfile generation
│   │       └── version.rs                 # Semver constraint parsing + solving
│   ├── cmod-build/                        # Build orchestration
│   │   └── src/
│   │       ├── compiler.rs                # CompilerBackend trait + Clang/GCC/MSVC backends + factory
│   │       ├── distributed.rs             # Remote worker pool for distributed builds
│   │       ├── graph.rs                   # ModuleGraph DAG + topological/critical-path sort
│   │       ├── incremental.rs             # BuildState + rebuild detection (powers cmod explain)
│   │       ├── plan.rs                    # BuildPlan IR + compile_commands generation
│   │       └── runner.rs                  # Parallel build execution + source discovery/classification
│   ├── cmod-cache/                        # Artifact caching
│   │   └── src/
│   │       ├── bmi.rs                     # BMI package export/import + compatibility keys
│   │       ├── cache.rs                   # ArtifactCache (store/get/evict, zstd, TTL/size eviction)
│   │       ├── distribution.rs            # BMI variant index + HTTP distributor
│   │       ├── key.rs                     # CacheKey computation (SHA-256, incl. compiler version)
│   │       └── remote.rs                  # RemoteCache trait + HTTP client
│   ├── cmod-workspace/                    # Workspace management
│   │   └── src/
│   │       └── workspace.rs               # WorkspaceManager (globs, unified deps, build order)
│   ├── cmod-security/                     # Supply-chain integrity
│   │   └── src/
│   │       ├── audit.rs                   # Dependency audit (unpinned commits, branch refs)
│   │       ├── hash.rs                    # Checkout/content/tree hash verification
│   │       ├── policy.rs                  # [security] policy enforcement
│   │       ├── sbom.rs                    # CycloneDX SBOM generation
│   │       ├── signing.rs                 # PGP/SSH/Sigstore artifact + BMI signing
│   │       ├── trust.rs                   # TOFU trust model + key revocation
│   │       └── verify.rs                  # Commit + signature verification
│   └── cmod-lsp/                          # LSP server (cmod lsp)
│       └── src/
│           ├── completion.rs              # Import/module-declaration completions
│           ├── diagnostics.rs             # Manifest/source diagnostics + Clang output parsing
│           └── server.rs                  # JSON-RPC server + custom cmod/* methods
├── editors/                               # Editor integrations (vscode/, clion/, shared/)
├── examples/                              # 13 example C++ projects (see examples/README.md):
│                                          #   hello, library, with-deps, workspace, path-deps,
│                                          #   header-only, include-dirs, ixx-modules, multi-binary,
│                                          #   nested-deps, plugin, shared-lib, with-tests
├── blog/                                  # Blog posts
└── docs/                                  # Design specifications
    ├── guide/                             # User guide
    ├── rfc/                               # 21 RFCs (see RFC Tiers below)
    └── *.md                               # Vision, architecture, CLI spec, roadmap, comparisons
```

## Build & Test Commands

```bash
git config core.hooksPath .githooks                # Enable fmt/clippy git hooks (once per clone)
cargo check                                       # Type-check all crates
cargo build                                        # Compile all crates
cargo test                                         # Run all tests
cargo clippy --all-targets -- -D warnings          # Lint all code
cargo fmt --all --check                            # Check formatting
cargo build --release                              # Release build
cargo run -- <subcommand>                          # Run the CLI
```

### Local gotchas

- Apple clang cannot build C++20 modules — for local E2E runs set `CXX=/opt/homebrew/opt/llvm@18/bin/clang++` and `SCAN_DEPS=.../clang-scan-deps` (see `docs/guide/toolchains.md#compiler-detection`).
- CI caches save only on `main` (`save-if`); PR branches restore them. Do not re-add per-branch cache saves — 1,270 stale caches once saturated the 10GB quota and caused the "slow Windows CI" symptom (#80).
- PRs are squash-merged: never `git rebase` a stacked branch across a squash — rebuild it (`git checkout -B <branch> origin/main` + cherry-pick).
- Real-world validation ports live under `github.com/cmod-ecosystem` on `cmod-support` branches (tracker: issue #22).

## Key Design Decisions

- **Git is the registry.** Module identity is bound to Git URLs (e.g., `github.com/fmtlib/fmt`). No central package server.
- **Three compiler backends.** Clang (reference, `clang-scan-deps` P1689 discovery), GCC 14+ (`-fmodules-ts`, module-mapper CMIs), and MSVC VS2022 (`/interface`, `/ifcOutput`, `cl /scanDependencies`) — all constructed via `make_backend(Compiler, &BackendConfig)`; the build pipeline only sees `dyn CompilerBackend`. BMI extensions differ per backend (`.pcm`/`.gcm`/`.ifc`) and flow through `bmi_extension()`.
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
| `cmod-core` | `Config`, `Manifest`, `Lockfile`, `CmodError`, `ModuleId`, `Shell` | Config loading, TOML parsing, error model, core types, terminal output |
| `cmod-cli` | `Cli`, `Commands` | clap-based CLI, subcommand dispatch, integration tests |
| `cmod-resolver` | `Resolver`, `ResolvedDep`, `RegistryClient` | Git fetch, semver solving, features, lockfile generation |
| `cmod-build` | `ModuleGraph`, `BuildPlan`, `BuildRunner`, `ClangBackend`, `BuildState`, `WorkerPool` | DAG construction, Clang invocation, parallel/incremental/distributed builds |
| `cmod-cache` | `ArtifactCache`, `CacheKey`, `RemoteCache`, `BmiPackage` | Content-addressed caching, remote cache, BMI distribution |
| `cmod-workspace` | `WorkspaceManager`, `WorkspaceMember` | Monorepo loading, unified deps, member management |
| `cmod-security` | `TrustDb`, `SecurityPolicy`, `SigningConfig` | TOFU trust, hash/signature verification, signing, audit, SBOM |
| `cmod-lsp` | `LspServer`, `CompletionProvider`, `DiagnosticsEngine` | LSP over stdio: completions, diagnostics, custom `cmod/*` methods |

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
| `cmod cache status\|clean\|gc\|push\|pull\|export\|import\|inspect\|status-json` | Manage the build cache |
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
| `cmod migrate cmake` | Migrate a CMake project to cmod |
| `cmod lsp` | Start the LSP server (stdio) for editor integration |

### Global Flags

`--locked`, `--offline`, `--verbose`, `--quiet`, `--target <triple>`, `--features <list>`, `--no-default-features`, `--no-cache`, `--untrusted`

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
| 3 — Distributed | **Implemented** | Remote cache protocol (HTTP), artifact push/pull, BMI distribution |
| 4 — Security | **Implemented** | GPG/SSH/Sigstore signing, TOFU trust model, `--locked --verify` modes |
| 5 — Ecosystem | **Implemented** | LSP server, plugin SDK with sandbox, graph visualization (ASCII/DOT/JSON) |

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
- All cross-crate dependencies flow downward: `cli → {resolver, build, cache, workspace, security, lsp} → core`.
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
