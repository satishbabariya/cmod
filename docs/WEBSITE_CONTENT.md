# cmod Website Content Bundle — v0.1.0-alpha.2

> **For:** Website & docs team
> **Source of truth:** `cmod 0.1.0-alpha.2` (tag pushed 2026-04-21)
> **Capture toolchain:** Homebrew `llvm@18` (Clang 18.1.8) on darwin/arm64
> Every code block, help snippet, and sample output below is verbatim output from the shipping binary. Copy directly; do not paraphrase error strings or flag names.

---

## 1. Product summary

**cmod** — Cargo-inspired, Git-native package & build tool for modern C++20 modules.

> _C++ has modules now. It deserves a build tool that knows it._

**Three differentiators:**
| Headline | Sub-headline |
|---|---|
| **Modules, not headers** | C++20 interface units, partitions, and BMIs are first-class — cmod builds the module DAG, not a translation-unit soup. |
| **Git is the registry** | No central server, no accounts. `cmod add github.com/fmtlib/fmt@^10.2` pins a Git URL, tag, or commit — reproducible by default. |
| **Deterministic & fast** | Mandatory lockfiles, SHA-256 content-addressed caching keyed on compiler version, incremental builds, remote cache push/pull. |

## 2. What's new in v0.1.0-alpha.2

Audit-cleanup release — no new user-facing features of scale, but two substantial behaviour improvements worth a callout:

- **Reliable caching across Clang upgrades.** Cache keys now include the compiler version, so a PCM built by Clang 18 will never be handed to Clang 22 (and rejected as "older format"). One-time cache miss on first build after upgrade — no user action.
- **Release builds are now actually release.** `cmod build --release` now rebuilds path dependencies under the release profile (previously they were linked from `build/debug/`).

New flag: `cmod cache push/pull --remote <URL>` lets you override the manifest's `[cache].shared_url` per invocation.

