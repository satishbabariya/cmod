# Implementation Plan: Continue RFC Implementation

## Current State Summary

**162 passing tests** across 6 crates. Core RFCs (0001-0004, UNIFIED) and Phase 0-2 are substantially implemented. The codebase has working CLI, dependency resolution, build orchestration, local caching infrastructure, and workspace management.

### Key Gaps in Existing Implementations
1. **Cache not wired into builds** — `ArtifactCache` exists but `BuildRunner::execute_plan()` never checks/stores cache entries
2. **No `clang-scan-deps` integration** — module graph is built via regex, not via `CompilerBackend::scan_deps()`
3. **Sequential-only builds** — `execute_plan()` processes nodes one at a time
4. **`[features]` parsed but unused** in resolution
5. **No integration tests** — `tests/` directory is empty
6. **No `cmod-security` crate** — Phase 4 not started

---

## Plan: 8 Steps across Tier 1 Completion + Tier 2/3 Start

### Step 1: Integrate Cache into Build Pipeline (RFC-0005 + RFC-0007)
**Files:** `crates/cmod-build/src/runner.rs`, `crates/cmod-cache/src/key.rs`

Connect the already-implemented `ArtifactCache` and `CacheKey` systems into the build runner:

- Before compiling each node in `execute_plan()`:
  - Compute `CacheKey` from source hash + dep hashes + compiler info + target
  - Check `ArtifactCache::has()` — if hit, copy cached artifact to output path and skip compilation
- After successful compilation of each node:
  - Store the output artifact(s) in the cache via `ArtifactCache::store()`
- Add a `--no-cache` flag to bypass cache (global CLI flag)
- Store metadata (timestamps, source paths) alongside cached artifacts
- Add tests for cache-integrated build flow

### Step 2: Parallel Build Execution (RFC-0007 + RFC-0012)
**Files:** `crates/cmod-build/src/runner.rs`, `crates/cmod-build/Cargo.toml`

Replace sequential `execute_plan()` with a parallel scheduler:

- Add `rayon` or `crossbeam` dependency for thread pool
- Implement a ready-queue scheduler:
  - Track in-degree of each node
  - When a node's dependencies are all complete, enqueue it for execution
  - Execute enqueued nodes in parallel (bounded by CPU cores or `--jobs N`)
- Preserve correct ordering: interface nodes before their dependents
- On failure, skip all downstream nodes and collect errors
- Add `--jobs <N>` CLI flag (defaults to num_cpus)
- Add tests for parallel execution ordering correctness

### Step 3: `clang-scan-deps` Integration for Graph Construction (RFC-0007)
**Files:** `crates/cmod-cli/src/commands/build.rs`, `crates/cmod-build/src/compiler.rs`

Replace/supplement the regex-based `extract_imports_from_source()` with actual compiler-based scanning:

- In the build command, attempt to use `CompilerBackend::scan_deps()` when `clang-scan-deps` is available
- Parse P1689 JSON output (parser already exists in `compiler.rs`)
- Fall back to regex scanning when `clang-scan-deps` is not found
- Wire the scanned dependencies into `ModuleGraph` construction
- Add tests with mock P1689 output

### Step 4: Feature Flag Resolution (RFC-0006 + RFC-UNIFIED)
**Files:** `crates/cmod-core/src/manifest.rs`, `crates/cmod-resolver/src/resolver.rs`

Implement the `[features]` section that is already parsed but not used:

- Add feature flag resolution during dependency resolution
- Support `default-features = false` and `features = ["x", "y"]` in dependency specs
- Resolve feature-gated optional dependencies
- Pass enabled features through to build configuration
- Add `--features` and `--no-default-features` CLI flags
- Add tests for feature resolution

### Step 5: Create `cmod-security` Crate (RFC-0009)
**Files:** New `crates/cmod-security/` crate

Create the security crate for Phase 4 foundations:

