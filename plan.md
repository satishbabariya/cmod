# Next Implementation Steps — Phase 4

## Current State (301 tests, 21 CLI commands)

**Implemented:** All core RFCs (0001-0008, 0019), plus incremental builds wired into runner,
remote cache wired into runner, cfg() dependency filtering, SBOM generation, publish command,
clean command, workspace subcommands, build stats reporting.

**Key gaps found:** `--force` flag not exposed in CLI, resolver doesn't call
`effective_dependencies()` for target filtering, remote cache never instantiated from
build command, module import validation missing, no LSP server, no plugin system.

---

## Step 1: `--force` Flag + Remote Cache Activation in Build Command

The `BuildRunner` has `force_rebuild` and `remote_cache` fields but neither is reachable
from the CLI. Wire both through:

- Add `--force` flag to `Build` command in main.rs
- Pass force flag through `build.rs` → `runner.with_force(force)`
- Read `[cache].shared_url` from manifest, instantiate `HttpRemoteCache`, pass to runner
- Add `--remote-cache <url>` CLI override

**Files:** `crates/cmod-cli/src/main.rs`, `crates/cmod-cli/src/commands/build.rs`

## Step 2: Target-Specific Dependency Filtering in Resolver

`Manifest::effective_dependencies()` exists but the resolver never calls it. When resolving
dependencies, the resolver should use the current target triple to filter platform-specific
deps before iterating.

- In `resolve_with_features()`, call `manifest.effective_dependencies(target)` instead of
  reading `manifest.dependencies` directly
- Thread the target triple from `Config` through to the resolver
- Update `Resolver::resolve()` / `resolve_with_features()` to accept an optional target

**Files:** `crates/cmod-resolver/src/resolver.rs`, `crates/cmod-cli/src/commands/resolve.rs`

## Step 3: Module Import Validation in Graph Builder

The graph builder silently drops unknown imports. Add validation:

- After building the `ModuleGraph`, check that every import in a node's dependencies
  either resolves to another graph node or to a dependency in the lockfile
- Emit a clear error naming the unresolved import and which source file references it
- Add `--allow-missing-imports` flag for migration scenarios

**Files:** `crates/cmod-build/src/graph.rs`, `crates/cmod-cli/src/commands/build.rs`

## Step 4: `cmod lint` Command

Add a lint command that statically checks the project for common issues without building:

- Manifest validation (already exists, wire to CLI)
- Module naming convention enforcement (reverse-domain Git path)
- Unused dependency detection (deps in manifest not imported by any source)
- Circular import detection (already in graph.validate(), expose diagnostics)
- Lockfile staleness check (manifest deps changed but lock not updated)

**Files:** `crates/cmod-cli/src/commands/lint.rs`, `crates/cmod-cli/src/main.rs`

## Step 5: `cmod fmt` / Configuration File Normalization

Add a formatter that canonicalizes `cmod.toml`:

- Sort dependency keys alphabetically
- Normalize version constraints to canonical form (`^1.2` → `^1.2.0`)
- Remove duplicate/redundant fields
- Ensure consistent key ordering ([package], [module], [dependencies], ...)
- Support `--check` mode (exit non-zero if changes needed, for CI)

**Files:** `crates/cmod-cli/src/commands/fmt.rs`, `crates/cmod-cli/src/main.rs`

## Step 6: `cmod search` Command

Search for C++ modules on GitHub by name/topic:

- Use GitHub API search (`gh api search/repositories`) to find repos with cmod.toml
- Display name, description, version, stars, last updated
- Support `--limit`, `--sort` flags
- Fallback for offline mode: search local cache / known registries

**Files:** `crates/cmod-cli/src/commands/search.rs`, `crates/cmod-cli/src/main.rs`

## Step 7: `cmod run` Command

Build and execute a binary module in one step (like `cargo run`):

- Build with the current profile
- Locate the output binary from the build plan
- Execute it, forwarding any trailing arguments
- Support `--release` flag

**Files:** `crates/cmod-cli/src/commands/run.rs`, `crates/cmod-cli/src/main.rs`

## Step 8: Integration Test Hardening

Add end-to-end integration tests that exercise real workflows:

- `cmod init` → `cmod build` → `cmod clean` round-trip
- `cmod init --workspace` → `cmod workspace add` → `cmod workspace list`
- Manifest with `[target.'cfg(unix)'.dependencies]` → `cmod resolve`
- `cmod sbom` output validates as JSON
- `cmod publish --dry-run` succeeds on a clean repo
- `cmod verify` on a real git repo with commits

**Files:** `tests/integration/` (new directory with test files)