Seven audit bugs fixed — see `RELEASE.md` or the [full CHANGELOG entry](https://github.com/satishbabariya/cmod/blob/main/CHANGELOG.md#010-alpha2---2026-04-21).

## 3. Installation

```bash
# Latest
curl -sSf https://raw.githubusercontent.com/satishbabariya/cmod/main/install.sh | sh

# Pin to v0.1.0-alpha.2
curl -sSf https://raw.githubusercontent.com/satishbabariya/cmod/main/install.sh | sh -s -- --version v0.1.0-alpha.2
```

### From source

```bash
git clone https://github.com/satishbabariya/cmod.git
cd cmod && git checkout v0.1.0-alpha.2
cargo install --path crates/cmod-cli
```

### Platform matrix

| Platform | Architecture | Archive |
|---|---|---|
| Linux | x86_64 (glibc) | `cmod-v0.1.0-alpha.2-x86_64-unknown-linux-gnu.tar.gz` |
| Linux | aarch64 (glibc) | `cmod-v0.1.0-alpha.2-aarch64-unknown-linux-gnu.tar.gz` |
| Linux | x86_64 (musl, static) | `cmod-v0.1.0-alpha.2-x86_64-unknown-linux-musl.tar.gz` |
| macOS | x86_64 (Intel) | `cmod-v0.1.0-alpha.2-x86_64-apple-darwin.tar.gz` |
| macOS | aarch64 (Apple Silicon) | `cmod-v0.1.0-alpha.2-aarch64-apple-darwin.tar.gz` |
| Windows | x86_64 | `cmod-v0.1.0-alpha.2-x86_64-pc-windows-msvc.zip` |
| Windows | aarch64 | `cmod-v0.1.0-alpha.2-aarch64-pc-windows-msvc.zip` |

### Runtime requirements

- **LLVM/Clang 17+** on `PATH`. On macOS use Homebrew `llvm@18`; Apple's `/usr/bin/clang++` mis-handles the C++20 global module fragment.
- **Git 2.25+** for fetching dependencies.

Verify:
```text
$ cmod --version
cmod 0.1.0-alpha.2
```

## 4. 60-second quickstart

```bash
# 1. Scaffold a new module
mkdir hello && cd hello
cmod init

# 2. Build and run
cmod run

# 3. Add a Git dependency
cmod add github.com/cmod-ecosystem/fmt@^0.1

# 4. Resolve + build
cmod resolve
cmod build

# 5. Inspect
cmod status
cmod deps --tree
cmod graph --format dot | dot -Tsvg > graph.svg
```

### `cmod init` output

```text
$ cmod init
     Created module 'hello' in /path/to/hello
```

**Scaffolded tree:**
```
hello/
├── .clang-format
├── cmod.toml
├── src/
│   ├── lib.cppm          # export module local.hello;
│   └── main.cpp          # import local.hello;
└── tests/
    └── main.cpp
```

### Generated `cmod.toml`

```toml
[package]
name = "hello"
version = "0.1.0"
edition = "2023"
authors = []

[module]
name = "local.hello"
root = "src/lib.cppm"

[dependencies]
[dev-dependencies]
[build-dependencies]
[features]

[compat]
cpp = ">=20"
platforms = []

[toolchain]
compiler = "clang"
cxx_standard = "20"

[build]
type = "binary"
optimization = "debug"
lto = false
parallel = true
incremental = true
```

## 5. CLI reference (verbatim help)

> Full help blocks are the source of truth — render these directly on the site.

### Top-level

```text
$ cmod --help
Cargo-inspired, Git-native package and build tool for C++20 modules

Usage: cmod [OPTIONS] <COMMAND>

Commands:
  init              Initialize a new module or workspace
  add               Add a dependency
  remove            Remove a dependency
  resolve           Resolve dependencies and generate/update the lockfile
  build             Build the current module or workspace
  test              Run module tests
  update            Update dependencies
  deps              Inspect the dependency graph
  cache             Manage the build cache
  verify            Verify integrity and security
  graph             Visualize the module dependency graph
  audit             Audit dependencies for security and quality issues
  status            Show project status overview
  explain           Explain why a module would be rebuilt
  toolchain         Manage the active toolchain
  vendor            Vendor dependencies for offline builds
  lint              Lint C++ source files for common issues
  fmt               Format C++ source files using clang-format
  search            Search for modules by name
  run               Build and run the project binary
  clean             Remove build artifacts
  workspace         Manage workspace members
  sbom              Generate a Software Bill of Materials (SBOM)
  publish           Publish a release (create a Git tag)
  compile-commands  Generate compile_commands.json for IDE integration
  tidy              Remove unused dependencies from cmod.toml
  check             Validate module naming, identity, and structure rules
  plugin            Manage plugins
  plan              Output the build plan as JSON without executing it
  emit-cmake        Export a CMakeLists.txt for CMake-based projects
  migrate           Migrate from another build system to cmod
  lsp               Start the LSP server for IDE integration
```

### Global flags (every subcommand)

| Flag | Effect |
|---|---|
| `--locked` | Use the lockfile strictly; fail if outdated |
| `--offline` | Disable all network access |
| `-v, --verbose` | Verbose output |
| `-q, --quiet` | Suppress status output |
| `--target <TRIPLE>` | Override target triple |
| `--features <CSV>` | Enable features (comma-separated) |
| `--no-default-features` | Disable default features |
| `--no-cache` | Skip build cache |
| `--untrusted` | Skip TOFU verification |

### New in alpha.2 — `cache push/pull --remote`

```text
$ cmod cache push --help
Push local cache entries to remote cache

Options:
      --remote <URL>   Remote cache URL (overrides manifest [cache].shared_url)
      ...
```

Takes precedence over `[cache].shared_url` in `cmod.toml`. Falls back to the manifest when the flag is absent; error message mentions both paths when neither is set.

## 6. Sample outputs

### `cmod build` — cold cache then cached

```text
$ cmod build                     # first invocation
    Building hello (debug)
    Compiled interface: /…/src/lib.cppm
    Compiled /…/src/main.cpp
      Linked /…/build/debug/hello
     Summary 2 modules (2 compiled), 1.6s

$ cmod build                     # second invocation — content-addressed cache hit
    Building hello (debug)
      Linked /…/build/debug/hello
     Summary 2 modules (2 up-to-date), 0.1s
    Finished /…/build/debug/hello
```

### Module partitions (`examples/library`)

```text
$ cmod graph
math-lib
├── /…/src/lib.cppm   (InterfaceUnit)
├── /…/src/ops.cppm   (PartitionUnit)
└── /…/src/stats.cppm (PartitionUnit)

$ cmod build
    Building math-lib (debug)
    Compiled interface: /…/src/ops.cppm
    Compiled interface: /…/src/stats.cppm
    Compiled interface: /…/src/lib.cppm
      Linked /…/build/debug/libmath-lib.a
     Summary 3 modules (3 compiled), 0.9s
```

### Git dependencies (`examples/with-deps`)

```text
$ cmod build
    Building with-deps (debug)
    Fetching github.com/cmod-ecosystem/fmt (7bf8390a)
    Compiled /…/deps/fmt/src/fmt-c.cc
    Compiled /…/deps/fmt/src/format.cc
    Compiled /…/deps/fmt/src/os.cc
    Compiled interface: /…/deps/fmt/src/fmt.cc
      Linked /…/deps/fmt/build/debug/libfmt.a
    Fetching github.com/cmod-ecosystem/json (3761884f)
    Compiled interface: /…/deps/json/src/modules/json.cppm
      Linked /…/deps/json/build/debug/libnlohmann_json.a
    Compiled interface: /…/src/lib.cppm
      Linked /…/build/debug/with-deps
     Summary 2 modules (2 compiled), 1.4s
```

### Workspace (`examples/workspace`)

```text
$ cmod workspace list
   Workspace workspace-example
     Members 3 member(s)
             app
             core
             utils

$ cmod build
    Building workspace (3 members, debug)
   Compiling core
      Linked /…/build/debug/core/libcore.a
   Compiling utils
      Linked /…/build/debug/utils/libutils.a
   Compiling app
      Linked /…/build/debug/app/app
    Finished workspace build complete
```

### Dependency inspection

```text
$ cmod deps --tree
with-deps v0.1.0
├── github.com/cmod-ecosystem/fmt  v0.0.0-20260312-7bf8390a
└── github.com/cmod-ecosystem/json v0.0.0-20260312-3761884f
```

### Cache status (JSON)

```text
$ cmod cache status-json | jq '.entries[0]'
{
  "module": "fmt",
  "key": "27576c523042b3ff4357b8a8be51c0bd1f99fdc65753750f859686901aec54bf",
  "size": 24125135
}
```

### SBOM (CycloneDX 1.5)

```json
{
  "bomFormat": "CycloneDX",
  "specVersion": "1.5",
  "metadata": {
    "timestamp": "2026-04-21T12:37:12Z",
    "tools": [{"vendor": "cmod", "name": "cmod", "version": "0.1.0-alpha.2"}],
    "component": {"type": "application", "name": "with-deps", "version": "0.1.0"}
  },
  "components": [
    {
      "type": "library",
      "name": "github.com/cmod-ecosystem/fmt",
      "version": "0.0.0-20260312-7bf8390a",
      "purl": "pkg:cmod/github.com/cmod-ecosystem/fmt@0.0.0-20260312-7bf8390a",
      "hashes": [{"alg": "SHA-256", "content": "sha256:9ca72aca…"}]
    }
  ]
}
```

## 7. IDE / tooling integration

- **clangd / VS Code / Neovim**: `cmod compile-commands` writes `compile_commands.json`.
- **LSP**: `cmod lsp` starts the Language Server (documentSymbol, references, codeAction + custom `cmod/buildStatus`, `cmod/dependencies`, `cmod/criticalPath`, `cmod/cacheStatus`).
- **CMake interop**: `cmod emit-cmake`.
- **Graph SVG**: `cmod graph --format dot | dot -Tsvg`.
- **CI-stable build plan**: `cmod plan` — deterministic JSON, diff in CI to detect unexpected DAG changes.

## 8. Error surface (real copy to reuse on docs pages)

```text
$ cmod deps                       # no lockfile
error: lockfile not found; run `cmod resolve` first
note: run `cmod resolve` to generate the lockfile

$ cmod --locked resolve           # lock outdated
   Resolving dependencies...
error: lockfile is outdated; run `cmod resolve` to update

$ cmod --offline resolve          # nothing cached
   Resolving dependencies...
error: git operation failed: cannot fetch 'X' in offline mode; no cached version available

$ cmod cache push                 # no remote configured
error: no shared cache URL configured; add [cache] shared_url to cmod.toml or pass --remote <URL>
```

**Convention:** `error:` prefix (red), `note:` prefix (blue), `warning:` prefix (yellow). Lean on these for docs-page callouts and colouring.

## 9. Exit codes

| Code | Meaning |
|---|---|
| `0` | Success |
| `1` | Build failure |
| `2` | Resolution error |
| `3` | Security violation |

## 10. Pages to build on the website

1. **Landing** — hero (tagline + 3 differentiators), install block, CTA to GitHub + alpha.2 release.
2. **Docs → Getting Started** — §4 (quickstart), §6 (sample outputs).
3. **Docs → CLI reference** — §5 (generated from `--help`). Include the alpha.2 `--remote` flag callout.
4. **Docs → Configuration** — `cmod.toml` schema (see `docs/rfc/rfc_unified_cmod_schema.md`).
5. **Docs → Examples** — one page per project in `examples/`.
6. **Docs → Troubleshooting** — `docs/TEST_REPORT.md` §12 + the error-surface copy above.
7. **Release** — render `RELEASE.md` verbatim. Upgrade notes prominent at the top.
8. **Security** — TOFU + SBOM + audit (§5 of the old bundle's deep-dive).
9. **Blog** — v0.1.0-alpha.2 release post template in §11 below.

## 11. Announcement templates

### Tweet / toot (280 chars)

> cmod v0.1.0-alpha.2 — Cargo-style build tool for C++20 modules.
> 7 audit bugs fixed, cache now keyed on compiler version (no more "older PCM format" after Clang upgrades), `cmod build --release` now actually releases path deps. Full notes ↓
> https://github.com/satishbabariya/cmod/releases/tag/v0.1.0-alpha.2

### Blog post / long-form

```markdown
# cmod v0.1.0-alpha.2 — audit cleanup + cache hardening

It's been a month since the first public alpha. This is the cleanup release:
a full command-surface test pass uncovered 7 bugs, we fixed all of them, and
hardened the cache so PCMs from different Clang majors can never collide.

## What to know before you upgrade

- **One-time cache miss on first build.** Cache keys now include the
  compiler version. Existing entries orphan harmlessly — `cmod cache clean`
  is optional if you want the space back.
- **First release build recompiles path deps.** Previously `cmod build
  --release` linked the debug artefacts of your path dependencies. That's
  a real bug; it's now fixed. First release build after upgrade will be
  slower; subsequent builds are cached.

Nothing else in the manifest, lockfile, or CLI surface changed incompatibly.

## Seven fixes

| Bug | Effect |
|---|---|
| `cmod vendor` rejecting all Git-URL dep names as path traversal | all offline workflows blocked — now works |
| `cmod verify --signatures` looking up repos at the wrong dir | always failed — now actually verifies |
| `cmod workspace add <existing-dir>` rejecting pre-existing dirs | can't integrate existing crates — now registers |
| `cmod run --release` looking for the binary in `build/debug` | always failed — now runs |
| `cmod build --release` linking debug path deps | mixed-profile ABI risk — now consistent |
| `cmod cache push/pull` missing a per-invocation remote flag | CI couldn't override cleanly — now accepts `--remote <URL>` |
| `example_projects` test harness copying stale build artefacts | CI flaky across Clang versions — now robust |

Details, repros, and the full test coverage matrix are in
[`docs/TEST_REPORT.md`](https://github.com/satishbabariya/cmod/blob/main/docs/TEST_REPORT.md).

## New CLI surface

```bash
cmod cache push --remote http://cache.internal:8080
cmod cache pull --remote http://cache.internal:8080
```

## Install

```bash
curl -sSf https://raw.githubusercontent.com/satishbabariya/cmod/main/install.sh | sh
```

Release notes, binaries, checksums: https://github.com/satishbabariya/cmod/releases/tag/v0.1.0-alpha.2

## What's next

- GCC and MSVC compiler backends
- Remote cache hardening
- VS Code extension
- Publishing the core crates to crates.io

Feedback welcome: https://github.com/satishbabariya/cmod/issues
```

### GitHub Release body

The `RELEASE.md` file in the repo is the canonical release-notes body; paste it verbatim into the GitHub Release.

## 12. Machine-readable assets to ship

Regenerate on every release:

| File | Command | Purpose |
|---|---|---|
| `compile_commands.json` | `cd examples/hello && cmod compile-commands` | clangd docs example |
| `plan.json` | `cmod plan > plan.json` | "What the DAG looks like" graphic |
| `graph.json` | `cmod graph --format json` | Interactive graph widget |
| `graph.dot` | `cmod graph --format dot` | SVG render |
| `sbom.json` | `cd examples/with-deps && cmod sbom > sbom.json` | Security/supply-chain page |
| `cache.json` | `cmod cache status-json` | Cache inspection UI |

---

_Content captured with `cmod 0.1.0-alpha.2` + Homebrew `llvm@18` on 2026-04-21. Regenerate with `cargo build --release && cargo test --release --workspace`._
