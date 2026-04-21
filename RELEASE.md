# cmod v0.1.0-alpha.2

**Audit-cleanup release.** Fixes 7 bugs surfaced by a full command-surface test pass (documented in `docs/TEST_REPORT.md`), hardens the cache key against Clang upgrades, and adds an ad-hoc `--remote` flag to `cmod cache push/pull`. No new features of scale — this is a stability release over `v0.1.0-alpha.1`.

---

## Upgrade notes

Two behaviour changes to be aware of — both are expected, neither requires user action:

- **One-time cache miss on first build.** `ClangBackend::detect_version()` now feeds `clang --version` into the cache key, so existing cache entries are orphaned. The next `cmod build` will rebuild from scratch once, then hit the cache as normal. Run `cmod cache clean` if you want to reclaim the orphaned space immediately.
- **First release build after upgrade recompiles path dependencies.** `cmod build --release` now correctly rebuilds path deps under the release profile (previously it linked the debug artefacts). If your project has many path deps, that first release build will be slower; subsequent builds are cached.

Nothing else in the manifest, lockfile, or CLI surface changed incompatibly.

## Bug fixes

| Bug | Severity | What changed |
|---|---|---|
| **BUG-01** | high | `cmod vendor` now encodes Git-URL dep names into filesystem-safe dirs (`vendor/github.com_fmtlib_fmt/`) instead of rejecting them as "path traversal" |
| **BUG-02** | high | `cmod verify --signatures` now opens repos at the sanitized path the resolver actually writes to |
| **BUG-03** | high | `cmod workspace add <dir>` registers an existing member if it has a `cmod.toml`, scaffolds if missing, rejects orphans — no more blanket "directory already exists" |
| **BUG-04** | medium | `cmod run --release` now locates the binary in `build/release/` |
| **BUG-05** | medium | Release builds now propagate profile / locked / offline / target settings into path-dep sub-builds |
| **BUG-06** | low | `cmod cache push/pull` now accept `--remote <URL>` as a per-invocation override of `[cache].shared_url` |
| **BUG-07** | low | Test harness no longer contaminates tempdirs with developer-built `build/` artefacts, so `cargo test --test example_projects` is stable across Clang versions |

## New CLI surface

```text
$ cmod cache push --help
Push local cache entries to remote cache

Options:
      --remote <URL>   Remote cache URL (overrides manifest [cache].shared_url)
```

## Under the hood

- `cmod_core::types` gains two shared helpers — `is_acceptable_package_name` (validation) and `sanitize_package_name_for_path` (filesystem encoding). The resolver, vendor, verify, and policy crates all route through them, so on-disk dep directory naming has a single source of truth.
- `BuildRunner` memoizes the compiler version via `OnceLock` to avoid re-running `clang --version` for every source file.
- `ArtifactMetadata` now records `compiler_version` and `target` instead of empty strings.

## Installation

### Pre-built binaries

```bash
# Latest
curl -sSf https://raw.githubusercontent.com/satishbabariya/cmod/main/install.sh | sh

# Pin to this version
curl -sSf https://raw.githubusercontent.com/satishbabariya/cmod/main/install.sh | sh -s -- --version v0.1.0-alpha.2
```

### From source

```bash
git clone https://github.com/satishbabariya/cmod.git
cd cmod && git checkout v0.1.0-alpha.2
cargo install --path crates/cmod-cli
```

### Download

| Platform | Architecture | Archive |
|----------|-------------|---------|
| Linux | x86_64 (glibc) | `cmod-v0.1.0-alpha.2-x86_64-unknown-linux-gnu.tar.gz` |
| Linux | aarch64 (glibc) | `cmod-v0.1.0-alpha.2-aarch64-unknown-linux-gnu.tar.gz` |
| Linux | x86_64 (musl, static) | `cmod-v0.1.0-alpha.2-x86_64-unknown-linux-musl.tar.gz` |
| macOS | x86_64 (Intel) | `cmod-v0.1.0-alpha.2-x86_64-apple-darwin.tar.gz` |
| macOS | aarch64 (Apple Silicon) | `cmod-v0.1.0-alpha.2-aarch64-apple-darwin.tar.gz` |
| Windows | x86_64 | `cmod-v0.1.0-alpha.2-x86_64-pc-windows-msvc.zip` |
| Windows | aarch64 | `cmod-v0.1.0-alpha.2-aarch64-pc-windows-msvc.zip` |

SHA-256 checksums are provided as `checksums-v0.1.0-alpha.2.sha256`.

### Runtime requirements

- **LLVM/Clang 17+** on `PATH`. On macOS, `brew install llvm@18` is recommended — Apple's bundled `/usr/bin/clang++` mis-handles the C++20 global module fragment.
- **Git 2.25+** for fetching dependencies.

## Feedback

- Issues: https://github.com/satishbabariya/cmod/issues
- Full changelog: https://github.com/satishbabariya/cmod/compare/v0.1.0-alpha.1...v0.1.0-alpha.2
