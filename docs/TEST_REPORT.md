# cmod — Extensive Testing Report

> **Date:** 2026-04-21
> **Version:** cmod 0.1.0-alpha.1
> **Platform:** macOS (darwin/arm64)
> **Toolchains used:**
> - Homebrew `llvm@18` (Clang **18.1.8**) — for manual CLI testing
> - Homebrew `llvm` / `llvm@20` / `llvm@22` → all point at **Clang 22.1.1** — used by integration tests (hard-coded to `/opt/homebrew/opt/llvm/bin`)
> **Binary:** `target/release/cmod` built from `main`

## TL;DR

| Metric | Result |
|---|---|
| Rust unit + integration test suites | **828 / 828 passing** across 12 crates |
| Examples built cleanly end-to-end | **12 / 13** (plugin has no sources — manifest-only) |
| CLI commands exercised | **33 commands, ~110 invocations** |
| Error-path scenarios verified | **14** (missing manifest, bad TOML, stale lockfile, offline, unknown cmd/flag, …) |
| Bugs / UX issues found | **7** — 3 high, 4 low/medium (detailed below) |

---

## 1. Test matrix

### 1.1 Rust test suites (`cargo test --release --workspace`)

After wiping `~/Library/Caches/cmod` and cleaning all `examples/*/build/` dirs (see bug #7 below for why this matters):

```
cmod-cache              110 passed
cmod-core                49 passed
cmod-resolver           162 passed
cmod-build               87 passed
cmod-cli (unit)          90 passed
cmod-cli example_projects 9 passed
cmod-workspace           35 passed
cmod-security            99 passed
cmod-lsp                 45 passed
cmod-plugin              69 passed
integration / CLI        61 passed
doc tests                12 passed
TOTAL                   828 passed — 0 failed
```

### 1.2 Example projects (live `cmod` invocations)

| Example | `cmod check` | cold `cmod build` | cached `cmod build` | `cmod build --release` | `cmod run` | `cmod test` |
|---|---|---|---|---|---|---|
| `hello` | ✅ | ✅ 1.6s | ✅ up-to-date 0.1s | ✅ | ✅ prints greeting | ⚠ no tests declared |
| `library` | ✅ | ✅ 0.9s (3 partitions) | ✅ | ✅ | n/a (lib) | ✅ 1 passed |
| `with-deps` | ⚠ warns undeclared `nlohmann.json` | ✅ fetches fmt + json, 1.4s | ✅ | ✅ | ✅ prints JSON | — |
| `nested-deps` | ✅ | ✅ 1.3s (path dep + own src) | ✅ | ✅ | ✅ | — |
| `with-tests` | ✅ | ✅ 0.7s | ✅ | ✅ | — | ⚠ "No tests found" |
| `header-only` | ✅ | ✅ 0.9s | ✅ | ✅ | ✅ | — |
| `include-dirs` | ✅ | ✅ 1.6s | ✅ | ✅ | — | — |
| `ixx-modules` | ✅ | ✅ 1.7s | ✅ | ✅ | ✅ | — |
| `shared-lib` | ✅ | ✅ 1.9s → `libshared-lib.dylib` | ✅ | ✅ | n/a | — |
| `path-deps` | ✅ | ✅ 2 path libs + main | ✅ | ⚠ release links debug-built path deps | ✅ | — |
| `workspace` | ✅ | ✅ 3 members | ✅ | ✅ | n/a | — |
| `multi-binary` | ✅ | ✅ 3 members (dice, stats, roller) | ✅ | ✅ | n/a | — |
| `plugin` | ✅ | ❌ `no source files found` (manifest-only demo) | — | — | — | — |

### 1.3 CLI surface coverage

All commands executed at least once; flags exercised for the common variants.

- **Lifecycle**: `init`, `init --workspace`, `status`, `check`, `clean`, `build`, `build --release`, `build --timings`, `build --jobs N`, `build --force`, `run`, `run --release`, `test`
- **Deps**: `add`, `add --branch`, `add --path`, `remove`, `resolve`, `--locked resolve`, `--offline resolve`, `update`, `update --patch`, `deps`, `deps --tree`, `deps --why`, `tidy`, `vendor`, `vendor --sync`
- **Graph**: `graph`, `graph --format dot`, `graph --format json`, `graph --status`, `graph --critical-path`, `graph --timing`, `graph --filter`
- **Plan**: `plan`, `compile-commands`, `emit-cmake`, `explain <mod>`
- **Quality**: `lint`, `lint --deny-warnings`, `fmt --check`
- **Cache**: `cache status`, `cache status-json`, `cache gc`, `cache clean`, `cache inspect`, `cache export`, `cache import`, `cache push`, `cache pull`
- **Security**: `audit`, `verify`, `verify --signatures`, `sbom`, `sbom --output`
- **Workspace**: `workspace list`, `workspace add`, `workspace remove`
- **Toolchain**: `toolchain show`, `toolchain check`
- **Plugin**: `plugin list`, `plugin run`
- **Misc**: `search`, `search --local-only`, `migrate cmake`, `lsp`

---

## 2. Bugs and issues discovered

### BUG-01 (high) — `cmod vendor` rejects all Git-URL-style deps as path traversal
- **Repro:** any project with a dep named `github.com/<owner>/<repo>`
  ```
  $ cmod vendor
  error: security violation: unsafe package name in lockfile:
         'github.com/cmod-ecosystem/fmt' contains path traversal or invalid characters
  ```
- **Root cause:** the path-safety check rejects any component containing `/`. Since cmod's own convention is to use Git URL paths as dep names (e.g. `github.com/fmtlib/fmt`), **no real-world `vendor` invocation can succeed**. Exit code `3` (security violation).
- **Impact:** `--offline` / air-gapped workflows are blocked entirely. Vendor directory is never created.
- **Expected:** `vendor` should derive a safe on-disk folder name from the dep (mirroring how `build/deps/github.com_cmod-ecosystem_fmt/` is already done on disk) rather than rejecting the dep.

### BUG-02 (high) — `cmod verify --signatures` looks up git repos at the wrong path
- **Repro:** `cd examples/with-deps && cmod verify --signatures`
- **Error:**
  ```
  error: package 'github.com/cmod-ecosystem/fmt' has invalid signature:
         failed to open repo at build/deps/github.com/cmod-ecosystem/fmt:
         No such file or directory
  ```
- **Actual on-disk path:** `build/deps/github.com_cmod-ecosystem_fmt/` (slashes replaced with underscores).
- **Root cause:** signature verification path-joins raw `/` separators from the package name, but the resolver writes deps under a sanitized underscore-encoded directory.
- **Impact:** `verify --signatures` always fails for any Git-URL dep.

### BUG-03 (high) — `cmod workspace add <dir>` refuses pre-existing directories
- **Repro:**
  ```
  $ mkdir -p member_a/src && echo '…manifest…' > member_a/cmod.toml
  $ cmod workspace add member_a
  error: invalid manifest: directory 'member_a' already exists
  ```
- **Inconsistency:**
  - `cmod workspace add nonexistent` **succeeds** and silently adds a member pointing at a non-existent directory (never scaffolds anything).
  - Real-world usage — "bring this existing crate into the workspace" — is blocked.
- **Expected:** `workspace add <dir>` should add the existing directory to `[workspace].members` if it already contains a `cmod.toml`. Scaffolding-on-add should be a separate opt-in flag.

### BUG-04 (medium) — `cmod run --release` looks for the binary in `build/debug`
- **Repro:** `cd examples/hello && cmod build --release && cmod run --release`
- **Error:** `error: build failed: no binary found in build/debug (expected 'hello'). Is the project configured as an executable?`
- **Actual output** of `build --release`: binary lives at `build/release/hello`.
- **Impact:** `cmod run --release` never works for executables. Users must invoke the binary manually after `cmod build --release`.

### BUG-05 (medium) — `path`-dep projects link the debug artefacts into release builds
- **Repro:** `cd examples/path-deps && cmod build --release`
- **Output:**
  ```
  Building path-deps (release)
    Linked /…/path-deps/libs/colors/build/debug/libcolors.a   ← debug!
    Linked /…/path-deps/libs/geometry/build/debug/libgeometry.a
    Linked /…/path-deps/build/release/path-deps
  ```
- **Impact:** a release build of the top-level target links against **debug** builds of its path dependencies. Mixed-mode ABI risks + larger binaries than expected.
- **Expected:** path deps should be rebuilt under the active profile, or at least a warning should be emitted.

### BUG-06 (low) — `cache push / cache pull` help omits the remote override
- **Repro:** `cmod cache push --remote http://127.0.0.1:9999`
- **Error:** `error: unexpected argument '--remote' found`
- Running without args is fine: `error: no shared cache URL configured; add [cache] shared_url to cmod.toml`.
- **Gap:** users can set the remote for `cmod build` via `--remote-cache <URL>`, but there is no CLI-only way to push/pull to an ad-hoc remote — you must edit `cmod.toml`. Either expose a `--remote` flag or clarify the workflow in help text.

### BUG-07 (low, test harness) — integration tests copy `examples/*/build/` into tempdirs
- **Repro:** `cargo test --release --test example_projects` after having manually run any `cmod build` in `examples/hello` (or similar) with a clang version different from `/opt/homebrew/opt/llvm/bin/clang++` (i.e. clang 22.1.1).
- **Symptom:** 4 tests fail with:
  ```
  module file '…/build/debug/pcm/local_hello.pcm' uses an older format that is no longer supported
  ```
- **Root cause:** `copy_dir_recursive` in `crates/cmod-cli/tests/example_projects.rs:43` copies everything under each example, **including** the `build/` directory. If that directory contains PCMs produced by a different clang version, the subsequent test build (which uses llvm-22) tries to import them and fails.
- **Fix:** exclude `build/`, `.cache/`, and generated files (`compile_commands.json`, `CMakeLists.txt`) during the copy.
- **Workaround:** `find examples -type d -name build -prune -exec rm -rf {} +` before running the suite — then **all 828 tests pass cleanly**.

### Additional observations (not bugs, but worth noting for the site docs)

- **Branch-ref warnings:** `cmod audit` correctly flags `branch = "…"` as "consider pinning to a tag or commit". Works as designed.
- **Undeclared-import warning:** `cmod check` in `examples/with-deps` correctly warns `import 'nlohmann.json' does not match any declared dependency` — useful signal. The manifest declares `github.com/cmod-ecosystem/json` but the code imports `nlohmann.json`; this is a real mismatch that the example might want to reconcile.
- **Cache key does not include compiler version.** Inspecting a cached entry:
  ```json
  "compiler": "clang",
  "compiler_version": "",
  "target": "",
  "created_at": ""
  ```
  Because these fields are empty, cache hits are compiler-version-agnostic. Usually that's fine for object files but PCMs are not forward/back compatible across clang major versions — a user who upgrades Clang and runs `cmod build` without `cmod cache clean` will see "older format that is no longer supported". This is exactly what tripped the integration tests. Recommendation: populate `compiler_version` from `clang --version` and include it in the cache key.
- **Workspace `cmod graph`** run at the root says `no source files found`. Graph is per-member, not per-workspace — this is expected but the error message could be friendlier (e.g. "workspace root has no sources; cd into a member or run `cmod graph -p <member>`").
- **`cmod test` on a project with no `tests/*.cpp`** emits `warning: No tests found, skipping` — but `hello`'s template `cmod init` writes `tests/main.cpp` so the warning still appears when that file exists but has no actual test assertions. Consider relaxing the glob or documenting the expected layout.
- **Exit codes look consistent** with the documented spec (`0/1/2/3`): missing manifest → 1, unknown flag → 2, stale `--locked` → 2, vendor path traversal → 3.
- **`migrate cmake`** correctly extracts project name, version, and C++ standard. Emitted manifest points `module.root` at `src/main.cppm` even though the input source is `.cpp` — this is intentional (cmod is modules-first), but the accompanying `note:` ("Add C++20 module declarations to your source files") is the only indicator. Consider renaming the source automatically, or keeping `.cpp` + generating a `lib.cppm` that exports it.

---

## 3. Verified working end-to-end

The following scenarios were exercised and all produced the expected artefacts:

1. **Fresh init → build → run** (`cmod init` → `cmod build` → `cmod run` → `Hello, world!`)
2. **Git-dep fetch & compile** (`examples/with-deps`: fetches `fmt` + `nlohmann.json`, links both, produces JSON output)
3. **Module partitions** (`examples/library`: interface unit + 2 partition units → static library)
4. **Path dependencies** (`examples/path-deps`: chained path deps linked into binary)
5. **Nested deps** (`examples/nested-deps`: path dep with its own transitive dep)
6. **Workspace** (`examples/workspace`: 3 members, unified resolve, per-member build)
7. **Multi-binary workspace** (`examples/multi-binary`: 3 members, each its own binary)
8. **Shared library** (`examples/shared-lib`: builds `.dylib`)
9. **`.ixx` module files** (MSVC-style extension recognized on clang)
10. **Header-only library interop** (`examples/header-only`)
11. **Content-addressed cache hit** (second build of any example: `up-to-date, 0.1s`)
12. **Cache export → import round-trip** (`cmod cache export <mod> <key>` → `cmod cache import <dir>`)
13. **SBOM generation** (CycloneDX 1.5 JSON with purl, SHA-256, VCS refs)
14. **CMake emission** (`cmod emit-cmake` generates valid `CMakeLists.txt`)
15. **Migrate from CMake** (`cmod migrate cmake` converts minimal `CMakeLists.txt` → `cmod.toml`)
16. **`cmod lsp`** starts, reads Content-Length-framed JSON-RPC (exits cleanly on malformed frames)
17. **`cmod graph --format dot`** → pipe to `dot -Tsvg` for SVG
18. **`cmod plan`** → stable JSON for CI diffing
19. **Tidy** (`cmod tidy` correctly flags unused `nlohmann/json` dep in `with-deps`)
20. **Audit** (`cmod audit` flags branch-ref deps)
21. **Error paths** (missing manifest, bad TOML, unknown subcommand, stale `--locked` lockfile, offline + no cache — all produce clean, actionable errors)

---

## 4. Recommended next steps

Ordered by impact:

1. **Fix BUG-01 (`cmod vendor`)** — blocks offline workflows entirely; the fix is localized to the path-sanitization check in `cmod-resolver` / vendor writer.
2. **Fix BUG-02 (`verify --signatures`)** — reuse the resolver's path-sanitization helper when opening the repo.
3. **Fix BUG-03 (`workspace add` semantics)** — either allow existing dirs or introduce `workspace add --scaffold`.
4. **Fix BUG-04 (`run --release`)** — one-line fix in `cmod-cli::commands::run.rs` to propagate the profile.
5. **Fix BUG-05 (path-dep profile inheritance)** — path deps should follow the active profile.
6. **Include `compiler_version` in cache key** (prevents the "older PCM format" surprise after Clang upgrades; also fixes BUG-07's root cause).
7. **Fix BUG-07 (test harness)** — exclude `build/` during recursive copy so the integration suite is robust against developer machines.
8. **Expose `cache push --remote` / `cache pull --remote`** or document the manifest-only workflow.

---

## 5. Raw captures

All raw outputs are in `/tmp/cmod-test-outputs/`:

```
examples-matrix.txt   — 13 examples × {status, check, clean, cold build, cached build, release build} + test + run
dep-lifecycle.txt     — resolve / add / remove / --locked / --offline / update / vendor
cache.txt             — cache status / status-json / gc / inspect / export / import / push / pull
workspace.txt         — init --workspace + add / remove / build / error paths
graph.txt             — graph across 5 examples × {ascii, dot, json, --status, --critical-path, --timing, --filter}
quality.txt           — lint / fmt --check / check / audit / verify / verify --signatures / sbom / emit-cmake
errors.txt            — 14 error-path scenarios
misc.txt              — migrate cmake / lsp / plugin list / compile-commands
cargo-test.txt        — cargo test --release --workspace summary (828 passed)
with-deps.sbom.json   — sample SBOM for website/docs
bmi-pkgs/pkg.bmi/     — exported BMI package for the `fmt` module
```

To reproduce:

```bash
export PATH=/opt/homebrew/opt/llvm@18/bin:$PATH   # or /opt/homebrew/opt/llvm/bin for llvm 22
cargo build --release
# clean state
rm -rf ~/Library/Caches/cmod
find examples -type d -name build -prune -exec rm -rf {} +
# unit + integration suite
cargo test --release --workspace
```

---

_Report generated 2026-04-21 by the cmod maintainers._
