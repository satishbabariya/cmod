# Implementation Plan: Next RFC Implementation Round

## Current State Summary

**230 passing tests** across 7 crates. Previous round implemented cache integration, parallel build infrastructure (`--jobs`), clang-scan-deps, feature flags, security crate, graph/status/explain/audit CLI commands, and integration tests.

### What's Already Implemented
- Cache wired into `BuildRunner::execute_plan()` with check-before-compile, store-after-compile
- `--jobs`, `--no-cache`, `--features`, `--no-default-features` CLI flags
- `cmod-security` crate: verify, hash, trust (TOFU), audit modules (29 tests)
- `cmod graph` (ASCII/DOT/JSON), `cmod status`, `cmod explain`, `cmod audit` commands
- `clang-scan-deps` integration with regex fallback
- Feature flag resolution (`dep:name`, `dep/feature`, transitive features)

### Key Gaps Remaining
1. **Parallel build execution is infrastructure-only** — `--jobs` flag exists, `effective_jobs()` computes the count, but `execute_plan()` still compiles sequentially
2. **Feature resolution not wired into resolver** — `resolve_features()` exists but `Resolver::resolve()` doesn't call `should_include_dep()` to filter optional deps
3. **No cross-compilation support** — RFC-0008 toolchain/target management not implemented
4. **No `cmod vendor` command** — RFC-0006 specifies vendoring support
5. **No remote/shared cache** — RFC-0005 specifies remote cache protocol (currently local-only)
6. **Signature verification is presence-check only** — detects GPG/SSH headers but doesn't verify cryptographic validity
7. **No `cmod publish`/`cmod search` commands** — ecosystem tooling from RFC-0015
8. **No precompiled BMI distribution** — RFC-0011 BMI packaging not started
9. **Workspace glob patterns** — RFC-0019 specifies glob matching for `members`, currently string-exact only
10. **No incremental rebuild detection** — RFC-0007/0012 specify hash-based invalidation for partial rebuilds
11. **Manifest schema incomplete** — RFC-UNIFIED specifies `[security]`, `[ide]`, `[plugins]`, `[metadata]` sections not yet in `Manifest` struct

---

## Plan: 8 Steps — Deep Tier 1 Completion + Tier 2/3 Advancement

### Step 1: Actual Parallel Build Execution (RFC-0007 + RFC-0012)
**Files:** `crates/cmod-build/src/runner.rs`, `crates/cmod-build/Cargo.toml`

Wire the existing `--jobs` infrastructure to actual concurrent compilation:

- Add `crossbeam` and `crossbeam-channel` dependencies for thread-safe work stealing
- Replace the sequential `for node in &plan.nodes` loop with a parallel scheduler:
  - Track completion state and in-degree count per node
  - Maintain a ready-queue of nodes whose dependencies are all complete
  - Spawn worker threads (bounded by `effective_jobs()`) that pull from the ready queue
  - Workers execute compile/link steps and signal completion
  - On node completion, decrement in-degrees of dependents and enqueue newly-ready nodes
- Interface nodes must still complete before their dependent implementation nodes
- Link node waits for all compile nodes
- On any failure: stop enqueuing new work, let in-flight jobs finish, collect all errors
- Add `BuildStats::wall_time` and `BuildStats::total_compile_time` fields
- Add tests verifying correct ordering under parallel execution

### Step 2: Wire Feature Resolution into Dependency Resolver (RFC-0006 + RFC-0017)
**Files:** `crates/cmod-resolver/src/resolver.rs`, `crates/cmod-resolver/src/features.rs`, `crates/cmod-cli/src/commands/build.rs`, `crates/cmod-cli/src/commands/resolve.rs`

Connect the existing feature resolution system to the resolver and build pipeline:

- In `Resolver::resolve()`, call `resolve_features()` before iterating dependencies
- Use `should_include_dep()` to skip optional deps that are not activated by features
- Pass `--features` and `--no-default-features` from CLI through to `Resolver::resolve()`
- Thread feature flags through `Config` so build commands can read them
- Add `enabled_features` field to `Config`
- Store activated features in lockfile metadata (add `features` field to `LockedPackage`)
- Forward dep-specific feature flags when resolving transitive dependencies
- Add tests: optional dep excluded without feature, included with feature, transitive feature propagation

### Step 3: Cross-Compilation & Toolchain Management (RFC-0008)
**Files:** `crates/cmod-core/src/types.rs`, `crates/cmod-core/src/config.rs`, `crates/cmod-build/src/compiler.rs`, `crates/cmod-cli/src/commands/build.rs`

