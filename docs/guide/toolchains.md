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

cmod defaults to Clang and uses `clang-scan-deps` for module dependency discovery (see [The Module Graph](modules.md#the-module-graph)); GCC 14+ scans with `g++` itself. If no scanner is available, cmod reads imports from the source text instead.

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

## Compiler Detection

cmod resolves which binaries to invoke in a fixed order — there is no
config-file search or automatic Homebrew probing:

| Binary | 1. Environment variable | 2. Lookup | 3. Fallback |
|--------|------------------------|-----------|-------------|
| C++ compiler | `CXX` | first `clang++` on `PATH` | literal `clang++` (OS lookup at spawn) |
| Dependency scanner | `SCAN_DEPS` | the compiler's own: next to it, named like it (`clang++-20` → `clang-scan-deps-20`), or next to the binary it links to (Debian's `/usr/bin/clang++` → `/usr/lib/llvm-18/bin/`); then the first `clang-scan-deps` on `PATH` | literal `clang-scan-deps` |

The scanner is looked up beside the compiler so that it is the same LLVM: with only `clang-scan-deps-18` on `PATH` (Debian and Ubuntu packages), `CXX=clang++-20` scans with `clang-scan-deps-20`, and a plain `clang++` with LLVM 18's own scanner.

The environment variables take absolute precedence and accept full paths:

```bash
CXX=/opt/homebrew/opt/llvm@18/bin/clang++ \
SCAN_DEPS=/opt/homebrew/opt/llvm@18/bin/clang-scan-deps \
cmod build
```

`cmod toolchain show` prints the resolved configuration (compiler, standard,
target) and the compiler a build runs; `cmod toolchain check` runs it (see
[Validate toolchain](#validate-toolchain)).

### macOS note

Apple's Xcode `clang++` (what a bare `clang++` resolves to) does not support
C++20 modules the way cmod drives them — builds fail with errors like
`unknown type name 'module'`. Install LLVM via Homebrew and make it win one
of the two detection steps:

```bash
brew install llvm@18

# Option A: put it first on PATH (affects everything in the shell)
export PATH="/opt/homebrew/opt/llvm@18/bin:$PATH"

# Option B: pin just cmod's binaries (per-project .envrc works well)
export CXX=/opt/homebrew/opt/llvm@18/bin/clang++
export SCAN_DEPS=/opt/homebrew/opt/llvm@18/bin/clang-scan-deps
```

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

Runs the compiler a build would use (`CXX`, or the one found as above) and reports its path and version, then:

- fails if it does not run, or if its version does not satisfy `[toolchain] version`;
- warns if it cannot build C++20 modules (Apple's clang, GCC before 14);
- for Clang, reports the dependency scanner, or warns that none was found and imports will be read from the source text.

### Compiler version

```toml
[toolchain]
compiler = "clang"
version = "18"        # any 18.x
# version = "18.1.0"  # 18.1.0 or a later 18.x
# version = "=18.1.8" # exactly 18.1.8
# version = ">=17, <20"
```

`version` is a constraint read as Cargo reads one. `cmod build` warns when the compiler does not satisfy it; `cmod toolchain check` fails.

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
