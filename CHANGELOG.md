# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Fixed

- **Every lockfile cmod writes verifies** — only `cmod resolve` recomputed the lockfile's integrity hash. `cmod remove` and `cmod add` kept the hash of the lockfile they loaded, so `cmod build --verify` rejected the lockfile they had just written ("integrity hash mismatch"). `cmod update` and a build's own resolve dropped the hash, which silently turned the check off. Saving a lockfile now always writes the hash of what it saves.
- **`cmod emit-cmake` writes a CMakeLists.txt that builds** — it listed every source in one target and nothing else, so CMake rejected every module interface ("not found in a FILE_SET of type CXX_MODULES"). Include directories and path dependencies were missing too, and a workspace got a target with no sources. Every package now gets its own target: the package or each workspace member, its path dependencies and its git dependencies checked out at their locked commits (as `cmod build` admits them, under the same signature policy). Module interfaces and partitions go in a `CXX_MODULES` file set, include directories and extra flags carry over, and targets link the targets of the dependencies they declare. It also asks Clang for full BMIs (`-fno-modules-reduced-bmi`, where supported), as `cmod build` makes: the reduced BMIs Clang 23 writes in CMake's one-pass compiles crash it on some imports with libc++. All eleven offline examples and the five real-world consumers CI builds now build and run with CMake 3.28 and Ninja.
- **`cmod migrate cmake` produces a package that builds** — it wrote `[module] root = "src/lib.cppm"`, a file that did not exist, named the module `local.<project>` whatever the sources declared, and ignored `target_sources` (so no module in a `FILE_SET CXX_MODULES` was seen), compile definitions and sources outside `src/`. A library defined beside the executable made the package a static library, and settings of tests and examples were mixed into it. It now picks the target the package is made from (an executable named after the project, else a library), takes the sources, include directories, options and definitions of that target and the targets it links, reads the module name and root from the source that exports it, and sets `[build] sources` and `exclude` when the sources are not all in `src/`. Flags set under `if()` are listed in TODO comments instead of applied, `function()` bodies are not read as if they ran, a package with a module gets at least C++20, and linked targets the file defines are no longer listed as dependencies to map. fmt's own CMakeLists.txt now migrates to a package that builds.
- **`cmod fmt --check`, `cmod lint --deny-warnings` and `cmod check` no longer report "build failed"** — their findings came out as `error: build failed: 2 file(s) need formatting: main.cpp, main.cpp`, though nothing was built. They now read `error: 2 file(s) need formatting: a/src/main.cpp, b/src/main.cpp`, with paths relative to the package or workspace root rather than bare file names. In a workspace, `fmt --check` adds up the members' files directly instead of reading the count back out of each member's message, and a clang-format that fails to run is reported as such. The exit code is still 1.
- **Vendored dependencies are used, and offline builds stay offline** — `cmod vendor` wrote `vendor/github.com_user_repo`, but builds looked for `vendor/github.com/user/repo`, so a vendored dependency was never used: an `--offline` build fetched it into `build/deps/` instead, or failed without a network. Builds now find what `cmod vendor` writes, and `--offline` never fetches (nor deletes a stale checkout): a dependency that is not vendored or checked out at its locked commit is an error.
- **`cmod vendor` writes files that can be committed, and builds check them** — it copied each dependency's git checkout, `.git` included, at whatever commit `build/deps/` had: `git add vendor` then committed an embedded repository rather than the files, and `vendor/config.toml` held absolute paths. It now exports the locked commit's files (keeping executable bits and links), records the commit and each file's SHA-256 in `.cmod-checksum.json`, writes relative paths and a `vendor/.gitignore` for build outputs, and honours `--offline`. A build refuses a vendored package at another commit or with a changed file (exit code 3 for a changed file), and `cmod verify` reports both.
- **`cmod verify` and `build --verify` check dependency checkouts** — `cmod verify` looked for checkouts under the unsanitized package name, never found one, and so never compared a content hash. It now finds them, checks vendored copies against their checksums, and reports local changes to a checkout in `build/deps/`, which `build --verify` also refuses.
- **`cmod graph` shows the graph** — it keyed units by source path but looked up what imports them by module name, so it drew no edges: the library example listed its interface and two partitions side by side, and a `main.cpp` importing anything disappeared. Dependencies were missing, every node was an absolute path, and `--status` and `--timing` looked the build state up under keys the build never writes, so implementation units and other sources always read as never built. The tree now starts from the units nothing imports and shows what each imports beneath it, partitions included (`import :part`), with modules of dependencies named after the dependency; units are named by module or by path relative to the package. `--format dot` draws the edges (dependency modules dashed) and `--format json` keys units by those names, adding `external_imports`.
- **`cmod sbom` writes valid CycloneDX** — hash content kept the lockfile's `sha256:` prefix (CycloneDX takes the hex alone) or was a path dependency's `local`, components had no `bom-ref`, so every entry of `dependencies` referred to nothing, a dependency missing from the lockfile was listed anyway, and the serial number was not an RFC 4122 UUID. The BOM of an fmt consumer and of the path-deps example now validates against the CycloneDX 1.5 schema.
- **Clang builds scan with the compiler's own `clang-scan-deps`** — the scanner was the first `clang-scan-deps` on `PATH`, and Debian and Ubuntu install only versioned ones (`clang-scan-deps-18`), so builds there never scanned: imports were read from the source text, counting those the preprocessor removes. With several LLVMs, the scanner could also be another version than the compiler. It is now looked up beside the compiler, named like it (`clang++-20` → `clang-scan-deps-20`), or beside the binary it links to (`/usr/bin/clang++` → `/usr/lib/llvm-18/bin/clang-scan-deps`), before `PATH`. The first build after upgrading scans again.
- **`cmod toolchain check` checks the toolchain a build uses** — it ran a bare `clang++ --version`, whatever `CXX` said, and reported no version. It now runs the compiler the build would (`CXX` or detection) and reports its path and version, fails if it does not run or does not satisfy `[toolchain] version`, which nothing checked before, warns about compilers that cannot build modules (Apple clang, GCC before 14), and reports the dependency scanner or its absence. `cmod build` warns when the compiler does not satisfy `[toolchain] version`, and `cmod toolchain show` prints the compiler it resolves.
- **`cmod init` sets up git and a package that does something** — it created no `.gitignore`, so `build/` was committed with the sources, and its `main` returned 0 without output, so a first `cmod run` printed nothing. It now creates a git repository (unless the directory is already in one; `--vcs none` for neither) with `/build/` ignored, and a module exporting a greeting that `main` prints and the test checks, formatted as the `.clang-format` it writes (so `cmod fmt --check` passes). A name that cannot be a C++ identifier (`std`, a keyword, a leading digit, `.`) is refused instead of generating code that does not compile.
- **Workspace members a glob matches build with their path dependencies** — with `members = ["crates/*"]`, members were named by their directory (`crates/a`), while path dependencies were matched to members by their key (`a`). So none matched: `b` could build before `a`, without the modules it imports, and libraries were named `libcrates_a.a`. Members are now named by their `[package] name`, and a path dependency is matched to the member whose directory it points at, whatever its key. Two members sharing a name, or a name that cannot name a directory (`../x`), are an error. `exclude` now covers the directories inside an excluded one. `cmod workspace remove` takes a name or a directory and excludes a member a glob matches, which it used to report as removed while leaving it in place. `cmod workspace add libs/util` names the scaffolded package `util`, and refuses a directory name that cannot name its module (`2d`, `export`).
- **`cmod init` inside a workspace joins it** — `cmod init --workspace` advised creating members with `cmod init` in subdirectories, but those packages were not added to `[workspace] members`, so the workspace built none of them. A package created in a workspace is now added to its members, unless a member pattern already matches it or `exclude` covers it (which it reports); a name another member has is refused before anything is written. `cmod workspace add` and `remove`, and with them `cmod init`, now edit only `[workspace] members` and `exclude`, keeping the root `cmod.toml` as written: they used to rewrite the whole file, dropping its comments and adding every empty table.

