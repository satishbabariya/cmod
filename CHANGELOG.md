# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0-alpha.3] - 2026-07-16

Offline & distribution polish, plus compiler-backend groundwork. Closes out the full v0.1.0-alpha.3 milestone (#36): all 15 planned items plus the three stretch goals.

### Fixed

- **`cmod vendor --sync` re-runs succeed** — the second sync no longer fails with "exists and is not an empty directory"; existing clones are reused (fetch + hard reset to the locked commit) and stale non-repo leftovers are cleared. (#55, fixes #38)
- **Test discovery matches root-relative glob patterns** — `[test] test_patterns = ["tests/**/*.cpp"]` never matched because discovered sources are absolute paths; globs now also match against the project-root-relative path, so configured projects stop reporting "No tests found". (#56, fixes #39)
- **`cmod cache push` reports upload failures** — errors were silently swallowed and every artifact counted as pushed; failures are now counted, warned, and push exits nonzero when nothing uploads. (#61)
- **`cmod cache push` works on Windows** — path splitting on `/` meant zero artifacts were ever uploaded from Windows; now splits with `Path::components`. (#61)
- **`[cache]` settings are honored end-to-end** — `auth_token_env`, `timeout`, and `retries` were documented and parsed but never applied; all remote-cache clients (build, push, pull) now route through a shared constructor that wires them. (#63, fixes #45)
- **Friendlier `cmod graph` at a workspace root** — explains graphs are per-member and lists member names instead of "no source files found". (#59, fixes #42)

### Changed

- **`cmod cache export` is all-positional** — `cmod cache export <MODULE> <KEY> <OUTPUT>`; the `-o/--output` flag is no longer accepted (consistency with `cache inspect`). **Breaking CLI change.** (#57, fixes #40)
- **`cmod test --format json/junit` output corrected for CI consumers** — JSON: `summary` gains `total` and `success`, `failed` no longer double-counts timeouts, compile failures include their reason. JUnit: suite-level `failures`/`errors`/`skipped`/`time` attributes (read by Jenkins/GitLab), timeouts and compile failures map to `<error>`, testcases gain `classname`, and XML-invalid control characters (ANSI escapes) are stripped. **Schema change for existing consumers.** (#60, fixes #43)
- **`[toolchain] compiler = "gcc"` / `"msvc"` now fail fast** with a clear not-implemented error — previously the setting was silently ignored and clang was used. (#66)
- **Cache keys derive from a full backend fingerprint** — LTO mode, optimization level, and sysroot were previously missing from cache keys, so toggling them could reuse stale artifacts. **One-time cache miss on first build after upgrade.** (#66)
- **Remote-cache downloads are atomic** — artifacts download to a `.part` sibling and rename into place, so interrupted transfers can't poison the cache. (#63)
- **MSRV raised to Rust 1.80** (required by the openssl security fix). (#37)

### Added

- **`cmod workspace add --scaffold`** — asserts creation intent: errors if the member directory already exists, making scripted workspace management auditable. Default inference behavior is unchanged. (#58, fixes #41)
- **Compiler backend abstraction** — `BackendConfig` + `make_backend()` factory; `BuildRunner` and `compile_commands` work against `dyn CompilerBackend`, so GCC/MSVC backends slot in without touching the pipeline. `compile_commands.json` now records the resolved compiler path. (#66, closes #47)
- **`MsvcBackend` skeleton** — real MSVC flag mapping and trait-shape validation (`.ifc` BMI naming via new `bmi_extension()`, `cl /scanDependencies` P1689 note); compilation deliberately stubbed. (#67, closes #48)
- **Git hooks** — `.githooks/` with fmt on pre-commit and clippy on pre-push; enable with `git config core.hooksPath .githooks`. (#68, closes #50)
- **Cross-target smoke tests in CI** — `--target` with a non-host triple is exercised through plan generation and emitted flags on every CI leg plus a dedicated job. (#70, closes #54)

### Docs

- **Remote-cache server guide** (`docs/guide/remote-cache.md`) — the HEAD/GET/PUT protocol spec plus recipes validated against real servers in Docker: nginx read-write, Caddy read-only, and a stdlib-Python dev server. (#61, closes #44)
- **Compiler detection** (`docs/guide/toolchains.md`) — the verified `CXX`/`SCAN_DEPS` → PATH → literal resolution order and the macOS Apple-clang note; README/CONTRIBUTING point at it. (#64, closes #49)
- **crates.io publishing decision doc** — recommendation: don't publish; `cmod`/`cmod-core` are already taken by an unrelated active project. (#65, closes #46)
- **Search registry design doc** — the client/index/governance code already exists; the phased plan covers bootstrapping the index repo, PR-based submissions, and scale. (#71, closes #53)
- **VS Code extension publishing runbook** (`editors/vscode/PUBLISHING.md`) — the release workflow was fully built but never run; documents the owner setup (marketplace publisher, PAT secrets) and tag-driven flow. (#72, refs #52)
- **CONTRIBUTING refresh** — commit conventions, CI job matrix, release process, 8-crate layout. (#69, closes #51)

### Dependencies

- **Resolved all 11 open Dependabot alerts**: `openssl` 0.10.75 → 0.10.80 (8 alerts incl. 5 high), `rustls-webpki` 0.103.10 → 0.103.13 (3 alerts incl. CRL panic DoS). (#37)

### Internal

- clippy 1.97 clean across three new lints that had turned main's CI red. (#37, #61)
- `cmod test` output rendering extracted into unit-testable `render_json`/`render_junit`. (#60)

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
