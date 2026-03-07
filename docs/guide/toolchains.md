# Toolchains & Cross-Compilation

cmod manages compiler toolchains for consistent, reproducible builds. This guide covers toolchain configuration, supported compilers, and cross-compilation.

## Toolchain Configuration

Configure the toolchain in `cmod.toml`:

```toml
[toolchain]
compiler = "clang"                          # Compiler backend
version = "18.1.0"                          # Required compiler version
cxx_standard = "20"                         # C++ standard
stdlib = "libc++"                           # Standard library
target = "x86_64-unknown-linux-gnu"         # Target triple
sysroot = "/opt/sysroot"                    # Sysroot for cross-compilation
```

All fields are optional. Defaults are sensible for most projects.

## Supported Compilers

| Compiler | Value | Binary | Description |
|----------|-------|--------|-------------|
| Clang | `"clang"` | `clang++` | Default. LLVM/Clang — full C++20 module support |
| GCC | `"gcc"` | `g++` | GNU Compiler Collection |
| MSVC | `"msvc"` | `cl` | Microsoft Visual C++ |

cmod defaults to Clang and uses `clang-scan-deps` for module dependency discovery.

## C++ Standard

```toml
[toolchain]
cxx_standard = "20"    # C++20 (default)
# cxx_standard = "23"  # C++23
```

## Standard Library

```toml
[toolchain]
stdlib = "libc++"      # LLVM's libc++ (common on macOS, recommended with Clang)
# stdlib = "libstdc++"  # GNU's libstdc++ (default on Linux with GCC)
```

The standard library choice affects ABI compatibility and cache keys.

## Toolchain Commands

### Show active toolchain

```bash
cmod toolchain show
```

Displays the resolved toolchain configuration: compiler, version, C++ standard, stdlib, target triple, and sysroot.

### Validate toolchain

```bash
cmod toolchain check
```

Verifies that the required compiler is available on your `PATH` and can execute successfully.

## Cross-Compilation

### Setting the target

In `cmod.toml`:

```toml
[toolchain]
compiler = "clang"
target = "aarch64-unknown-linux-gnu"
sysroot = "/opt/aarch64-sysroot"
```

Or from the CLI:

```bash
cmod build --target aarch64-unknown-linux-gnu
```

The CLI `--target` flag overrides the manifest setting.

### Target Triples

Target triples follow the format: `<arch>-<vendor>-<os>-<env>`

| Triple | Platform |
|--------|----------|
| `x86_64-unknown-linux-gnu` | Linux x86_64 with glibc |
| `x86_64-unknown-linux-musl` | Linux x86_64 with musl |
| `aarch64-unknown-linux-gnu` | Linux ARM64 |
| `x86_64-apple-darwin` | macOS Intel |
| `arm64-apple-darwin` | macOS Apple Silicon |
| `x86_64-pc-windows-msvc` | Windows x86_64 with MSVC |

### Host target detection

cmod automatically detects the host target triple. Use `cmod toolchain show` to see it.

### Cross-compilation requirements

For cross-compilation, you need:

1. A cross-compiler targeting the desired platform
2. A sysroot with headers and libraries for the target
3. The `sysroot` field set in `[toolchain]`

### Cache isolation

Each target triple gets its own cache namespace. The cache key includes the full toolchain tuple:

```
<compiler>-<version>-std<standard>-<stdlib>-<target>
```

This ensures that artifacts for different targets are never mixed.

## Compatibility Constraints

Use `[compat]` to declare compatibility requirements:

```toml
[compat]
cpp = ">=20"                            # Minimum C++ standard
llvm = ">=17"                           # Minimum LLVM version
abi = "itanium"                         # ABI variant: "itanium" or "msvc"
platforms = ["linux", "macos"]          # Supported platforms
```

This helps consumers know if your module is compatible with their toolchain.

## ABI Configuration

For libraries distributing precompiled BMIs, declare ABI metadata:

```toml
[abi]
version = "1.0"
variant = "itanium"                     # "itanium" or "msvc"
stable = true                           # Provides a stable ABI guarantee
min_cpp_standard = "20"
verified_platforms = ["x86_64-unknown-linux-gnu", "arm64-apple-darwin"]
breaking_changes = ["Removed deprecated foo::bar() API"]
```
