# Getting Started with cmod: From Zero to Building in 60 Seconds

*A hands-on guide to creating, building, and managing C++20 projects with cmod.*

---

The best way to understand cmod is to use it. This guide walks you through creating a project, adding dependencies, building, testing, and managing a workspace — all in a few minutes.

## Install cmod

cmod is written in Rust and builds with Cargo:

```bash
git clone https://github.com/nickshouse/cmod.git
cd cmod
cargo build --release
```

Add the binary to your PATH:

```bash
export PATH="$PWD/target/release:$PATH"
```

**Requirements:**
- Rust 1.70+ (for building cmod)
- Clang 18+ with `clang-scan-deps` (for building C++ projects)
- Git 2.25+

## Create Your First Project

```bash
cmod init hello
cd hello
```

This creates a clean project structure:

```
hello/
├── cmod.toml
└── src/
    └── main.cpp
```

And a minimal `cmod.toml`:

```toml
[package]
name = "hello"
version = "0.1.0"
edition = "2024"

[module]
name = "com.github.yourname.hello"
root = "src/main.cppm"

[build]
type = "binary"
```

Build and run it:

```bash
cmod build
cmod run
```

That's it. No CMakeLists.txt, no Makefile, no build directory configuration.

## Add a Dependency

Let's add the `fmt` library:

```bash
cmod add github.com/fmtlib/fmt@10.0
```

This updates your `cmod.toml`:

```toml
[dependencies]
"github.com/fmtlib/fmt" = ">=10.0"
```

Now resolve and build:

```bash
cmod resolve   # Fetch deps, solve versions, generate lockfile
cmod build     # Build everything
```

cmod clones the `fmt` repository, resolves the best matching version from Git tags, records the exact commit hash in `cmod.lock`, and compiles it as a module dependency.

## Understand the Lockfile

After resolving, you'll see a `cmod.lock` file:

```
cmod.lock
```

This file pins every dependency to an exact Git commit. Check it into version control. When a teammate runs `cmod build --locked`, they'll get the exact same dependency versions you used.

```bash
# In CI, always use --locked to ensure reproducibility
cmod build --locked
```

## Use the Dependency Graph

See what you're depending on:

```bash
# Tree view
cmod deps --tree

# Visual graph (outputs DOT format for Graphviz)
cmod graph --format dot > deps.dot
dot -Tpng deps.dot -o deps.png

# JSON for programmatic use
cmod graph --format json
```

Want to know why a specific module is in your build?

```bash
cmod deps --why fmt
```

## Run Tests

cmod has built-in test support:

```bash
cmod test
```

For release-mode tests:

```bash
cmod test --release
```

## Build Profiles

```bash
cmod build              # Debug build (default)
cmod build --release    # Optimized release build
cmod build --jobs 8     # Parallel compilation with 8 jobs
```

## Work Offline

Already resolved your dependencies? You can build without network access:

```bash
cmod build --offline
```

This uses the locally cached repository clones and artifacts. Useful for airplanes, restricted networks, and CI environments where you want to ensure no unexpected network calls.

## Set Up a Workspace

For projects with multiple modules (monorepo style):

```bash
cmod init my_workspace --workspace
cd my_workspace
```

This creates a workspace-level `cmod.toml`:

```toml
[workspace]
members = []
```

Add members:

```bash
cmod workspace add core
cmod workspace add app
cmod workspace add tests
```

Your structure becomes:

```
my_workspace/
├── cmod.toml          # Workspace manifest
├── core/
│   ├── cmod.toml
│   └── src/
├── app/
│   ├── cmod.toml
│   └── src/
└── tests/
    ├── cmod.toml
    └── src/
```

Dependencies are resolved once across the entire workspace. Build artifacts (BMIs, object files) are shared between members. One `cmod build` from the root builds everything in the correct order.

## Manage Your Cache

cmod caches compiled artifacts locally:

```bash
cmod cache status    # See what's cached
cmod cache clean     # Clear the cache
cmod cache gc        # Garbage-collect old entries
```

Cache keys are computed from source content (SHA-256), compiler version, flags, and dependencies. If nothing changed, nothing rebuilds.

## Verify Integrity

Check that your dependencies haven't been tampered with:

```bash
cmod verify              # Hash verification
cmod verify --signatures # Signature verification (when available)
```

Generate a Software Bill of Materials:

```bash
cmod sbom --output sbom.json
```

Audit your dependency tree:

```bash
cmod audit
```

## IDE Integration

Generate `compile_commands.json` for clangd, CLion, VS Code, or any LSP-compatible editor:

```bash
cmod compile-commands
```

This gives you full code intelligence — autocompletion, go-to-definition, diagnostics — without any IDE-specific configuration.

## Lint and Format

cmod integrates with clang-tidy and clang-format:

```bash
cmod lint           # Run clang-tidy
cmod fmt            # Format with clang-format
cmod fmt --check    # Check formatting without modifying
```

## Common Workflows

### Starting a new project
```bash
cmod init my_project
cd my_project
cmod add github.com/fmtlib/fmt@10.0
cmod add github.com/nlohmann/json@3.11
cmod resolve
cmod build
```

### CI pipeline
```bash
cmod resolve
cmod build --locked --release
cmod test --release
cmod verify
cmod sbom --output sbom.json
```

### Daily development
```bash
cmod build          # Incremental build
cmod test           # Run tests
cmod run -- arg1    # Run with arguments
cmod deps --tree    # Check dependency tree
```

### Updating dependencies
```bash
cmod update                  # Update all deps to latest compatible
cmod update fmt              # Update a specific dependency
cmod update --patch          # Only patch-level updates
cmod tidy --apply            # Remove unused dependencies
```

## The Full Command Reference

Run `cmod --help` to see all available commands, or `cmod <command> --help` for detailed usage of any specific command. With 30+ commands covering the entire development lifecycle, cmod is a single tool for the complete C++ workflow.

## What's Next

Now that you're up and running, explore:

- The [examples/](https://github.com/nickshouse/cmod/tree/main/examples) directory for real-world project templates
- The [docs/](https://github.com/nickshouse/cmod/tree/main/docs) directory for design specifications and RFCs
- The [CONTRIBUTING.md](https://github.com/nickshouse/cmod/blob/main/CONTRIBUTING.md) guide if you want to help build the future of C++ tooling

---

*cmod is open source under Apache-2.0. Questions? Issues? Feature requests? We're on [GitHub](https://github.com/nickshouse/cmod).*
