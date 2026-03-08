# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **LSP Server** — `textDocument/documentSymbol` for outline/breadcrumb view, `textDocument/references` for finding module importers, `textDocument/codeAction` with quick fixes for missing imports and syntax errors
- **LSP Build Integration** — `cmod/buildStatus` notification on save, `cmod/dependencies` / `cmod/criticalPath` / `cmod/cacheStatus` custom query methods, diagnostic propagation through module DAG, module graph caching with 30s TTL
- **Plugin SDK** — argument passing via `key=value` pairs, `min_cmod_version` validation, signed plugin verification wired into `cmod plugin run`, build hook `plugin:` prefix for dispatching hooks to plugins
- **Plugin Guide** — `docs/guide/plugins.md` with plugin.toml schema, JSON IPC protocol, capability reference, and build hook integration
- **IDE Integration Guide** — `docs/guide/ide-integration.md` with editor config for Neovim, VS Code, Emacs and custom method reference
- **Example Plugin** — `examples/plugin/` with hello-plugin demonstrating the JSON IPC protocol

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
