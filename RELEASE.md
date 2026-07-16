# cmod v0.1.0-alpha.3

**Offline & distribution polish, plus compiler-backend groundwork.** Closes out the entire v0.1.0-alpha.3 milestone — all 15 planned items and all 3 stretch goals (#36): the remaining alpha.2 audit defects, the remote-cache story hardened end-to-end and documented, the compiler backend abstraction that GCC/MSVC support will build on, and 11 Dependabot security alerts resolved.

---

## Upgrade notes

Four behaviour changes to be aware of:

- **`cmod cache export` is now all-positional.** `cmod cache export <MODULE> <KEY> <OUTPUT>` — the `-o/--output` flag is rejected. Update scripts accordingly; the error is loud, not silent.
- **One-time cache miss on first build.** Cache keys now derive from the full compiler-configuration fingerprint (adding LTO mode, optimization level, and sysroot — previously missing, which could reuse stale artifacts). Existing entries are orphaned; the first build recompiles once. `cmod cache clean` reclaims the space if you care.
- **`cmod test --format json/junit` schemas corrected.** JSON `summary` gains `total`/`success` and no longer double-counts timeouts in `failed`; JUnit gains the suite-level count attributes CI parsers actually read, and maps timeouts/compile failures to `<error>`. CI consumers parsing the old shapes need a one-time update — the old JUnit output could even be invalid XML when compiler output contained ANSI escapes.
- **`[toolchain] compiler = "gcc"` (or `"msvc"`) now errors** with a clear not-implemented message instead of silently building with clang. Remove the line or set `"clang"`.

Also: **MSRV is now Rust 1.80** (was 1.74), required by the openssl security fix. This affects building cmod from source only.

## Highlights

### Remote cache, actually usable

- `[cache] auth_token_env`, `timeout`, and `retries` were documented but dead — never applied to any HTTP client. Now wired end-to-end (build, push, pull).
- `cmod cache push` no longer lies: upload failures are counted and reported, and a fully-failed push exits nonzero. On Windows it previously uploaded **zero artifacts** due to path-separator splitting — fixed.
- Downloads are atomic (`.part` + rename), so an interrupted transfer can't poison the local cache.
- New guide: `docs/guide/remote-cache.md` — the three-verb protocol spec plus hosting recipes validated against real servers (nginx read-write, Caddy read-only, stdlib-Python dev server).

### Compiler backend groundwork

- Construction goes through `make_backend(Compiler, &BackendConfig)`; the build pipeline holds `dyn CompilerBackend` and never sees a concrete type. A GCC backend is now one trait impl + one factory arm away.
- `MsvcBackend` skeleton validates the trait shape against MSVC's model (`.ifc` BMIs via the new `bmi_extension()`, `cl /scanDependencies` P1689 compatibility) with real flag mapping.
- `compile_commands.json` records the resolved compiler path instead of literal `clang++`.

### Fixed defects (carried from the alpha.2 audit)

| Issue | What changed |
|---|---|
| #38 | `cmod vendor --sync` re-runs reuse the existing clone (fetch + hard-reset) instead of failing on a non-empty directory |
| #39 | `[test] test_patterns` root-relative globs (`tests/**/*.cpp`) now match — affected projects stop reporting "No tests found" |
| #40 | `cache inspect` / `cache export` argument shapes are consistent (all-positional) |

### Developer experience

- `cmod workspace add --scaffold` asserts creation intent for scripts (default inference unchanged).
- `cmod graph` at a workspace root explains that graphs are per-member and lists the members.
- Git hooks (`.githooks/`): fmt on commit, clippy on push — `git config core.hooksPath .githooks`.
- CI gains a cross-target smoke job; CONTRIBUTING documents the commit conventions, CI matrix, and this release process.

### Security

- All 11 open Dependabot alerts resolved: `openssl` → 0.10.80 (5 high-severity fixes incl. AES key-wrap OOB writes), `rustls-webpki` → 0.103.13 (CRL panic DoS, name-constraint bypasses).

## Decision docs shipped

- **crates.io publishing** (`docs/plan-crates-io-publishing.md`): don't publish — `cmod`/`cmod-core` are owned by an unrelated active project; git deps serve consumers until 1.0 planning.
- **Search registry** (`docs/plan-search-registry.md`): the code side already exists; the phased plan bootstraps the index repo, then PR-based submissions.
- **VS Code extension** (`editors/vscode/PUBLISHING.md`): packaging + publish automation is complete and verified; first marketplace publish awaits publisher-account setup.

## Stats

- 18 PRs merged over the milestone (#37, #55–#72)
- 857 tests passing (was 828 at alpha.2)
- 8 crates, MSRV 1.80, clippy 1.97 clean
