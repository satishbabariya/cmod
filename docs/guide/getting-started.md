# Getting Started with cmod

This guide walks you through installing cmod, creating your first C++20 module project, and building it.

## Prerequisites

Before using cmod, ensure you have:

- **Rust toolchain** (1.74+) — for building cmod from source
- **LLVM/Clang 17+** — the default compiler backend; `clang++` and `clang-scan-deps` must be on your `PATH`
- **Git** — used for dependency fetching (Git is the registry)
- **A C++20-capable compiler** — Clang 17+ is recommended

## Installation

Build and install cmod from source:

```bash
git clone https://github.com/nickelpack/cmod.git
cd cmod
cargo install --path crates/cmod-cli
```

Verify the installation:

```bash
cmod --version
```

## Create Your First Project

### Initialize a new module

```bash
mkdir hello && cd hello
cmod init --name hello
```

This creates:

```
hello/
├── cmod.toml       # Project manifest
└── src/
    └── lib.cppm    # Module interface unit (placeholder)
```

The generated `cmod.toml` looks like this:

```toml
[package]
name = "hello"
version = "0.1.0"
edition = "2023"

[module]
name = "local.hello"
root = "src/lib.cppm"

[compat]
cpp = ">=20"

[toolchain]
compiler = "clang"
cxx_standard = "20"

[build]
type = "binary"
optimization = "debug"
lto = false
parallel = true
incremental = true
```

### Write the module interface

Edit `src/lib.cppm`:

```cpp
module;

#include <string>
#include <string_view>

export module local.hello;

export namespace hello {

inline auto greet(std::string_view name) -> std::string {
    return std::string("Hello, ") + std::string(name) + "!";
}

} // namespace hello
```

### Write the entry point

Create `src/main.cpp`:

```cpp
#include <iostream>

import local.hello;

int main() {
    std::cout << hello::greet("world") << std::endl;
    return 0;
}
```

### Build and run

```bash
cmod build            # Compile in debug mode
cmod run              # Build and run the binary
```

You should see:

```
Hello, world!
```

### Build in release mode

```bash
cmod build --release
cmod run --release
```

## Create a Library

To create a static library instead of a binary:

```bash
mkdir math-lib && cd math-lib
cmod init --name math-lib
```

Edit `cmod.toml` and change the build type:

```toml
[build]
type = "static-lib"
```

Build types are: `binary`, `static-lib`, `shared-lib`.

## Add Dependencies

cmod uses Git URLs as package identifiers. There is no central registry — dependencies are fetched directly from Git repositories.

```bash
# Add a dependency with a version constraint
cmod add "github.com/satishbabariya/fmt-cmod@^0.1"

# Add a dependency pinned to a branch
cmod add "github.com/satishbabariya/json-cmod" --branch develop
```

After adding dependencies, resolve and lock them:

```bash
cmod resolve
```

This fetches the dependencies and generates `cmod.lock`, which pins exact commit hashes for reproducible builds. Commit `cmod.lock` to version control.

## Project Structure

A typical cmod project looks like this:

```
my-project/
├── cmod.toml           # Project manifest
├── cmod.lock           # Lockfile (generated, committed to VCS)
├── src/
│   ├── lib.cppm        # Module interface unit
│   ├── main.cpp        # Entry point (for binaries)
│   └── ...             # Implementation files
├── tests/              # Test files
└── build/              # Build artifacts (gitignored)
    ├── debug/
    └── release/
```

## Key Concepts

- **Git is the registry.** Dependencies are identified by Git URL (e.g., `github.com/fmtlib/fmt`). No central package server.
- **Modules are first-class.** cmod is designed for C++20 modules, not headers.
- **Lockfiles are mandatory.** `cmod.lock` pins exact commits and toolchain info for reproducible builds.
- **Module names use reverse-domain format.** For example, `github.com/fmtlib/fmt` becomes module `github.fmtlib.fmt`. Local modules use the `local.*` prefix.

## Next Steps

- [Configuration Reference](configuration.md) — all `cmod.toml` options
- [CLI Reference](cli-reference.md) — every command and flag
- [Dependencies](dependencies.md) — version constraints, lockfiles, vendoring
- [C++20 Modules](modules.md) — module naming, partitions, build graph
- [Building](building.md) — build profiles, cross-compilation, hooks
- [Examples](examples.md) — walkthrough of example projects
