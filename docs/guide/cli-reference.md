# CLI Reference

Complete reference for all `cmod` commands, flags, and options.

## Global Flags

These flags can be used with any command:

| Flag | Short | Description |
|------|-------|-------------|
| `--locked` | | Fail if the lockfile is outdated (strict mode) |
| `--offline` | | Disable network access |
| `--verbose` | `-v` | Enable verbose output |
| `--quiet` | `-q` | Suppress all status output |
| `--target <TRIPLE>` | | Override the target triple (e.g., `aarch64-unknown-linux-gnu`) |
| `--features <LIST>` | | Enable specific features (comma-separated) |
| `--no-default-features` | | Disable default features |
| `--no-cache` | | Skip build cache |
| `--untrusted` | | Skip TOFU trust verification for dependencies |

`--verbose` and `--quiet` are mutually exclusive.

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | Build failure (compilation error, missing compiler, scan failure) |
| `2` | Resolution error (dependency not found, version conflict, lockfile issues) |
| `3` | Security violation |

---

## Core Workflow

### `cmod init`

Initialize a new module or workspace.

```
cmod init [--workspace] [--name <NAME>]
```

| Option | Description |
|--------|-------------|
| `--workspace` | Initialize as a workspace instead of a single module |
| `--name <NAME>` | Project name (defaults to the current directory name) |

Creates a `cmod.toml` manifest and a `src/` directory with a placeholder module interface.

**Examples:**

```bash
cmod init                      # Initialize in current directory
cmod init --name my-project    # Initialize with a specific name
cmod init --workspace          # Initialize a workspace
```

### `cmod build`

Build the current module or workspace.

```
cmod build [OPTIONS]
```

| Option | Short | Description |
|--------|-------|-------------|
| `--release` | | Build in release mode |
| `--jobs <N>` | `-j` | Maximum parallel compilation jobs (0 = auto, default: 0) |
| `--force` | | Force rebuild, ignoring incremental state |
| `--remote-cache <URL>` | | Remote cache URL (overrides `[cache].shared_url`) |
| `--no-hooks` | | Skip pre-build and post-build hooks |
| `--verify` | | Verify lockfile integrity and package hashes before building |
| `--timings` | | Display per-module compile timings |
| `--distributed` | | Enable distributed build across remote workers |
| `--workers <URLS>` | | Worker endpoints for distributed builds (comma-separated) |

**Examples:**

```bash
cmod build                     # Debug build
cmod build --release           # Release build
cmod build -j 8                # Limit to 8 parallel jobs
cmod build --force             # Force full rebuild
cmod build --release --timings # Release build with timing info
```

### `cmod test`

Build and run module tests. See the [Testing guide](testing.md) for full details.

```
cmod test [TESTNAME] [OPTIONS]
```

| Argument/Option | Short | Description |
|-----------------|-------|-------------|
| `[TESTNAME]` | | Positional filter: run only tests whose name contains this string |
| `--release` | | Build tests in release mode |
| `--filter <GLOB>` | | Filter test files by glob pattern |
| `--jobs <N>` | `-j` | Number of test binaries to run in parallel (0 = auto) |
| `--no-fail-fast` | | Continue running after a failure |
| `--timeout <SECS>` | | Per-test timeout in seconds (overrides `[test].timeout`) |
| `--package <NAME>` | `-p` | Run tests for a specific workspace member |
| `--coverage` | | Instrument builds for code coverage |
| `--sanitize <LIST>` | | Enable sanitizers: `address`, `undefined`, `thread`, `memory` |
| `--format <FMT>` | | Output format: `human` (default), `json`, `junit`, `tap` |

**Examples:**

```bash
cmod test                      # Run tests in debug mode
cmod test --release            # Run tests in release mode
cmod test math                 # Run only tests matching "math"
cmod test --filter "test_*"    # Filter by glob pattern
cmod test -j 4                 # Run 4 tests in parallel
cmod test --no-fail-fast       # Run all tests even if some fail
cmod test --timeout 60         # 60-second per-test timeout
cmod test -p my-lib            # Test a specific workspace member
cmod test --coverage           # Generate code coverage report
cmod test --sanitize address   # Run with AddressSanitizer
cmod test --format junit       # Output as JUnit XML
```

### `cmod run`

Build and run the project binary.

```
cmod run [OPTIONS] [-- <ARGS>...]
```

