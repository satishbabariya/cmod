# Next Implementation Steps — Phase 7

## Current State (408 tests, 30+ CLI commands)

**Implemented through Phase 6:** Core RFCs (0001-0008, 0019, UNIFIED), plus:
- Incremental builds, remote cache, cfg() dep filtering, SBOM, publish
- compile_commands.json, TrustDb TOFU, build hooks, graph status coloring
- Lockfile integrity/--verify, cache eviction (gc), per-node build timings
- Conflict diagnostics (--why, --conflicts), cmod tidy, cmod check
- Plugin framework (list/run), BMI distribution (export/import)
- ABI metadata ([abi] section), IDE config ([ide] section)
- Critical path analysis, build plan JSON output, CMake export
- LTO support wired from manifest

---

## Phase 7: Closing actionable RFC gaps

### Step 1: Lockfile integrity hash (RFC-0003/0009) — Small
- Compute SHA-256 integrity hash over all lockfile package entries at save time
- Populate `Lockfile.integrity` field in `resolver.rs` when saving
- `verify_integrity()` actually checks the hash (not just None-pass)
- Populate `LockedPackage.toolchain` from resolved manifest toolchain

### Step 2: Optimization level mapping & test patterns (RFC-UNIFIED) — Small
- Map `[build] optimization = "size"` → `-Os`, `"speed"` → `-O3` in ClangBackend
- Wire `[test] test_patterns` and `exclude_patterns` into `cmod test` source discovery
- Fix SBOM `chrono_now()` to use actual system time

### Step 3: Compat constraint enforcement (RFC-0006) — Medium
- During `Resolver::resolve()`, check candidate deps against project `[compat]` constraints
- Warn/error when dep requires C++ standard above project toolchain
- Enforce `[compat] platforms` filtering during resolution

### Step 4: Feature → compiler flags (RFC-0017) — Small
- Translate activated features into `-DCMOD_FEATURE_<NAME>=1` compiler flags
- Pass feature flags through `ClangBackend::common_flags()`
- Wire `[plugins]` manifest entries into plugin discovery (not just `.cmod/plugins/`)

### Step 5: Hook completion & vendor integration (RFC-0001/0018) — Medium
- Invoke `pre_test`, `post_test` hooks in `cmod test`
- Invoke `pre_resolve` hook in `cmod resolve` (if defined)
- Make `cmod build` detect `vendor/` directory and use vendored sources
- Consume `vendor/config.toml` in resolver for offline builds

### Step 6: Remote cache improvements (RFC-0005) — Medium
- Implement real `cmod cache pull` using lockfile cache keys
- Verify artifact hash after remote cache download
- Auto-evict after store in builder (respect `max_size`)

### Step 7: Security policy enforcement (RFC-0009) — Small
- Consult `[security] signature_policy` during build/resolve
- When policy = "require", fail if any dep lacks a verified signature
- When policy = "warn", emit warnings for unsigned deps

### Step 8: Persist node timings in BuildState (RFC-0012/0014) — Small
- Save per-node compile times to `.cmod-build-state.json`
- Use real timings (not unit weights) for `cmod graph --critical-path`
- Show critical path timing in `cmod build --timings` output

### Step 9: Integration tests + commit
- Test lockfile integrity roundtrip
- Test optimization level flags
- Test compat constraint enforcement
- Test feature compiler flags
- Test hook invocations in test command
- Test security policy warnings
