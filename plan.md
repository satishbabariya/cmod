# Next Implementation Steps — Phase 5

## Current State (340 tests, 26 CLI commands)

**Implemented:** Core RFCs (0001-0008, 0019, UNIFIED), plus Phase 3-4 work:
incremental builds, remote cache wiring, cfg() dep filtering, SBOM, publish,
clean, workspace CLI, lint, fmt, search, run, --force, --remote-cache,
target-specific resolution, module graph validation.

**Remaining RFCs:** 0009 (security gaps), 0010 (IDE/compile_commands.json),
0011 (precompiled BMI distribution), 0012 (advanced build strategies),
0013 (distributed builds), 0014 (graph enhancements), 0015 (governance),
0016 (advanced LSP), 0017 (dep overrides/conflict diagnostics), 0018 (plugin system).

**Focus:** Close Tier 2 gaps (0009, 0010, 0014, 0017) and lay foundations
for Tier 3-4 (0012, 0018). Prioritize features that close integration gaps
in existing code over brand-new subsystems.

---

## Step 1: compile_commands.json Generation (RFC-0010)

Critical for IDE adoption. After building the module graph and plan, emit a
`compile_commands.json` file that clangd/IDEs can consume.

- Add `generate_compile_commands()` to `cmod-build` that walks the BuildPlan
  and emits the JSON compilation database format
- Wire it into `build_module()` / `build_workspace()` — write to `build/`
- Add `cmod compile-commands` standalone command for generating without building
- Format: `[{directory, file, arguments, output}]` per source file

**Files:** `crates/cmod-build/src/plan.rs`, `crates/cmod-cli/src/commands/build.rs`,
new `crates/cmod-cli/src/commands/compile_commands.rs`

## Step 2: TrustDb Integration into Resolve Workflow (RFC-0009)

The TrustDb is fully implemented but never called during `cmod resolve`.
Wire TOFU into the dependency resolution flow.

- In `resolver.rs`, after `git::fetch_repo()`, call `trust_on_first_use()`
  for new dependencies and verify `origin_matches()` for existing ones
- When origin mismatch is detected, emit `CmodError::SecurityViolation`
- Save the trust database after successful resolution
- When `cmod add` succeeds, automatically create a trust entry
- Add `--untrusted` global flag to bypass trust checks

**Files:** `crates/cmod-resolver/src/resolver.rs`,
`crates/cmod-security/src/trust.rs`, `crates/cmod-cli/src/main.rs`,
`crates/cmod-cli/src/commands/resolve.rs`

## Step 3: Build Hooks — pre-build / post-build (RFC-0018 foundation)

Add manifest-driven build hooks that run shell commands before/after build.
This is the minimal viable piece of RFC-0018 (plugin system).

- Add `[hooks]` section to manifest: `pre_build = "scripts/generate.sh"`,
  `post_build = "scripts/deploy.sh"`, `pre_publish = "scripts/check.sh"`
- Parse in manifest.rs, execute in build command before/after compilation
- Hooks run in project root, inherit environment, fail-fast on non-zero exit
- Add `--no-hooks` flag to skip
- Wire pre-publish hook into `cmod publish`

**Files:** `crates/cmod-core/src/manifest.rs`, `crates/cmod-cli/src/commands/build.rs`,
`crates/cmod-cli/src/commands/publish.rs`, `crates/cmod-cli/src/main.rs`

## Step 4: Graph Build-Status Coloring & Critical Path (RFC-0014)

Enhance `cmod graph` with build state awareness:

- Load `.cmod-build-state.json` and annotate graph nodes with status
  (up-to-date, needs-rebuild, never-built)
- In ASCII output: prefix nodes with markers [ok], [!!], [??]
- In DOT output: color nodes green/yellow/red via fillcolor
- In JSON output: add `status` and `last_build_time` fields
- Add `--critical-path` flag showing the longest dependency chain

**Files:** `crates/cmod-cli/src/commands/graph.rs`,
`crates/cmod-build/src/incremental.rs`

## Step 5: `--verify` Flag on Build + Lockfile Integrity Hash (RFC-0009)

The RFC specifies `cmod build --locked --verify` as the CI-recommended
mode. Add verification before build begins.

- Add `--verify` boolean flag to the Build command
- When set: verify all package hashes via `verify_checkout_hash()`,
  check lockfile integrity hash, fail with exit code 3 on mismatch
- Add `integrity` field to Lockfile struct — SHA-256 of the lockfile
  contents, validated on load when `--verify` is set
- Also add real GPG/SSH signature verification in `check_commit_signature()`:
  shell out to `gpg --verify` or `ssh-keygen -Y verify` instead of
  just checking for signature presence

**Files:** `crates/cmod-cli/src/main.rs`, `crates/cmod-cli/src/commands/build.rs`,
`crates/cmod-core/src/lockfile.rs`, `crates/cmod-security/src/verify.rs`

## Step 6: Cache Eviction Policies (RFC-0012)

The cache grows unboundedly. Add LRU and TTL-based eviction.

- Add `evict_by_age(max_age: Duration)` — remove entries older than TTL
- Add `evict_by_size(max_bytes: u64)` — LRU eviction until under threshold
- Wire the manifest `[cache].ttl` field (already parsed but unused) into
  auto-eviction after each `store()` call
- Add `max_size` to the `[cache]` manifest section
- Add `cmod cache gc` subcommand for manual eviction

**Files:** `crates/cmod-cache/src/cache.rs`, `crates/cmod-core/src/manifest.rs`,
`crates/cmod-cli/src/main.rs`, `crates/cmod-build/src/runner.rs`

## Step 7: Per-Node Build Timing & Conflict Diagnostics (RFC-0012 + RFC-0017)

Two related improvements: build profiling and better error messages.

- Record per-node compilation time in `BuildStats` via
  `node_timings: BTreeMap<String, Duration>`
- Add `--timings` flag to build command that prints a timing report
- Persist node timings in `BuildState` so they survive across builds
- In resolver, when a version conflict is detected, collect both sides
  (which parent required which version) and include in the error
- Add `cmod deps --why <dep>` flag for reverse dependency trace

**Files:** `crates/cmod-build/src/runner.rs`, `crates/cmod-build/src/incremental.rs`,
`crates/cmod-cli/src/commands/build.rs`, `crates/cmod-resolver/src/resolver.rs`,
`crates/cmod-cli/src/commands/deps.rs`

## Step 8: Integration Tests for Phase 5

Add integration and unit tests for all new features:

- compile_commands.json generation and format validation
- TrustDb integration with resolve (new dep creates trust entry)
- Build hook execution (pre/post, failure handling)
- Graph status coloring output in all three formats
- `--verify` flag on build (pass and fail cases)
- Cache eviction by age and size
- Build timing report output
- Version conflict diagnostic messages
- End-to-end workflow combining hooks + verify + compile_commands

**Files:** `crates/cmod-cli/tests/cli_integration.rs`, unit tests in each module
