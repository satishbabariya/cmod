# Examples & Tutorials

cmod includes five example projects demonstrating common use cases. Each is a self-contained project in the `examples/` directory.

## Overview

| Example | Description | Key Concepts |
|---------|-------------|-------------|
| [hello](#hello--minimal-binary) | Minimal binary, no dependencies | Module interface, `cmod build`, `cmod run` |
| [library](#library--static-library-with-partitions) | Static lib with module partitions | `static-lib`, partitions, `export import :part` |
| [with-deps](#with-deps--git-dependencies) | Git dependencies (fmt + json) | Version constraints, branches, lockfiles |
| [workspace](#workspace--multi-member-monorepo) | Multi-member monorepo | `[workspace]`, inter-member path deps |
| [path-deps](#path-deps--local-path-dependencies) | Local path dependencies | `path = "libs/..."`, `cmod deps --tree` |

---

## hello — Minimal Binary

**Location:** `examples/hello/`

The simplest possible cmod project — a binary with no external dependencies.

### Project structure

```
hello/
├── cmod.toml
└── src/
    ├── lib.cppm      # Module interface
    └── main.cpp      # Entry point
```

### cmod.toml

```toml
[package]
name = "hello"
version = "0.1.0"
edition = "2023"
description = "Minimal cmod binary example"
authors = ["cmod contributors"]
license = "Apache-2.0"

[module]
name = "local.hello"
root = "src/lib.cppm"

[toolchain]
compiler = "clang"
cxx_standard = "20"

[build]
type = "binary"
```

### Module interface (src/lib.cppm)

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

### Entry point (src/main.cpp)

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
cd examples/hello
cmod build
cmod run
# Output: Hello, world!
```

---

## library — Static Library with Partitions

**Location:** `examples/library/`

Demonstrates a static library using module partitions to organize code.

### Project structure

```
library/
├── cmod.toml
└── src/
    ├── lib.cppm       # Primary module interface (re-exports partitions)
    ├── ops.cppm       # :ops partition — arithmetic operations
    └── stats.cppm     # :stats partition — statistical functions
```

### cmod.toml

```toml
[package]
name = "math-lib"
version = "0.1.0"
edition = "2023"
description = "Static library example with module partitions"
authors = ["cmod contributors"]
license = "Apache-2.0"

[module]
name = "local.math"
root = "src/lib.cppm"

[toolchain]
compiler = "clang"
cxx_standard = "20"

[build]
type = "static-lib"
```

### Primary interface (src/lib.cppm)

```cpp
export module local.math;

export import :ops;      # Re-export the ops partition
export import :stats;    # Re-export the stats partition
```

Consumers only need `import local.math;` to access everything from both partitions.

### Partition (src/ops.cppm)

```cpp
export module local.math:ops;

export namespace math {
    auto add(int a, int b) -> int { return a + b; }
    auto multiply(int a, int b) -> int { return a * b; }
}
```

### Build

```bash
cd examples/library
cmod build
# Produces: build/debug/libmath-lib.a
```

---

## with-deps — Git Dependencies

**Location:** `examples/with-deps/`

Shows how to use Git-hosted dependencies with version constraints and branch pinning.

### cmod.toml

```toml
[package]
name = "with-deps"
version = "0.1.0"
edition = "2023"
description = "Example using Git dependencies (fmt + json)"
authors = ["cmod contributors"]
license = "Apache-2.0"

[module]
name = "local.with_deps"
root = "src/lib.cppm"

[dependencies]
"github.com/satishbabariya/fmt-cmod" = "^0.1"
"github.com/satishbabariya/json-cmod" = { version = "^0.1", branch = "develop" }

[toolchain]
compiler = "clang"
cxx_standard = "20"

[build]
type = "binary"
```

### Key concepts

- **Simple version constraint:** `"^0.1"` — any compatible version in the 0.1.x range
- **Branch pinning:** `{ version = "^0.1", branch = "develop" }` — resolve from a specific branch
- **Lockfile:** After `cmod resolve`, `cmod.lock` pins exact commit hashes

### Workflow

```bash
cd examples/with-deps
cmod resolve          # Fetch deps, generate cmod.lock
cmod build            # Build with resolved dependencies
cmod build --locked   # Fail if lockfile is outdated (CI mode)
```

---

## workspace — Multi-Member Monorepo

**Location:** `examples/workspace/`

Demonstrates a workspace with three members that depend on each other.

### Project structure

```
workspace/
├── cmod.toml             # Workspace root
├── core/
│   ├── cmod.toml         # Core library
│   └── src/lib.cppm
├── utils/
│   ├── cmod.toml         # Utilities library
│   └── src/lib.cppm
└── app/
    ├── cmod.toml         # Application binary
    └── src/
        ├── lib.cppm
        └── main.cpp
```

### Root cmod.toml

```toml
[package]
name = "workspace-example"
version = "0.1.0"
edition = "2023"
description = "Multi-member workspace example"
authors = ["cmod contributors"]
license = "Apache-2.0"

[workspace]
members = ["core", "utils", "app"]
```

### Member dependencies

The `app` member depends on `core` and `utils` via path:

```toml
# app/cmod.toml
[dependencies]
core = { path = "../core" }
utils = { path = "../utils" }

[build]
type = "binary"
```

### Workflow

```bash
cd examples/workspace
cmod build                # Build all members
cmod workspace list       # List members
cmod run -p app           # Run the app member
```

---

## path-deps — Local Path Dependencies

**Location:** `examples/path-deps/`

Shows how to use local path dependencies for libraries within the same repository.

### Project structure

```
path-deps/
├── cmod.toml
├── src/
│   ├── lib.cppm
│   └── main.cpp
└── libs/
    ├── geometry/
    │   ├── cmod.toml
    │   └── src/lib.cppm
    └── colors/
        ├── cmod.toml
        └── src/lib.cppm
```

### cmod.toml

```toml
[package]
name = "path-deps"
version = "0.1.0"
edition = "2023"
description = "Example using local path dependencies"
authors = ["cmod contributors"]
license = "Apache-2.0"

[dependencies]
geometry = { path = "libs/geometry" }
colors = { path = "libs/colors" }

[toolchain]
compiler = "clang"
cxx_standard = "20"

[build]
type = "binary"
```

### Workflow

```bash
cd examples/path-deps
cmod build
cmod deps --tree          # Visualize the dependency tree
cmod run
```

---

## Running the Examples

All examples follow the same pattern:

```bash
cd examples/<name>
cmod resolve              # (if the project has dependencies)
cmod build
cmod run                  # (for binary projects)
```

For workspace examples, use `-p` to run a specific member:

```bash
cmod run -p <member-name>
```
