# C++20 Modules in cmod

cmod is built around C++20 modules as the primary compilation model. This guide explains how modules work in cmod, naming conventions, partitions, and the build graph.

## What Are C++20 Modules?

C++20 modules replace the traditional `#include` model with a structured import system:

- **Faster builds** — module interfaces are compiled once into Binary Module Interfaces (BMIs/PCMs), not re-parsed per translation unit
- **Better isolation** — modules have explicit export boundaries; internal details are hidden
- **No header guards** — no `#pragma once` or include guards needed
- **Cleaner dependencies** — imports are explicit and ordered

## Module Units

cmod recognizes four kinds of module units:

### Module Interface Unit

The primary interface for a module. This is what consumers `import`.

```cpp
// src/lib.cppm
export module local.hello;

export namespace hello {
    auto greet(std::string_view name) -> std::string;
}
```

The file path is specified in `cmod.toml` as `module.root`:

```toml
[module]
name = "local.hello"
root = "src/lib.cppm"
```

### Implementation Unit

Contains the implementation of exported declarations. Not importable by consumers.

```cpp
// src/hello.cpp
module local.hello;

namespace hello {
    auto greet(std::string_view name) -> std::string {
        return std::string("Hello, ") + std::string(name) + "!";
    }
}
```

### Partition Unit

Modules can be split into partitions for internal organization. Partitions use the `:partition` syntax.

```cpp
// src/ops.cppm — partition interface
export module local.math:ops;

export namespace math {
    auto add(int a, int b) -> int;
    auto multiply(int a, int b) -> int;
}
```

```cpp
// src/stats.cppm — another partition
export module local.math:stats;

export namespace math {
    auto mean(const std::vector<double>& data) -> double;
}
```

The primary module interface re-exports partitions:

```cpp
// src/lib.cppm — primary interface
export module local.math;

export import :ops;
export import :stats;
```

Consumers only need `import local.math;` to access everything.

### Legacy Unit

Non-module translation units (traditional `.cpp` files with `#include`). cmod handles these as regular object file compilations.

## Module Naming

### Reverse-Domain Git Path Format

cmod derives module names from Git URLs using a reverse-domain format:

| Git URL | Module Name |
|---------|-------------|
| `https://github.com/fmtlib/fmt` | `github.fmtlib.fmt` |
| `https://gitlab.com/org/infra/log` | `gitlab.org.infra.log` |
| `git@github.com:user/repo.git` | `github.user.repo` |

This ensures globally unique module names without a central authority.

### Local Modules

Modules that are not published to a Git repository use the `local.*` prefix:

```toml
[module]
name = "local.my_project"
root = "src/lib.cppm"
```

`cmod init` generates local module names automatically (e.g., `local.hello` for a project named "hello").

### Reserved Prefixes

The following prefixes are reserved and cannot be used:

- `std.*` — reserved for the C++ standard library modules
- `stdx.*` — reserved for future C++ standard extensions

## The Module Graph

cmod builds a **Directed Acyclic Graph (DAG)** of module dependencies before any compilation begins:

1. **Source scanning** — `clang-scan-deps` discovers `import` statements in source files
2. **Name resolution** — imported module names are matched to source files and dependencies
3. **DAG construction** — dependency edges are created between module units
4. **Topological sort** — the graph is sorted to determine correct compilation order
5. **Parallel execution** — independent modules are compiled concurrently

### Build Order

Modules must be compiled in dependency order:

1. Partitions are compiled first (`:ops`, `:stats`)
2. The primary interface is compiled next (depends on partitions)
3. Implementation units are compiled (depend on the interface)
4. The final link step combines all object files

### Viewing the Graph

```bash
cmod graph                          # ASCII visualization
cmod graph --format dot             # DOT format for Graphviz
cmod graph --format json            # JSON for programmatic use
cmod graph --status                 # Annotate with build status
cmod graph --critical-path          # Highlight the longest chain
```

### Explaining Rebuilds

```bash
cmod explain local.math
```

Shows why a module would be rebuilt (source changed, dependency BMI changed, compiler flags changed, etc.).

## File Conventions

cmod discovers source files in the `src/` directory by default (configurable via `[build].sources`):

| Extension | Usage |
|-----------|-------|
| `.cppm` | Module interface units and partition interfaces |
| `.cpp` | Implementation units and entry points (`main.cpp`) |
| `.cc`, `.cxx` | Alternative implementation file extensions |

## Importing Dependencies

After adding a dependency, import its module by name:

```toml
# cmod.toml
[dependencies]
"github.com/satishbabariya/fmt-cmod" = "^0.1"
```

```cpp
// src/main.cpp
import github.satishbabariya.fmt_cmod;

int main() {
    // Use the imported module
}
```

The module name is derived from the Git URL using the reverse-domain format.

## Module Interface with Global Module Fragment

Use the global module fragment for `#include` directives that must precede the module declaration:

```cpp
module;                          // Start of global module fragment

#include <string>                // Standard library headers
#include <string_view>
#include <vector>

export module local.hello;       // Module declaration

// Everything after this is part of the module
export namespace hello {
    auto greet(std::string_view name) -> std::string;
}
```

The global module fragment (`module;` ... `export module ...;`) is where you place `#include` directives for headers that haven't been modularized yet.
