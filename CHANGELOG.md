# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0-alpha.2] - 2026-04-21

### Fixed

- **`cmod vendor` accepts Git-URL dep names** — the path-safety check previously rejected every dep whose name contained `/` (i.e. `github.com/owner/repo` — the whole Git-URL convention), blocking every offline workflow. Vendor now encodes names to the same underscore-separated on-disk form the resolver already uses (`vendor/github.com_owner_repo/`) while preserving the Git-URL key in `vendor/config.toml`. (#31, BUG-01)
- **`cmod verify --signatures` looks up repos at the right path** — previously path-joined raw package names, hitting `build/deps/github.com/owner/repo` instead of the actual `build/deps/github.com_owner_repo`. Now shares `sanitize_package_name_for_path` with the resolver and vendor. (#31, BUG-02)
- **`cmod workspace add <dir>` registers existing member directories** — adding a pre-existing dir with its own `cmod.toml` now registers it instead of rejecting with "already exists". Missing dirs are still scaffolded; orphan dirs without a manifest are rejected with a clear error; duplicate members are caught up-front. (#31, BUG-03)
- **`cmod run --release` locates the release binary** — `run` was resolving the debug path even when building with `--release`. Now computes the profile-specific directory directly from the flag. (#31, BUG-04)
- **Release builds rebuild path deps in release mode** — `cmod build --release` previously linked `libs/<dep>/build/debug/libX.a` into the release binary, mixing profiles. Profile, locked, offline, and target settings now propagate from the parent's `Config` into each path-dep sub-build. **First release build after upgrade will recompile path deps from scratch.** (#31, BUG-05)

### Changed

- **Cache keys include the compiler version.** `ClangBackend::detect_version()` parses `clang --version` once per build; `BuildRunner` memoizes and feeds it into both `CacheKey::compute` and `ArtifactMetadata`. PCMs produced by different Clang majors no longer collide in the local cache — fixes spurious "module file uses an older format" errors after a Clang upgrade. **One-time cache miss on first build after upgrade** (expected and harmless; `cmod cache clean` optional). (#31)
- **Integration test harness** `example_projects::copy_dir_recursive` now skips `build/`, `target/`, `.cache`, `.git`, `vendor`, `compile_commands.json`, and `CMakeLists.txt` so the suite is stable regardless of the developer's local build artefacts. (#31, BUG-07)

### Added

- **`cmod cache push/pull --remote <URL>`** — per-invocation override for the remote cache endpoint, complementing the manifest-level `[cache].shared_url`. Useful for CI and ad-hoc inspection. (#31, BUG-06)
- **`cmod_core::types::is_acceptable_package_name` / `sanitize_package_name_for_path`** — shared helpers so resolver, vendor, verify, and policy agree on on-disk dep directory naming. (#31)

### Dependencies

- Bump `rustls-webpki` in the cargo group (Dependabot #29).

### Internal

- clippy 1.95 clean: collapse a nested `if` into a match guard in `cmod-lsp::CodeActionHandler`. (#32)

## [0.1.0-alpha.1] - 2026-03-14

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
