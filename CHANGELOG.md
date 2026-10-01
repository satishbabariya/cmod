# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0-alpha.7] - 2026-10-01

The release that makes incremental builds trustworthy, and fast. Up to alpha.6, `cmod build` could report success while linking objects built from old inputs: an edited header never rebuilt anything, even with `--force`; a module's importers rebuilt one build late; and a dependency's module change never reached its importers at all. All three are fixed, and with rebuilds now driven by real inputs, dependencies build incrementally, unchanged links are skipped, and scanning runs in parallel: a no-op build of a Catch2 consumer went from ~5.2 s to ~1.5 s on 4 cores. `cmod build --dry-run` and a rewritten `cmod explain` show what a build would do and why.

### Fixed

- **Editing a header rebuilds the files that include it** — neither the incremental check nor the cache key looked at included headers. After a header edit, `cmod build` said "up-to-date", and `cmod build --force` restored the object built with the old header from the cache, as did any other project with the same source file, through the remote cache too. The compiler now reports each TU's headers (`-MD` for Clang and GCC, `/sourceDependencies` for MSVC). Build state records them, and cache keys cover their contents through a per-source include manifest, as in ccache's direct mode. Touching a header without changing it rebuilds nothing. Found by #131.
- **A module change rebuilds its importers in the same build** — importers were checked against the previous build's state, so they stayed "up-to-date" and kept the old inlined code until the next build.
- **A dependency's module change rebuilds its importers** — imports of git, path and workspace dependencies' modules were dropped from the graph, so changing a dependency's interface left importing TUs stale until their own source changed, and their cache keys did not change either. Dependency BMIs are now inputs of the TUs that import them (`external_imports` in `cmod plan`).
- **A header saved during a build no longer hides the change from the next build** — build state stored one record per header path. When a header changed mid-build, a TU rebuilt against the new version replaced the record that up-to-date TUs, compiled against the old version, also pointed to. The next build then found them up to date and linked their stale objects. Each header version now gets its own record. Headers touched without changing also keep their new mtime, so they are hashed once instead of on every build.
- **Runner-pushed remote cache entries can be restored** — `cmod build` uploaded artifacts without their `metadata.json`, which restores require. It is now uploaded last, after the artifacts.

The first build after upgrading recompiles everything: old build state has no header lists, and cache keys changed. Lockfile format unchanged. See `docs/adr/0004-track-included-headers-and-dependency-bmis.md`.

### Added

- **`cmod build --dry-run` (`-n`)** — prints each build step (compile or link, for the package and its dependencies) and whether it would run, with the reason: source changed, an included header changed, an imported module will be rebuilt, flags changed, an output is missing, link inputs changed. It builds nothing, writes no build state or lockfile, fetches no dependencies, and runs no hooks.

### Changed

- **`cmod explain` reports the reason the build would act on** — it compared a cache key built from placeholder inputs (`compiler = "clang"`, no compiler version) and object paths that did not match the build's, so it reported "cache miss" and missing outputs for every module, even one fully up to date. It now runs a dry run and shows that module's reasons.
- **Dependencies build incrementally, and unchanged links are skipped** — every build deleted each git dependency's `obj/` and `pcm/` directories, so all of its objects were rebuilt or restored from the cache and re-archived, and every build re-ran `ar` and the final link. Outputs that the current plan does not produce (from deleted sources, or from a checkout that moved) are now pruned instead, and a link is skipped when the objects, dependency archives and flags it would read are unchanged and its output exists. A no-op build of a Catch2 consumer (scanner time excluded) went from ~1.1 s to ~0.35 s. Pruning also stops `cmod test` from linking objects of deleted sources. Build state now also records which compiler executable and version built each object, so switching `CXX` to another installation, or upgrading one in place, rebuilds and relinks instead of keeping the old outputs.
- **Module scanning runs in parallel** — `clang-scan-deps` ran once per source, one at a time, on every build, for the package and for each dependency. It now runs on all cores. A no-op build of a Catch2 consumer (107 sources) went from 5.2 s to 2.3 s on 4 cores; the build plan is byte-identical.
- The `e2e_validation`, `example_projects` and `real_projects` compile tests also run on Linux with `clang++` on PATH. They had only looked for Homebrew LLVM, so the Linux E2E job skipped all of them.

## [0.1.0-alpha.6] - 2026-10-01

The release that fixes a GCC project's first git dependency. Alpha.5 shipped the GCC backend for the root package but left git and path dependencies on clang's command line and `.pcm`-only BMI lookup; both are real breaks for any `compiler = "gcc"` project and are fixed here.

### Fixed