Implement the toolchain/target management layer:

- Add `ToolchainSpec` struct: `(compiler, version, cxx_standard, stdlib, abi, target, sysroot)`
- Add `ToolchainSpec::from_manifest()` that reads `[toolchain]` section
- Add `ToolchainSpec::validate()` that checks compiler availability
- Support `--target <triple>` to override the manifest target at build time (already partially exists)
- In `ClangBackend`, add `sysroot` support (`--sysroot=<path>` flag)
- Add cross-compilation detection: when target != host, require explicit sysroot or toolchain path
- Ensure cache keys include full toolchain tuple (already partially done, needs sysroot/abi)
- Add `cmod toolchain` subcommand: `cmod toolchain show` (display active toolchain), `cmod toolchain check` (validate)
- Add tests for toolchain validation, cross-target cache key isolation

### Step 4: `cmod vendor` Command (RFC-0006)
**Files:** `crates/cmod-cli/src/commands/vendor.rs`, `crates/cmod-cli/src/main.rs`, `crates/cmod-resolver/src/resolver.rs`

Implement dependency vendoring for offline/airgapped builds:

- New `cmod vendor` command that copies all resolved dependencies into a `vendor/` directory
- For each dependency in the lockfile:
  - If Git dep: clone/fetch the repo at the pinned commit, copy source tree to `vendor/<name>/`
  - If path dep: create symlink or copy to `vendor/<name>/`
- Generate a `vendor/config.toml` that maps dependency names to local paths
- Add `--sync` flag to re-vendor after lockfile changes
- Support `cmod build --vendor-dir <path>` to use vendored deps instead of fetching
- Add `path_override` support in resolver: when vendor dir exists, prefer it over Git
- Add tests for vendor workflow

### Step 5: Incremental Rebuild Detection (RFC-0007 + RFC-0012)
**Files:** `crates/cmod-build/src/runner.rs`, `crates/cmod-build/src/plan.rs`, `crates/cmod-cache/src/key.rs`

Implement hash-based invalidation for efficient partial rebuilds:

- Add `.cmod-build-state.json` file that records per-node:
  - Source file content hash
  - Dependency hashes (imported module BMI hashes)
  - Compiler flags hash
  - Output file mtime + hash
- Before compilation, compare current inputs against saved state:
  - If all inputs unchanged and outputs exist → skip compilation entirely (faster than cache lookup)
  - If inputs changed → recompile and update state
- Compute the transitive invalidation set using `ModuleGraph::invalidation_set()`
- Only rebuild nodes in the invalidation set + their downstream dependents
- Display rebuild reasons in `cmod explain` (already scaffolded, make it data-driven)
- `cmod build --force` flag to bypass incremental state and rebuild everything
- Add tests for incremental detection

### Step 6: Expanded Manifest Schema (RFC-UNIFIED + RFC-0017)
**Files:** `crates/cmod-core/src/manifest.rs`, `crates/cmod-core/src/types.rs`

Add missing manifest sections from the unified schema:

- Add `[metadata]` section: `category`, `keywords`, `links`, `documentation`
  ```rust
  pub struct Metadata {
      pub category: Option<String>,
      pub keywords: Vec<String>,
      pub links: BTreeMap<String, String>,
      pub documentation: Option<String>,
  }
  ```
- Add `[security]` section: `signing`, `verify_checksums`, `trusted_sources`
  ```rust
  pub struct Security {
      pub signing: Option<SigningConfig>,
      pub verify_checksums: Option<bool>,
      pub trusted_sources: Vec<String>,
  }
  ```
- Add `[publish]` section: `registry`, `include`, `exclude`, `tags`
  ```rust
  pub struct Publish {
      pub registry: Option<String>,
      pub include: Vec<String>,
      pub exclude: Vec<String>,
      pub tags: Vec<String>,
  }
  ```
- Add conditional dependencies: `[target.'cfg(...)'.dependencies]` parsing
- Add `default-features = false` support in `DetailedDependency`
- Add `Manifest::validate()` method: check module name matches `export module` decl, version is valid SemVer, dependencies are resolvable, etc.
- Add serde tests for roundtrip of all new sections

### Step 7: Workspace Enhancements (RFC-0019)
**Files:** `crates/cmod-workspace/src/workspace.rs`, `crates/cmod-cli/src/commands/init.rs`

