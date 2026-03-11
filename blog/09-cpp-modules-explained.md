# C++20 Modules Explained for Practical Developers

*A no-nonsense guide to C++20 modules: what they are, how they differ from headers, and why they change everything about C++ builds.*

---

C++20 modules are the biggest change to C++ compilation since the language was standardized in 1998. They replace the textual `#include` mechanism with a proper module system that's faster, safer, and more explicit. Yet adoption remains slow, partly because the tooling hasn't kept up.

This guide cuts through the confusion.

## The Problem with Headers

When you write `#include <vector>`, the preprocessor literally copies the contents of that header file into your source file. For a typical standard library header, that's thousands of lines. For a project that includes the same header in 100 translation units, that's the same code parsed 100 times.

The consequences:
- **Slow compilation** — redundant parsing
- **Macro pollution** — macros leak across boundaries
- **Include order matters** — different orderings can produce different results
- **No encapsulation** — everything is visible
- **ODR violations** — same entity defined differently in different TUs

## Key Concepts

### Module Interface Units (.cppm)

```cpp
export module com.github.user.mylib;

export int public_function();    // Visible to consumers
int internal_helper();            // Private to the module
```

The `export` keyword is explicit. Only what you mark is visible.

### Module Implementation Units (.cpp)

```cpp
module com.github.user.mylib;

int public_function() {
    return internal_helper() * 2;
}
```

### Module Partitions

Large modules split into logical pieces:

```cpp
// math:ops.cppm
export module math:ops;
export int add(int a, int b);

// math.cppm — primary interface
export module math;
export import :ops;
```

### Binary Module Interfaces (BMIs)

Compiled once, imported by consumers. Much faster than re-parsing text headers.

## The Build Order Problem

Modules introduce ordering requirements: if `main.cpp` imports `math`, then `math.cppm` must be compiled first. The build system must discover and respect the module dependency graph.

**This is exactly what cmod does.** It uses `clang-scan-deps` for automatic discovery, constructs a DAG, and compiles in the correct order.

## Module Naming in cmod

Reverse-domain notation from the Git URL:

```
github.com/fmtlib/fmt  →  com.github.fmtlib.fmt
```

No naming collisions globally.

## A Complete Example

```cpp
// src/mylib.cppm
export module com.github.user.mylib;
export namespace mylib {
    int add(int a, int b);
}

// src/mylib.cpp
module com.github.user.mylib;
namespace mylib {
    int add(int a, int b) { return a + b; }
}
```

```toml
# cmod.toml
[package]
name = "mylib"
version = "0.1.0"

[module]
name = "com.github.user.mylib"
root = "src/mylib.cppm"

[build]
type = "library"
```

```bash
cmod build && cmod test
```

## When to Adopt

- **New projects:** Use modules from the start
- **Active projects with tests:** Migrate incrementally
- **Legacy without tests:** Wait for test coverage first

The ecosystem is moving toward modules. Starting now means you're ahead.

[Get started with cmod →](https://github.com/satishbabariya/cmod)
