# Next Implementation Steps — Phase 5

## Current State (340 tests, 26 CLI commands)

**Implemented:** Core RFCs (0001-0008, 0019, UNIFIED), plus Phase 3-4 work:
incremental builds, remote cache wiring, cfg() dep filtering, SBOM, publish,
clean, workspace CLI, lint, fmt, search, run, --force, --remote-cache,
target-specific resolution, module graph validation.

**Remaining RFCs:** 0009 (security gaps), 0010 (IDE/compile_commands.json),
0011 (precompiled BMI distribution), 0012 (advanced build strategies),
0013 (distributed builds), 0014 (graph enhancements), 0015 (governance),
0016 (advanced LSP), 0017 (dep overrides), 0018 (plugin system).

**Focus:** Tier 2 RFCs (0009, 0010, 0014, 0017) + foundational pieces of
Tier 3-4 (0011, 0018) that unlock future work.

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

## Step 2: Dependency Override / Patch Directives (RFC-0017)

Allow overriding transitive dependencies — essential for monorepos and
patching upstream bugs.

- Add `[patch]` section to manifest: `[patch."github.com/original/lib"]`
  with `path = "../my-fork"` or `git = "..."` + `branch = "fix"`
- Parse in manifest.rs, apply overrides in resolver before resolution
- Overrides replace the dependency at resolution time, recorded in lockfile

**Files:** `crates/cmod-core/src/manifest.rs`, `crates/cmod-resolver/src/resolver.rs`

## Step 3: Build Hooks — pre-build / post-build (RFC-0018 foundation)

Add manifest-driven build hooks that run shell commands before/after build.
This is the minimal viable piece of RFC-0018 (plugin system).

- Add `[hooks]` section: `pre-build = ["./scripts/generate.sh"]`,
  `post-build = ["./scripts/deploy.sh"]`
- Parse in manifest.rs, execute in build command before/after compilation
- Hooks run in project root, inherit environment, fail-fast on non-zero exit
- Add `--no-hooks` flag to skip

**Files:** `crates/cmod-core/src/manifest.rs`, `crates/cmod-cli/src/commands/build.rs`,
`crates/cmod-cli/src/main.rs`

## Step 4: Graph Build-Status Coloring & Critical Path (RFC-0014)

Enhance `cmod graph` with build state awareness:

- Load `.cmod-build-state.json` and annotate graph nodes with status
  (up-to-date, needs-rebuild, never-built)
- In ASCII output: prefix nodes with [✓], [!], [?] markers
- In DOT output: color nodes green/yellow/red
- In JSON output: add `status` and `last_build_time` fields
- Add `--critical-path` flag showing the longest dependency chain

**Files:** `crates/cmod-cli/src/commands/graph.rs`

## Step 5: Artifact Signing & Verification (RFC-0009)

Complete the security model. Currently `verify.rs` detects signature
presence but doesn't validate. Add real verification:

- Add `sign_artifact()` to cmod-security that creates a detached signature
  file (`.sig`) alongside cached artifacts using SSH key signing
- Add `verify_artifact_signature()` that validates `.sig` files
- Wire into cache store/restore: sign on store, verify on restore when
  `security.verify_checksums = true`
- Add `cmod verify --artifacts` flag to check all cached artifact signatures

**Files:** `crates/cmod-security/src/verify.rs`, `crates/cmod-cache/src/cache.rs`,
`crates/cmod-cli/src/commands/verify.rs`

## Step 6: Precompiled Module Packaging (RFC-0011 foundation)

Add ability to export/import precompiled module packages:

- `cmod package` command: creates a `.cmod-pkg.tar.gz` containing PCM/object
  files + metadata JSON (compiler version, target triple, source hash)
- `cmod install <package>` command: unpacks into local cache after verifying
  metadata compatibility with current toolchain
- Compatibility check: compiler + version + target + stdlib must match

**Files:** new `crates/cmod-cli/src/commands/package.rs`,
new `crates/cmod-cli/src/commands/install.rs`

## Step 7: Build Performance Profiling (RFC-0012 + RFC-0014)

Add build timing and profiling output:

- Record per-node compilation time in `BuildStats` (already has
  `incremental_skipped`, add `timings: Vec<(String, Duration)>`)
- Add `--timings` flag to build command that prints a timing report
- Add `cmod graph --timings` that annotates the graph with build durations
- Identify and display the critical path (slowest chain)

**Files:** `crates/cmod-build/src/runner.rs`, `crates/cmod-cli/src/commands/build.rs`,
`crates/cmod-cli/src/commands/graph.rs`

## Step 8: Integration Tests + Test Count Push

Add integration tests for all new Phase 5 features:

- compile_commands.json generation and format validation
- Dependency patch/override resolution
- Build hook execution (pre/post)
- Graph status coloring output
- Package export/import round-trip
- Build timing report output
- End-to-end workflow with patches + hooks + compile_commands

**Files:** `crates/cmod-cli/tests/cli_integration.rs`, unit tests in each module
