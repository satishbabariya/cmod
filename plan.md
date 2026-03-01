# Next Implementation Steps — Phase 6

## Current State (380 tests, 26+ CLI commands)

**Implemented through Phase 5:** Core RFCs (0001-0008, 0019, UNIFIED), plus:
- Incremental builds, remote cache, cfg() dep filtering, SBOM, publish
- compile_commands.json, TrustDb TOFU integration, build hooks
- Graph status coloring, lockfile integrity/--verify, cache eviction (gc)
- Per-node build timings, conflict diagnostics (--why, --conflicts)
- 11 Phase 5 integration tests

**Remaining RFC gaps:** 0006 (tidy), 0008 (multi-target), 0011 (BMI distribution),
0012 (critical path), 0013 (distributed stubs), 0014 (critical path viz),
0016 (LSP), 0017 (ABI metadata), 0018 (plugins), 0019 (workspace deps).

---

## Steps

### Step 1: `cmod tidy` — remove unused deps (RFC-0006)
- Scan source files for imports, compare against manifest deps
- Report unused deps, `--apply` to remove them

### Step 2: Workspace `{ workspace = true }` dep inheritance (RFC-0019)
- Support `workspace = true` in DetailedDependency
- Resolve from `[workspace.dependencies]` at manifest load time
- `workspace.version` enforcement across members

### Step 3: Multi-target build (RFC-0008)
- `--target` accepts comma-separated triples
- Per-target build dirs and cache isolation
- Sequential build per target

### Step 4: Critical path analysis (RFC-0012 + RFC-0014)
- Compute critical path from node timings
- `cmod graph --critical-path` highlights longest path
- `cmod build --timings` reports critical path

### Step 5: Plugin framework (RFC-0018)
- `[plugins]` manifest section
- `cmod plugin list/run` commands
- Subprocess execution with JSON protocol

### Step 6: LSP server foundation (RFC-0016)
- `cmod lsp` command with basic LSP protocol
- Module-aware document symbols from build graph

### Step 7: BMI distribution metadata (RFC-0011)
- BmiMetadata struct (toolchain, target, hash, signature)
- `cmod cache export/import` commands for BMI packages

### Step 8: ABI compatibility metadata (RFC-0017)
- `[abi]` section in cmod.toml with version tracking
- Warn on ABI-incompatible updates during resolve

### Step 9: Integration tests + commit
