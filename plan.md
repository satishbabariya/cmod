# Next Implementation Steps — Phase 3

## Step 1: Wire Incremental Rebuild into BuildRunner
The `incremental.rs` module exists with `BuildState`, `needs_rebuild()`, and `record_node()` but
is never called from `runner.rs`. Wire it in so that before doing expensive cache lookups, the
runner checks the local build state first and skips up-to-date nodes.

**Files:** `crates/cmod-build/src/runner.rs`

## Step 2: Wire Remote Cache into BuildRunner
The `RemoteCache` trait and `HttpRemoteCache` exist but the build runner only uses local cache.
Add remote cache as a fallback: on local miss, check remote; after compile, push to remote.

**Files:** `crates/cmod-build/src/runner.rs`, `crates/cmod-cache/src/lib.rs`

## Step 3: Build Progress & Statistics Reporting
`BuildStats` exists with timing data but is discarded in `build()`. Show cache hit rates,
wall time, parallelism speedup, and per-node timing in verbose mode.

**Files:** `crates/cmod-cli/src/commands/build.rs`, `crates/cmod-build/src/runner.rs`

## Step 4: Conditional / Platform-Specific Dependencies
Add `[target.'cfg(...)'.dependencies]` support so dependencies can be filtered by platform.
Parse `cfg()` expressions and evaluate them against the current target.

**Files:** `crates/cmod-core/src/manifest.rs`, `crates/cmod-resolver/src/resolver.rs`

## Step 5: `cmod clean` Command
Add a `clean` command to remove build artifacts (build dir, build state) while leaving the cache
and vendored deps intact.

**Files:** `crates/cmod-cli/src/commands/clean.rs`, `crates/cmod-cli/src/main.rs`

## Step 6: Workspace CLI Subcommands
Expose workspace management via `cmod workspace add/list/remove` subcommands using the
existing `WorkspaceManager` infrastructure.

**Files:** `crates/cmod-cli/src/commands/workspace.rs`, `crates/cmod-cli/src/main.rs`

## Step 7: SBOM Generation (`cmod sbom`)
Generate Software Bill of Materials from the lockfile in CycloneDX-like JSON format for
supply chain transparency (RFC-0009).

**Files:** `crates/cmod-security/src/sbom.rs`, `crates/cmod-cli/src/commands/sbom.rs`, `crates/cmod-cli/src/main.rs`

## Step 8: `cmod publish` Command
Package the current module for distribution — validate manifest, create a Git tag, and
optionally push. Foundation for RFC-0015 ecosystem.

**Files:** `crates/cmod-cli/src/commands/publish.rs`, `crates/cmod-cli/src/main.rs`