Improve workspace support to match RFC-0019 specification:

- Support glob patterns in `workspace.members` (e.g., `"crates/*"`, `"libs/**"`)
  - Use the `glob` crate for pattern matching
  - Expand patterns to actual directories at load time
- Support `workspace.exclude` patterns to skip matching directories
- Add `cmod workspace add <name>` subcommand (scaffolded in `add_member()`, expose via CLI)
- Add `cmod workspace list` subcommand to show all members
- Workspace-level build ordering: resolve inter-member dependencies and build in correct order
  - Members that depend on other members (via path deps) must build after their dependencies
  - Build members in topological order, with independent members parallelizable
- Add `workspace.version` field for unified versioning across all members
- Add tests for glob expansion, exclude patterns, inter-member ordering

### Step 8: Remote Cache Protocol Scaffolding (RFC-0005 + RFC-0013)
**Files:** `crates/cmod-cache/src/cache.rs`, `crates/cmod-cache/src/remote.rs` (new), `crates/cmod-core/src/manifest.rs`

Lay groundwork for distributed/shared caching:

- Add `RemoteCache` trait:
  ```rust
  pub trait RemoteCache: Send + Sync {
      fn has(&self, module_id: &str, key: &CacheKey) -> Result<bool, CmodError>;
      fn get(&self, module_id: &str, key: &CacheKey, dest: &Path) -> Result<bool, CmodError>;
      fn put(&self, module_id: &str, key: &CacheKey, artifacts: &[(&str, &Path)]) -> Result<(), CmodError>;
  }
  ```
- Implement `HttpRemoteCache` using `ureq` for simple HTTP GET/PUT:
  - `GET /cache/<module_id>/<key>/<artifact_name>` to download
  - `PUT /cache/<module_id>/<key>/<artifact_name>` to upload
  - `HEAD /cache/<module_id>/<key>` to check existence
- Wire into `BuildRunner`: check remote cache after local cache miss, store to remote after local store
- Support `[cache]` manifest config: `shared_url`, `mode` (off/readonly/readwrite), `ttl`
- Add `cmod cache push` and `cmod cache pull` subcommands for explicit sync
- Content verification: hash-check downloaded artifacts before use
- Add tests with mock HTTP server

---

## Dependency Order

```
Step 1 (Parallel Builds)      ─── independent, high priority
Step 2 (Feature→Resolver)     ─── independent, high priority
Step 3 (Toolchain/Cross)      ─── independent, medium priority
Step 4 (Vendor)               ─── depends on Step 2 (needs resolver changes)
Step 5 (Incremental Rebuild)  ─── depends on Step 1 (needs parallel-aware state)
Step 6 (Manifest Schema)      ─── independent, can be done anytime
Step 7 (Workspace Globs)      ─── independent
Step 8 (Remote Cache)         ─── depends on Step 1 (needs build stats for metrics)
```

Recommended execution order: 1, 2, 3, 6, 7, 5, 4, 8

## Files Modified/Created Summary

| Action | Path |
|--------|------|
| Modify | `crates/cmod-build/src/runner.rs` |
| Modify | `crates/cmod-build/Cargo.toml` |
| Modify | `crates/cmod-build/src/plan.rs` |
| Modify | `crates/cmod-build/src/compiler.rs` |
| Modify | `crates/cmod-resolver/src/resolver.rs` |
| Modify | `crates/cmod-resolver/src/features.rs` |
| Modify | `crates/cmod-cache/src/cache.rs` |
| Modify | `crates/cmod-cache/src/key.rs` |
| Modify | `crates/cmod-cache/Cargo.toml` |
| Modify | `crates/cmod-core/src/manifest.rs` |
| Modify | `crates/cmod-core/src/types.rs` |
| Modify | `crates/cmod-core/src/config.rs` |
| Modify | `crates/cmod-workspace/src/workspace.rs` |
| Modify | `crates/cmod-workspace/Cargo.toml` |
| Modify | `crates/cmod-cli/src/main.rs` |
| Modify | `crates/cmod-cli/src/commands/build.rs` |
| Modify | `crates/cmod-cli/src/commands/resolve.rs` |
| Modify | `crates/cmod-cli/src/commands/init.rs` |
| Create | `crates/cmod-cli/src/commands/vendor.rs` |
| Create | `crates/cmod-cli/src/commands/toolchain.rs` |
| Create | `crates/cmod-cache/src/remote.rs` |
