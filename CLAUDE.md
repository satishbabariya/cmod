# CLAUDE.md — AI Assistant Guide for cmod

## Project Overview

**cmod** is a Cargo-inspired, Git-native package and build tool for modern C++20+ modules. It provides dependency resolution, build orchestration, workspace management, and caching — all without a central package registry.

**Status:** RFC-driven specification phase. No implementation code exists yet. The repository contains 21 RFCs and supporting design documents that define the complete system.

**Planned implementation language:** Rust (with LLVM/Clang C++ APIs for build hooks).

## Repository Structure

```
cmod/
├── CLAUDE.md                              # This file
├── todo.txt                               # Project-level TODO notes
├── .gitignore                             # Ignore rules (C++, Rust, IDE artifacts)
└── docs/                                  # All documentation and specifications
    ├── cmod_readme_vision.md              # High-level vision and core principles
    ├── cmod_architecture_diagram.md       # Layered system architecture
    ├── cmod_cli_ux_command_specification.md # Complete CLI command reference
    ├── cmod_reference_implementation_skeleton.md # Planned Rust crate structure
    ├── cmod_implementation_roadmap.md     # 6-phase development roadmap
    ├── cmod_vs_existing_tools.md          # Comparison with CMake, Conan, vcpkg, Bazel
    ├── why_cmod_exists_pitch_doc.md       # Problem statement and motivation
    └── rfc/                               # Request for Comments (21 total)
        ├── rfc_0001_cargo_style_c_modules_tooling_llvm_based.md  # Core: tooling foundation
        ├── rfc_0002_module_identity_import_rules.md              # Core: module naming
        ├── rfc_0003_lockfile_reproducible_builds.md              # Core: lockfiles
        ├── rfc_0004_build_plan_ir_module_graph_execution.md      # Core: build DAG
        ├── rfc_0005_cache_artifact_model.md                      # Tier 1: caching
        ├── rfc_0006_versioning_compatibility_lockfiles.md        # Tier 1: versioning
        ├── rfc_0007_build_graph_incremental_compilation_caching.md # Tier 1: incremental builds
        ├── rfc_0008_toolchains_targets_cross_compilation.md      # Tier 1: cross-compilation
        ├── rfc_0009_security_trust_supply_chain.md               # Tier 2: security
        ├── rfc_0010_ide_integration_developer_experience.md      # Tier 2: IDE/LSP
        ├── rfc_0011_precompiled_module_distribution.md           # Tier 3: binary distribution
        ├── rfc_0012_advanced_build_strategies_performance_optimizations.md # Tier 3
        ├── rfc_0013_distributed_builds_remote_execution.md       # Tier 3
        ├── rfc_0014_module_graph_visualization_developer_tools_enhancements.md # Tier 3
        ├── rfc_0015_cmod_ecosystem_governance_community_standards.md # Tier 4
        ├── rfc_0016_lsp_enhancements_advanced_ide_features.md    # Tier 4
        ├── rfc_0017_module_metadata_extensions_advanced_dependency_features.md # Tier 4
        ├── rfc_0018_tooling_plugins_ecosystem_utilities.md       # Tier 4
        ├── rfc_0019_workspaces_monorepos_multi_module_projects.md # Tier 4
        ├── rfc_implementation_phases.md                          # Phase timeline
        └── rfc_unified_cmod_schema.md                            # Consolidated cmod.toml schema
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

## Planned Rust Crate Structure

When implementation begins, the code will be organized as:

| Crate | Responsibility |
|---|---|
| `cmod-cli` | Command parsing, UX, argument handling |
| `cmod-core` | Config loading (`cmod.toml`), global context, error model |
| `cmod-resolver` | Git fetch, version solving, lockfile generation |
| `cmod-build` | Module DAG construction, Clang invocation, incremental logic |
| `cmod-cache` | Local and remote cache management |
| `cmod-security` | Hashing, signature verification, supply-chain validation |
| `cmod-workspace` | Monorepo and multi-module workspace support |

External dependencies: `libgit2`, LLVM/Clang driver, TOML parser.

## CLI Commands

Core commands (from `docs/cmod_cli_ux_command_specification.md`):

| Command | Purpose |
|---|---|
| `cmod init` | Initialize a new module or workspace |
| `cmod add <dep>` | Add a dependency |
| `cmod remove <name>` | Remove a dependency |
| `cmod build` | Build current module/workspace |
| `cmod test` | Run module tests |
| `cmod resolve` | Generate lockfile from dependencies |
| `cmod update` | Update dependencies |
| `cmod cache` | Manage caches |
| `cmod verify` | Verify security/integrity |
| `cmod deps` | Inspect dependency graph |

## Configuration Format

The project uses `cmod.toml` as defined in `docs/rfc/rfc_unified_cmod_schema.md`. Key sections:

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

| Phase | Goal | Key Deliverables |
|---|---|---|
| 0 — Foundations | Deterministic dependency resolution | `cmod.toml` parser, Git resolver, lockfile, CLI: `init`/`add`/`resolve` |
| 1 — Builds | Build real C++20 modules | LLVM integration, module DAG, incremental rebuilds, CLI: `build`/`deps` |
| 2 — Scale | Workspaces and caching | Workspace manager, shared cache, parallel builds |
| 3 — Distributed | CI and team acceleration | Remote cache protocol, artifact upload/download |
| 4 — Security | Supply-chain integrity | Signature verification, `--locked --verify` modes |
| 5 — Ecosystem | Adoption and tooling | LSP integration, plugin SDK, visualization tools |

## RFC Tiers

RFCs are organized by priority tier. When contributing, respect this ordering:

- **Core (must implement first):** RFC-0001 through RFC-0004, RFC-UNIFIED
- **Tier 1 (essential features):** RFC-0005 through RFC-0008
- **Tier 2 (developer experience):** RFC-0009, RFC-0010
- **Tier 3 (advanced):** RFC-0011 through RFC-0014
- **Tier 4 (ecosystem):** RFC-0015 through RFC-0019

## Conventions for AI Assistants

### When working with documentation
- All design specifications live under `docs/`. Do not create specifications elsewhere.
- RFCs follow the naming pattern `rfc_NNNN_<descriptive_name>.md` under `docs/rfc/`.
- Cross-reference RFCs by number (e.g., "as defined in RFC-0002") when referencing design decisions.
- The unified schema (`rfc_unified_cmod_schema.md`) is the canonical `cmod.toml` reference — keep it in sync with any schema changes in other RFCs.

### When implementation begins
- The implementation will be in Rust, organized as a Cargo workspace under `crates/`.
- Follow Cargo-idiomatic Rust conventions (snake_case, standard module layout).
- Each crate should have a focused responsibility matching the table above.
- Prefer the crate boundaries defined in the reference skeleton — do not merge or split crates without updating the skeleton doc.

### General guidelines
- Keep documentation concise and structured with Markdown headings and tables.
- Maintain consistency between the roadmap, RFCs, and architecture docs when making changes.
- The `.gitignore` covers C++, Rust, IDE, and build artifacts — update it when adding new tooling.
- No implementation code exists yet. If asked to start implementing, begin with Phase 0 (dependency resolution).
