# Migrating a CMake Project to cmod

*A step-by-step walkthrough of converting a real-world CMake project with external dependencies into a clean cmod project.*

---

You have a working CMake project. It builds. Tests pass. But the `CMakeLists.txt` is 400 lines long, `find_package` calls break on different systems, and nobody fully understands the build configuration.

This guide walks through migrating step by step.

## Step 1: Convert Headers to Module Interfaces

Before (`include/myapp/processor.h`):
```cpp
#pragma once
#include <string>
namespace myapp {
    struct ProcessResult { std::string output; int status; };
    ProcessResult process(const std::string& input);
}
```

After (`src/myapp.cppm`):
```cpp
export module com.github.user.myapp;
import std;
export namespace myapp {
    struct ProcessResult { std::string output; int status; };
    ProcessResult process(const std::string& input);
}
```

## Step 2: Create cmod.toml

```toml
[package]
name = "myapp"
version = "1.0.0"

[module]
name = "com.github.user.myapp"
root = "src/myapp.cppm"

[dependencies]
"github.com/fmtlib/fmt" = ">=10.0"
"github.com/nlohmann/json" = "^3.11"

[build]
type = "binary"
```

Compare this with a 30+ line CMakeLists.txt.

## Step 3: Restructure Source Tree

The `include/` directory is gone (modules define interfaces in `.cppm` files). The `cmake/` directory is gone (cmod handles resolution).

## Step 4: Update Source Files

Replace `#include` with `import`, remove include guards, use `module` declarations.

## Step 5: Resolve and Build

```bash
cmod resolve
cmod build
cmod run
cmod test
```

## Step 6: Set Up CI

Replace multi-step CMake CI with:
```bash
cmod build --locked --release
cmod test --locked --release
cmod verify
```

## Common Challenges

- **Libraries without module support:** Use header units as a bridge
- **Macro-heavy code:** Replace macros with `constexpr`/`consteval`
- **Build time:** First build slightly slower, incremental builds significantly faster

## Result

400-line CMakeLists.txt → 20-line cmod.toml. Dependencies from Git, not system packages. Lockfile for reproducibility. Faster incremental builds.

[Get started with cmod →](https://github.com/satishbabariya/cmod)