| Option | Short | Description |
|--------|-------|-------------|
| `--release` | | Build in release mode |
| `--package <NAME>` | `-p` | Run a specific workspace member |
| `-- <ARGS>` | | Arguments to pass to the binary |

**Examples:**

```bash
cmod run                       # Build and run in debug mode
cmod run --release             # Build and run in release mode
cmod run -- --config app.toml  # Pass arguments to the binary
cmod run -p my-app             # Run a specific workspace member
```

### `cmod clean`

Remove build artifacts from the `build/` directory.

```
cmod clean
```

---

## Dependency Management

### `cmod add`

Add a dependency to the project.

```
cmod add <DEP> [OPTIONS]
```

| Argument/Option | Description |
|-----------------|-------------|
| `<DEP>` | Dependency specifier (e.g., `github.com/fmtlib/fmt` or `github.com/fmtlib/fmt@^10.2`) |
| `--git <URL>` | Explicit Git URL (if different from the key) |
| `--branch <NAME>` | Pin to a Git branch |
| `--rev <HASH>` | Pin to an exact Git revision |
| `--path <PATH>` | Add as a local path dependency |
| `--features <LIST>` | Features to enable (comma-separated) |

**Examples:**

```bash
# Add with version constraint
cmod add "github.com/fmtlib/fmt@^10.2"

# Add pinned to a branch
cmod add "github.com/acme/json" --branch develop

# Add a local path dependency
cmod add my-utils --path ./libs/utils

# Add with features enabled
cmod add "github.com/acme/lib@^1.0" --features simd,logging
```

### `cmod remove`

Remove a dependency from `cmod.toml`.

```
cmod remove <NAME>
```

**Example:**

```bash
cmod remove "github.com/fmtlib/fmt"
```

### `cmod resolve`

Resolve dependencies and generate or update the lockfile (`cmod.lock`).

```
cmod resolve
```

Fetches dependency metadata from Git, solves version constraints, and writes `cmod.lock`.

### `cmod update`

Update dependencies to newer versions within their constraints.

```
cmod update [NAME] [--patch]
```

| Argument/Option | Description |
|-----------------|-------------|
| `[NAME]` | Update only a specific dependency |
| `--patch` | Only allow patch-level updates (e.g., `1.2.3` to `1.2.4`) |

**Examples:**

```bash
cmod update                    # Update all dependencies
cmod update "github.com/fmtlib/fmt"  # Update a specific dep
cmod update --patch            # Conservative updates only
```

### `cmod deps`

Inspect the dependency graph.

```
cmod deps [--tree] [--why <NAME>] [--conflicts]
```

| Option | Description |
|--------|-------------|
| `--tree` | Display as a tree |
| `--why <NAME>` | Explain why a specific dependency is included |
| `--conflicts` | Show transitive dependency conflicts |

**Examples:**

```bash
cmod deps                      # List dependencies
cmod deps --tree               # Show dependency tree
cmod deps --why json           # Why is json included?
cmod deps --conflicts          # Show version conflicts
```

### `cmod tidy`

Detect and optionally remove unused dependencies.

```
cmod tidy [--apply]
```

| Option | Description |
|--------|-------------|
| `--apply` | Actually remove unused dependencies (default is dry run) |

**Examples:**

```bash
cmod tidy                      # Show what would be removed
cmod tidy --apply              # Remove unused dependencies
```

### `cmod vendor`

Vendor dependencies for offline builds.

```
cmod vendor [--sync]
```

| Option | Description |
|--------|-------------|
| `--sync` | Re-synchronize vendored deps with the lockfile |

### `cmod search`

Search for modules by name.

```
cmod search <QUERY> [--local-only]
```

| Argument/Option | Description |
|-----------------|-------------|
| `<QUERY>` | Search query (substring match) |
| `--local-only` | Only search local dependencies and lockfile |

---

## Build Tools

### `cmod graph`

Visualize the module dependency graph.

