# Building & Compilation

This guide covers build profiles, parallel compilation, cross-compilation, build hooks, and IDE integration.

## Build Profiles

cmod supports two build profiles:

| Profile | Flag | Optimization | Use Case |
|---------|------|-------------|----------|
| Debug | (default) | `debug` — no optimization, debug symbols | Development |
| Release | `--release` | `release` — full optimization | Production |

```bash
cmod build              # Debug build → build/debug/
cmod build --release    # Release build → build/release/
```

## Build Types

Set the build output type in `cmod.toml`:

```toml
[build]
type = "binary"       # Compile to an executable (default)
# type = "static-lib" # Compile to a static library (.a)
# type = "shared-lib" # Compile to a shared library (.so/.dylib)
```

A shared library is compiled with `-fPIC` (Clang and GCC, except for Windows targets), as are the dependencies linked into it. Members of a workspace that are shared libraries get `-fPIC` for their own sources; a static member linked into one needs `extra_flags = ["-fPIC"]`.

## Optimization Levels

```toml
[build]
optimization = "debug"    # No optimization, debug info (default)
# optimization = "release" # -O2 equivalent
# optimization = "size"    # -Os — optimize for binary size
# optimization = "speed"   # -O3 — optimize for speed
```

## Parallel Compilation

By default, cmod compiles independent modules in parallel:

```toml
[build]
parallel = true     # Enable parallel compilation (default)
```

Control the number of parallel jobs from the CLI:

```bash
cmod build -j 8     # Limit to 8 parallel jobs
cmod build -j 1     # Sequential build
cmod build -j 0     # Auto-detect (default)
```

## Incremental Builds

cmod tracks changes and only recompiles what's necessary:

```toml
[build]
incremental = true  # Enable incremental builds (default)
```

A module is rebuilt when any of these change:
- Source file content
- Content of any header it includes, as reported by the compiler (touching a header without changing it rebuilds nothing)
- Imported BMIs, from this package or from dependencies, including ones rebuilt earlier in the same build
- Compiler version or flags
- Target triple
- Standard library

Dependencies are incremental too. A link is skipped when the objects, dependency archives and flags it reads are unchanged and its output exists. Objects and BMIs left in `build/<profile>/obj` and `pcm` by deleted sources or by a checkout in another directory are removed before each build.

Force a full rebuild:

```bash
cmod build --force
```

### Understanding Rebuilds

See what a build would do, and why, without building:

```bash
cmod build --dry-run        # every step: rebuild (with the reason) or up-to-date
cmod explain local.math     # just one module
```

A dry run only looks: it runs no hooks, never resolves or writes `cmod.lock`, never fetches or removes a dependency checkout (one missing or not at its locked commit is reported as "not checked out at the locked commit"), and leaves build outputs and build state untouched. Without a `cmod.lock` it still examines path dependencies. An importer of a module that will be rebuilt is reported as rebuilt; the real build may skip it if the new BMI is byte-identical.

## Link-Time Optimization (LTO)

Enable LTO for whole-program optimization:

```toml
[build]
lto = true          # Default: false
```

LTO is most useful for release builds and can significantly reduce binary size and improve performance at the cost of longer link times.

## Source Directories

By default, cmod discovers source files in `src/`. Override this:

```toml
[build]
sources = ["Jolt/", "extra/src/"]    # Custom source directories
exclude = ["*_test.cc", "test/**"]   # Exclude patterns
```

If `sources` is empty or not set, cmod defaults to `["src"]`.

## Include Directories and Flags

```toml
[build]
include_dirs = ["include/", "third_party/headers/"]
extra_flags = ["-Wall", "-Wextra", "-Werror"]
```

## Build Hooks

Run shell commands at specific points in the build lifecycle:

```toml
[hooks]
pre-build = "echo 'Starting build...'"
post-build = "echo 'Build complete!'"
pre-test = "./scripts/setup-test-env.sh"
post-test = "./scripts/cleanup-test-env.sh"
pre-resolve = "echo 'Resolving dependencies...'"
pre-publish = "./scripts/validate-release.sh"
```

- Hooks run in the project root directory
- A non-zero exit code fails the build
- Skip hooks with `cmod build --no-hooks`

## Cross-Compilation

### Configure the target

Set the target triple in `cmod.toml`:

```toml
[toolchain]
compiler = "clang"
target = "aarch64-unknown-linux-gnu"
sysroot = "/opt/aarch64-sysroot"
```

Or override from the CLI:

```bash
cmod build --target aarch64-unknown-linux-gnu
```

### Common target triples

| Triple | Platform |
|--------|----------|
| `x86_64-unknown-linux-gnu` | Linux x86_64 (glibc) |
| `aarch64-unknown-linux-gnu` | Linux ARM64 |
| `x86_64-apple-darwin` | macOS x86_64 |
| `arm64-apple-darwin` | macOS Apple Silicon |
| `x86_64-pc-windows-msvc` | Windows x86_64 |

See the [Toolchains guide](toolchains.md) for more details on cross-compilation setup.

## Build Timings

Display per-module compile timings to identify bottlenecks:

```bash
cmod build --timings
```

## Build Graph Visualization

Visualize the module dependency graph:

```bash
cmod graph                             # ASCII visualization
cmod graph --format dot                # DOT format for Graphviz
cmod graph --status                    # Show build status annotations
cmod graph --critical-path             # Highlight the critical path
cmod graph --timing                    # Annotate with build timing
```

Generate a graph image:

```bash
cmod graph --format dot | dot -Tpng -o graph.png
```

## Build Plan

Export the build plan as JSON without executing it:

```bash
cmod plan
```

This outputs the full build DAG with node kinds (interface, implementation, object, link), dependencies, and commands.

## IDE Integration

### compile_commands.json

Generate a compilation database for clangd, VS Code, and other tools:

```bash
cmod compile-commands
```

This creates `compile_commands.json` in the project root.

### CMake Interop

Export a `CMakeLists.txt` for projects that need CMake integration:

```bash
cmod emit-cmake
```

### LSP Server

Start the built-in LSP server:

```bash
cmod lsp
```

## Distributed Builds

For large projects, cmod supports distributing compilation across remote workers:

```toml
[build.distributed]
enabled = true
workers = ["https://worker1.example.com:8443", "https://worker2.example.com:8443"]
scheduler = "least_loaded"
auth_token_env = "CMOD_DISTRIBUTED_AUTH_TOKEN"
task_timeout = 300
```

Or via CLI flags:

```bash
cmod build --distributed --workers https://w1.example.com,https://w2.example.com
```

## Build Output

Build artifacts are placed in the `build/` directory:

```
build/
├── debug/           # Debug profile artifacts
│   ├── *.pcm        # Precompiled module files (BMIs)
│   ├── *.o          # Object files
│   └── <binary>     # Final executable or library
├── release/         # Release profile artifacts
└── deps/            # Fetched dependency sources
```

Clean all build artifacts:

```bash
cmod clean
```