- **`crates/cmod-security/src/lib.rs`** — crate root
- **`crates/cmod-security/src/verify.rs`** — commit signature verification using `git2` signature APIs (GPG, SSH)
- **`crates/cmod-security/src/hash.rs`** — content hash verification against lockfile entries
- **`crates/cmod-security/src/audit.rs`** — dependency audit scaffolding (license check, known-vulnerability warnings)
- **`crates/cmod-security/src/trust.rs`** — TOFU trust model: first-seen key recording, trust database (`~/.config/cmod/trust.toml`)
- Wire into `cmod verify --signatures` command
- Make exit code 3 functional for security violations
- Update `Cargo.toml` workspace members
- Add tests for signature verification and hash checking

### Step 6: `cmod graph` Command (RFC-0014)
**Files:** `crates/cmod-cli/src/commands/`, `crates/cmod-cli/src/main.rs`

Add the graph visualization command:

- **`crates/cmod-cli/src/commands/graph.rs`** — new `graph` subcommand
- Output formats:
  - ASCII tree (default) — reuse `deps --tree` style with box-drawing
  - DOT format (`--format dot`) — for Graphviz rendering
  - JSON format (`--format json`) — for IDE consumption
- Node annotations: module name, build state (cached/needs-rebuild/failed)
- Support `--filter <pattern>` to focus on specific modules
- Wire into clap CLI in `main.rs`
- Add tests for DOT and JSON output generation

### Step 7: `cmod explain` and `cmod status` Commands (RFC-0014)
**Files:** `crates/cmod-cli/src/commands/`, `crates/cmod-cli/src/main.rs`

Add developer insight commands:

- **`cmod explain <module>`** — show why a module would be rebuilt:
  - Source file changed
  - Dependency BMI changed
  - Compiler/flags changed
  - Cache miss reason
- **`cmod status`** — show project state:
  - Modules: total, cached, needs-rebuild
  - Cache: size, hit rate from last build
  - Deps: resolved count, locked status
  - Workspace: member count if applicable
- Add tests for explain output logic

### Step 8: Integration Tests + Housekeeping
**Files:** `tests/`, `CLAUDE.md`

- Create integration tests that exercise the full CLI pipeline:
  - `cmod init` → `cmod build` flow
  - `cmod add` → `cmod resolve` → `cmod build` flow
  - `cmod verify` validation
  - `cmod graph` output
  - Cache hit/miss scenarios
- Update `CLAUDE.md` to reflect actual test count (162+) and new commands
- Update `todo.txt` with remaining work items

---

## Dependency Order

```
Step 1 (Cache Integration) ──┐
Step 3 (clang-scan-deps)  ───┤──→ Step 2 (Parallel Builds)
Step 4 (Feature Flags)    ───┘
Step 5 (Security Crate)  ─── independent
Step 6 (cmod graph)       ─── independent
Step 7 (explain/status)   ─── depends on Step 1 (cache stats)
Step 8 (Integration Tests) ── after Steps 1-7
```

Steps 1, 3, 4, 5, 6 can be developed in parallel. Step 2 should follow Step 1. Step 7 depends on cache integration. Step 8 is last.

## Files Modified/Created Summary

| Action | Path |
|--------|------|
| Modify | `crates/cmod-build/src/runner.rs` |
| Modify | `crates/cmod-build/Cargo.toml` |
| Modify | `crates/cmod-cache/src/key.rs` |
| Modify | `crates/cmod-cli/src/main.rs` |
| Modify | `crates/cmod-cli/src/commands/build.rs` |
| Modify | `crates/cmod-cli/src/commands/verify.rs` |
| Modify | `crates/cmod-core/src/manifest.rs` |
| Modify | `crates/cmod-resolver/src/resolver.rs` |
| Modify | `Cargo.toml` (workspace members) |
| Modify | `CLAUDE.md` |
| Create | `crates/cmod-security/Cargo.toml` |
| Create | `crates/cmod-security/src/lib.rs` |
| Create | `crates/cmod-security/src/verify.rs` |
| Create | `crates/cmod-security/src/hash.rs` |
| Create | `crates/cmod-security/src/audit.rs` |
| Create | `crates/cmod-security/src/trust.rs` |
| Create | `crates/cmod-cli/src/commands/graph.rs` |
| Create | `tests/cli_integration.rs` |