- **Dependencies build with the root package's compiler** — git and path dependencies were built with the compiler in their own `[toolchain]`, and every cmod-ecosystem port says clang, so a `compiler = "gcc"` project handed clang's `--target=` and `--precompile` to g++ and failed on its first dependency. BMIs only work with the compiler that wrote them, so the root's compiler now builds the whole graph. A dependency's own `compiler` is reported under `--verbose` and not used. The compiler family is part of the cache key, so the first build after upgrading is a cold one for any dependency whose manifest names a different compiler from the root. Cache-key format and lockfile unchanged. See `docs/adr/0002-build-dependencies-with-the-root-compiler.md`. (#111, #127)
- **GCC consumers can `import` a dependency's module** — cmod looked for each dependency's BMI as `<module>.pcm`, but GCC writes `.gcm` and MSVC `.ifc`, so under `compiler = "gcc"` a consumer's `import nlohmann.json;` got no mapping and g++ stopped with `Unknown CMI mapping`. BMIs of git, path and workspace dependencies, and those `cmod test` and `cmod compile-commands` read, are now found by the backend's own extension. Clang builds are unchanged. (#114, #128)

## [0.1.0-alpha.5] - 2026-09-30

The release that makes `cmod test` tell the truth. The newest binary `install.sh` gave you (alpha.4) still exited 0 when every test failed to compile; that fix, and everything else on `main` since alpha.4, ships here. The GCC-backend defects the ecosystem sweep in #108 found (#111 dependency compiler routing, #114 BMI extension collection) are real but are not in this release — they need cache-key and multi-site changes and go to alpha.6.

### Added

- **`cmod registry validate <path> [--against <base>]`** — index-wide governance validation (`validate_for_publishing` on every entry and version, structural checks) plus a no-deletions policy against a base revision: the gate the `cmod-registry` validation Action runs. `cmod publish` without push access now prints a ready-made `RegistryEntry` JSON fragment with PR instructions, sharing construction with `publish_module` so the fallback can't drift. Closes #79. (#102)
- **`cmod publish` explains the tag-only path** — publishing without `[publish] registry` configured used to tag silently, leaving first-time publishers unsure whether listing happened. It now prints what was skipped and how to opt in. Closes #101. (#103)
- **GCC and MSVC E2E jobs build real ecosystem ports, not just synthetic projects** — fmt, nlohmann/json, spdlog and Catch2 now go through `init → add → resolve → build → run` under `compiler = "gcc"` on every PR (#108); json, CLI11 and rapidjson do the same under `compiler = "msvc"` (#105, closes #99). The GCC sweep is what found #111, #112, #113 and #114 below.

### Fixed

- **`cmod test` builds tests with the configured compiler** — test binaries were built with a hand-written clang command line, so under `compiler = "gcc"` g++ rejected `--target=` and no test compiled. Tests now build through `CompilerBackend::test_binary_command`, like `cmod build`: GCC gets `-fmodules-ts` and a module mapper for the `.gcm` files, and the clang command line is unchanged. `--coverage` flags come from the backend, and with gcc or msvc it stops with an error that names the compiler. `cmod test` with msvc reports that it is not supported yet. See `docs/adr/0001-compile-tests-through-compiler-backend.md`. (#112)
- **Branch/rev/tag dependencies honour `cmod.lock`** — lock reuse compared the semver `version` requirement against the locked pseudo-version (`^0.1` vs `0.0.0-<date>-<sha>`), which never matches, so every `cmod resolve` silently moved branch deps to the upstream head. Pinned deps now reuse the lock when it matches the pin; a branch keeps its locked commit while that commit is still on the branch, and `cmod update` advances it. `--locked` and `--offline` apply the same rules. Lock format unchanged. See `docs/guide/dependencies.md#pinned-dependencies-branch-rev-tag`. (#118)
- **`cmod test` fails when a test does not compile** — compile failures were only warned about and never reached the summary, so a run where every test failed to compile printed `test result: ok. 0 passed, 0 failed` and exited 0. Each test that fails to compile now counts as failed, in human, JSON, JUnit and TAP output, and the exit code is non-zero. This is the correctness bug alpha.5 exists to ship. (#113)
- **`install.sh` picks the newest cmod release, not the VS Code extension tag** — `get_latest_version()` called `/releases/latest` first; every `v*` cmod release is marked prerelease, so that endpoint skipped straight to whichever non-prerelease tag existed in another namespace (the VS Code extension's `vscode-v0.1.0`) instead of the newest cmod build. It now lists `/releases` (newest first, prereleases included) and takes the first `v[0-9]*` tag, skipping `vscode-v*`. `scripts/test-install.sh` reproduces the bug offline against a fixture release list and fails without the fix. The release workflow also sets `make_latest: false` on the VS Code extension tag so it stops competing with cmod tags for GitHub's "latest" slot. (#109)
- **The fmt port builds under Homebrew LLVM 23** — `brew install llvm` now gives clang 23.1.0, under which the cmod-ecosystem/fmt module interface failed two ways: libc++ 23 dropped the transitive include that supplied `std::atomic_flag`, and clang 23 rejects `module :private;` under `#if !FMT_GCC_VERSION`. Cherry-picked the two upstream fmt fixes (fmtlib/fmt `3febdca5`, `758d39eb`) onto the `cmod-support` fork. All 12 `examples.yml` examples pass on clang 23.1.0 with `--no-cache`; the temporary `llvm@22` pin is gone. (#125)

### Security

- **VS Code extension: 0 npm audit findings** — bumped `markdown-it` to 14.3.2, clearing GHSA-253c-mchw-3w2r (quadratic linkify paths, moderate). (#121)
- **VS Code extension: cleared all 16 Dependabot alerts** — `fast-uri`, `undici`, `brace-expansion` (multiple nesting points), `js-yaml`, `browserslist`, `baseline-browser-mapping` and `qs` were pinned below versions already permitted by their parents' declared ranges; `npm update` resolved each in range, no `package.json` changes. `ci.yml` now installs the extension and runs `npm audit --audit-level=moderate` as a gate, so a regressed lockfile fails a build instead of sitting on `main` unnoticed — which is how the alerts this clears got past the alpha.4 release notes' "0 alerts" claim. Supersedes #107 and folds in the #119 Dependabot group bump. (#110)

## [0.1.0-alpha.4] - 2026-07-19

Compiler backends & ecosystem bootstrap. Closes the v0.1.0-alpha.4 milestone (#74): all three major C++ compilers build modules, the module registry is live, and the VS Code extension shipped its first release.

### Added

- **GCC backend (GCC 14+)** — `-fmodules-ts` module builds with module-mapper CMI placement (`.gcm`) and `g++ -fdeps-format=p1689r5` dependency scanning; permanent ubuntu/g++-14 E2E CI job. (#82, closes #76)
- **MSVC backend (VS 2022)** — `/interface /TP` compilation with `/ifcOutput` (`.ifc`) and `/reference` dependencies, `cl /scanDependencies` P1689 scanning, `lib.exe`/`link.exe` linking; toolchain siblings resolved beside `cl.exe` so Git Bash's coreutils `link` can't shadow the linker; permanent windows/VS2022 E2E CI job. Failures outside a VS developer environment carry a vcvars hint. (#86, #67, closes #77, #48)
- **BMI extensions flow through the build plan** — `BuildPlan::from_graph` takes the backend's extension (`.pcm`/`.gcm`/`.ifc`); clang paths byte-identical. (#81, closes #75)
- **The module registry is live** — [`cmod-registry/index`](https://github.com/cmod-registry/index) exists at the long-baked-in default URL, seeded with the nine cmod-ecosystem ports; `cmod search` returns real results online and offline. (closes #78)
- **VS Code extension v0.1.0** — first release: 7 platform VSIXes with bundled cmod binaries + universal VSIX on the [vscode-v0.1.0 release](https://github.com/satishbabariya/cmod/releases/tag/vscode-v0.1.0); marketplace publish pending publisher setup (#52). Release-workflow first-run fixes: committed lockfile, real binaryVersion pin, checksum filenames. (#89, #92, #93)
- **Verified remote-cache restores** — downloads are checked against the entry's metadata hashes; truncated server-side files from interrupted uploads are treated as misses, never used, never stored. Resumable-transfer roadmap in `docs/plan-remote-cache-resilience.md`. (#85, closes #62)

### Fixed

- **`cmod publish` actually publishes** — the registry client only edited its local cache clone and never pushed; publications were silently discarded on the next pull. Now commits and pushes (credential-helper aware). (#88)
- **`cmod add` accepts pasted URLs** — `cmod add https://github.com/owner/repo` double-prepended the scheme and failed resolution; full URLs now normalize to the canonical bare key. Found by the real-world validation sweep. (#84)
- **Windows CI eviction lottery** — 1,270 stale per-branch caches saturated the 10GB Actions quota, evicting the (largest) Windows caches and causing random 8–10 minute legs. Caches now save on `main` only with per-toolchain shared keys; Windows legs run ~2 minutes steady-state. `line-tables-only` debuginfo trims MSVC link cost. (#83, closes #80)
- **Extension npm audit clean** — the 22 alerts surfaced by committing the lockfile resolved to zero: `npm audit fix`, typescript-eslint 6→8, mocha 10→11 with overrides for its vulnerable internal pins; toolchain re-verified (lint/compile/package). (#94)
- **spdlog ecosystem port repaired** — its hand-tuned build config had been lost to a migrate-regenerated manifest, breaking every consumer; restored and re-validated. (cmod-ecosystem/spdlog@eda53d1)
- clippy.toml MSRV aligned to 1.80 (was silently evaluating MSRV-gated lints against 1.74). (#86)

### Docs

- CLAUDE.md refreshed for the alpha.4 state with a Local Gotchas section. (#87)
- Registry phase 1 marked complete; phase 2 (PR-based submissions) tracked in #79. (#88)

### Validation

- Real-world OSS sweep re-run against current main: fmt, json, spdlog, Catch2 — 4/4 green after the #84 and spdlog fixes. (#22)

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