```
cmod graph [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `--format <FORMAT>` | Output format: `ascii` (default), `dot`, `json` |
| `--filter <PATTERN>` | Filter modules matching a pattern |
| `--status` | Show build status annotations (up-to-date, needs-rebuild, never-built) |
| `--critical-path` | Highlight the critical path (longest compile chain) |
| `--timing` | Annotate nodes with build timing (color-coded by duration) |

**Examples:**

```bash
cmod graph                         # ASCII graph
cmod graph --format dot            # DOT format for Graphviz
cmod graph --format json           # JSON format
cmod graph --status --timing       # Show build status and timing
cmod graph --format dot | dot -Tpng -o graph.png  # Generate image
```

### `cmod explain`

Explain why a specific module would be rebuilt.

```
cmod explain <MODULE>
```

**Example:**

```bash
cmod explain local.math            # Why would local.math rebuild?
```

### `cmod compile-commands`

Generate `compile_commands.json` for IDE integration (clangd, VS Code, etc.).

```
cmod compile-commands
```

### `cmod plan`

Output the build plan as JSON without executing it.

```
cmod plan
```

### `cmod emit-cmake`

Export a `CMakeLists.txt` for interop with CMake-based projects.

```
cmod emit-cmake
```

### `cmod lint`

Lint C++ source files for common issues using clang-tidy.

```
cmod lint
```

### `cmod fmt`

Format C++ source files using clang-format.

```
cmod fmt [--check]
```

| Option | Description |
|--------|-------------|
| `--check` | Check formatting without modifying files |

---

## Cache Management

### `cmod cache status`

Show cache status and size.

```
cmod cache status
```

### `cmod cache status-json`

Show cache status as JSON (machine-readable).

```
cmod cache status-json
```

### `cmod cache clean`

Clear the local cache.

```
cmod cache clean
```

### `cmod cache gc`

Run garbage collection — evict old and oversized entries.

```
cmod cache gc
```

### `cmod cache push`

Push local cache entries to the remote cache.

```
cmod cache push
```

### `cmod cache pull`

Pull cache entries from the remote cache.

```
cmod cache pull
```

### `cmod cache inspect`

Inspect a specific cache entry.

```
cmod cache inspect <MODULE> <KEY>
```

### `cmod cache export`

Export a cached module as a BMI package.

```
cmod cache export <MODULE> <KEY> -o <OUTPUT>
```

| Argument/Option | Description |
|-----------------|-------------|
| `<MODULE>` | Module name |
| `<KEY>` | Cache key (hex) |
| `-o <OUTPUT>` | Output directory |

### `cmod cache import`

Import a BMI package into the local cache.

```
cmod cache import <PATH>
```

---

## Security & Verification

### `cmod verify`

Verify integrity and security of dependencies.

```
cmod verify [--signatures]
```

| Option | Description |
|--------|-------------|
| `--signatures` | Also check commit signatures (GPG/SSH) |

### `cmod audit`

Audit dependencies for security and quality issues.

```
cmod audit
```

---

## Project Management

### `cmod status`

Show a project status overview including manifest info, dependency count, and build state.

```
cmod status
```

### `cmod check`

Validate module naming, identity, and structure rules.

```
cmod check
```

### `cmod toolchain show`

Show the active toolchain configuration.

```
cmod toolchain show
```

### `cmod toolchain check`

Validate that the required toolchain is available.

```
cmod toolchain check
```

---

## Workspace Management

### `cmod workspace list`

List workspace members.

```
cmod workspace list
```

### `cmod workspace add`

Add a new member to the workspace.

```
cmod workspace add <NAME>
```

### `cmod workspace remove`

Remove a member from the workspace.

```
cmod workspace remove <NAME>
```

---

## Publishing

### `cmod publish`

Publish a release by creating a Git tag.

```
cmod publish [OPTIONS]
```

| Option | Description |
|--------|-------------|
| `--dry-run` | Show what would happen without making changes |
| `--push` | Push the tag to origin after creation |
| `--sign` | Sign the release tag |
| `--no-sign` | Do not sign (overrides `[security]` config; conflicts with `--sign`) |
| `--skip-governance` | Skip governance policy validation |

**Examples:**

```bash
cmod publish --dry-run         # Preview the release
cmod publish --push --sign     # Publish, push, and sign
```

### `cmod sbom`

Generate a Software Bill of Materials (SBOM).

```
cmod sbom [-o <FILE>]
```

| Option | Short | Description |
|--------|-------|-------------|
| `--output <FILE>` | `-o` | Output file path (prints to stdout if not specified) |

---

## Plugin Management

### `cmod plugin list`

List discovered plugins.

```
cmod plugin list
```

### `cmod plugin run`

Run a plugin by name.

```
cmod plugin run <NAME>
```

---

## LSP Server

### `cmod lsp`

Start the LSP server for IDE integration.

```
cmod lsp
```
