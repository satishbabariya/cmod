# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **Distributed Builds** — Remote cache protocol with HTTP REST API (`HEAD`/`GET`/`PUT`), BMI distribution via Git repositories (RFC-0011), distributed build worker pool with work stealing and task scheduling (RFC-0012/0013)
- **Security** — Cryptographic signing backends: OpenPGP (GPG), SSH key signing, Sigstore cosign (RFC-0009); dependency auditing with severity levels; SBOM generation; enhanced hash verification; security policy enforcement
- **Ecosystem** — LSP server crate (`cmod-lsp`) with completion and diagnostics (RFC-0010/0016); module registry index for discovery (RFC-0015); plugin sandboxing with capability-based permissions (RFC-0018); feature resolution and optional dependency activation; conditional dependency expressions with transitive feature propagation (RFC-0017)
- **Build** — Incremental rebuild detection via persistent build state with per-node content hashes; Cargo-style colored build output with Shell abstraction; build timing visualization in graph output; support for multiple source directories and exclude patterns; additional include directories and extra compiler flags
- **Test** — Comprehensive test overhaul with parallel execution, structured output, and workspace support; test count grew from 270+ to 737+
- **CLI** — Plugin sandboxing command; enhanced status output
- **Docs** — Comprehensive user-facing documentation; enhanced examples documentation; 8 new blog posts; complete marketing website

### Fixed

- Edge case bugs in `run`, `tidy`, `add`, and `init` commands
- Workspace run command and path-dep import checking
- Windows build: normalized path separators in workspace member names, proper file URL generation
- Eliminated remaining `eprintln` calls in favor of Shell abstraction

## [0.1.0] - 2025-01-01

### Added

- **Core** — `cmod.toml` manifest parser, `cmod.lock` lockfile format, configuration loading, error model with exit codes
- **CLI** — 30+ subcommands including `init`, `add`, `remove`, `resolve`, `build`, `test`, `update`, `deps`, `cache`, `verify`, `graph`, `audit`, `status`, `explain`, `toolchain`, `vendor`, `lint`, `fmt`, `search`, `run`, `clean`, `workspace`, `sbom`, `publish`, `compile-commands`, `tidy`, `check`, `plugin`, `plan`, `emit-cmake`
- **Resolver** — Git-based dependency resolution, semver constraint solving, lockfile generation
- **Build** — LLVM/Clang backend, module DAG construction, topological sort, build plan IR, parallel build execution, source discovery
- **Cache** — Content-addressed local artifact cache with SHA-256 keys, eviction, and garbage collection
- **Workspace** — Monorepo support with unified dependency resolution, member management, cross-member builds with PCM/obj sharing
- **Security** — Trust-on-first-use (TOFU) model, hash verification, signature checking foundations
- **21 RFCs** — Complete design specification covering all planned features