## [0.1.0-alpha.8] - 2026-10-02

GCC and the tools around the build catch up with the build itself. GCC 14+ packages now find their imports the way the compiler does, build completely the first time (the second build of an fmt consumer went from 6 s to 0.25 s), and link as shared libraries. `cmod test` compiles only the tests that changed, in parallel. `clang-scan-deps` finally runs, with its results cached. `cmod compile-commands` entries compile, in workspaces too. `cmod tidy --apply` no longer deletes dependencies that are in use, and a failed build reports the module that failed, or the real import cycle, instead of a cascade of errors.

The first build after upgrading scans every source again, because scan results are now keyed on the scanner's full command line. Shared libraries, and the dependencies built for them, recompile once with `-fPIC`. Manifest, lockfile and cache formats are unchanged.

### Changed

- **`cmod test` compiles only the tests that changed, in parallel** — every run compiled and linked every test binary again, one after another, even when nothing had changed: with a test framework like Catch2, each run paid for every test's full compile. A test is now compiled again only when its source, a header it includes, the package's BMIs and objects, or the compile command changed (recorded in `.cmod-test-state.json`), and the compiles run `--jobs` at a time. `cmod -v test` prints `Fresh` for the tests it skipped.

### Fixed

- **`cmod compile-commands` works in a workspace** — at a workspace root it warned "no source files" and wrote nothing, so editors had no compilation database for any member. It now writes one database with every member's entries, each with the member's flags and include directories and the BMIs of the members it depends on; it shares that setup with `cmod build` (`member_build`).
- **`cmod compile-commands` entries compile** — the database is what clangd and other IDE tools build from, and for many packages its entries did not compile: a partition re-exported by an interface (`export import :part;`) was never resolved, path dependencies' BMIs were never passed, and a TU importing a module got only that module's BMI, not those it re-exports. It also had its own copy of the compiler setup, which missed the `include/` directory, `[build] optimization` and LTO. It now uses the build's graph and compiler setup, passes path and git dependencies' BMIs and include directories, and every interface a TU depends on transitively.
- **`clang-scan-deps` is actually used, and its results are cached** — every scan ran `clang-scan-deps -- <source> -std=c++20`, which takes the source for the compiler, finds no input and crashes. cmod then fell back to reading `import` lines from the source text, so the scanner never ran successfully and still cost every build one crashing process per source (1.2 s of a 1.7 s no-op build of a Catch2 consumer). It now runs with the compiler and the package's compile flags, so an `import` inside `#if 0` no longer counts. Each source's result is cached with the headers the scan read (`.cmod-scan-state.json`): the same no-op build takes 0.6 s.
- **A GCC build is complete the first time** — g++ lists the module mapper cmod writes just before each compile among the files it read. Linux records file times from a coarse clock, so the mapper's mtime usually fell in the millisecond the compile started, and cmod discarded the header list as possibly read mid-change. Most GCC units were therefore neither recorded nor cached on their first compile. The next build compiled them all again: 6 s instead of 0.25 s for an fmt consumer. The mapper and the CMIs g++ lists are no longer counted as headers; both were already inputs.
- **GCC builds see imports the way the compiler does** — GCC packages' imports were read from the source text, so an `import` the preprocessor removes still counted: one inside `#if 0`, or behind a macro from a header, could add a false cycle (`circular dependency detected`) or a wrong build order. GCC 14+ packages are now scanned by `g++ -fdeps-format=p1689r5` with the package's flags, and the results are cached with the headers each scan read, as for Clang. The backends' unused `scan_deps` gave way to `CompilerBackend::scan_command`, which `cmod build` and `cmod compile-commands` share.
- **`cmod tidy --apply` no longer deletes dependencies in use** — tidy only looked for `import`s of a module named after the dependency. A dependency used through its headers (`#include <fmt/format.h>`, as every header-library port is) or through a module its interface declares under another name (`import nlohmann.json;`) was reported unused, and `--apply` removed it from `cmod.toml`, breaking the build: it did so for all five real-world consumers CI builds. Tidy now also matches the modules a dependency's interfaces declare and the headers in its include directories. A git dependency that is not checked out is reported as unchecked and kept.
- **No more "argument unused during compilation" warnings from Clang** — compiling an interface's PCM to an object does not preprocess, so every include path on its command line went unused, and Clang warned about each one for every interface of a package with an `include/` directory or dependencies.
- **Shared libraries link with GCC** — nothing was compiled as position-independent code, and GCC as distributions build it emits PIE objects by default, which cannot go into a shared object: a `shared-lib` package whose code referred to data such as `std::cout` failed to link with "recompile with -fPIC", including the `shared-lib` example. Shared libraries, and the dependencies linked into them, are now compiled with `-fPIC` on Clang and GCC (not for MSVC or Windows targets).
- **A module that fails to compile stops the build there** — in a parallel build, its importers were still compiled. An idle worker picked each one up before noticing the failure, and each failed again with "module not found", burying the real error under one bogus error per importer. Importers of a failed node are no longer scheduled, and no new work starts once a node has failed, as with `--jobs 1`.
- **A circular-import error names the cycle** — it listed every module that could not be ordered, joined by arrows, including modules that only import the cycle: two modules importing each other were reported as `local.demo -> local.extra -> main`. It now prints the cycle itself (`local.demo -> local.extra -> local.demo`); a workspace path-dependency cycle names its members instead of only saying there is one. The hint no longer suggests `cmod deps --tree`, which shows neither.
- **Tests with the same file name in different directories run separately** — `tests/a/check.cpp` and `tests/b/check.cpp` were both built to `test_check`, so both results came from whichever compiled last. They now get distinct binaries and are reported by relative path.
- **A no-op workspace build no longer relinks** — a member's upstream members' objects were collected in hash-set order, which changes from run to run, so the link inputs looked different every build: the member was relinked each time, and its binary was not reproducible. Members, and the archives and objects collected from every dependency's build directory, are now sorted.
- **`cmod build --dry-run` no longer reports path dependencies as unfetched** — with a lockfile, every path dependency was reported as "not checked out at the locked commit; a build would fetch it", although a build never fetches them.

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
