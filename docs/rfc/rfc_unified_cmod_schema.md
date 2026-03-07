# RFC-UNIFIED: Unified cmod.toml Schema Specification

## Status
Draft (Consolidation RFC)

## Summary
This document consolidates all `cmod.toml` schema definitions across RFC-0001, RFC-0006, RFC-0017, and RFC-0019 into a single unified specification.

## Complete Schema

```toml
# === Core Package Identity ===
[package]
name = "my_math"                    # Human-readable name
version = "1.4.2"                   # SemVer: MAJOR.MINOR.PATCH
edition = "2030"                    # C++ edition/year
description = "Math utilities"
authors = ["Jane Doe <jane@example.com>"]
license = "MIT"
repository = "https://github.com/user/my_math"
homepage = "https://github.com/user/my_math"

# === Module Configuration ===
[module]
name = "com.github.user.my_math"   # Full module name (RFC-0002)
root = "src/lib.cppm"               # Primary module interface

# === Dependencies ===
[dependencies]
github.com/fmtlib/fmt = "^10.2"     # Semantic version range
github.com/acme/math = { version = ">=1.0.0", features = ["simd"] }
local_utils = { path = "./utils", version = "0.1.0" }

[dev-dependencies]
github.com/catchorg/Catch2 = "^3.4"

[build-dependencies]
github.com/llvm/llvm-tblgen = "17.0"

# === Features ===
[features]
default = ["core", "format"]
core = []
format = ["core"]
simd = []
benchmarks = []

# === Compatibility Constraints ===
[compat]
cpp = ">=20"                        # Minimum C++ standard
llvm = ">=17"                       # Minimum LLVM version
abi = "itanium"                     # ABI: itanium, msvc
platforms = ["x86_64-linux-gnu", "arm64-macos"]  # Supported platforms

# === Toolchain Configuration ===
[toolchain]
compiler = "clang"                  # clang, gcc, msvc
version = "18.1.0"                   # Exact version or constraint
cxx_standard = "23"                  # 20, 23, 26
stdlib = "libc++"                   # libc++, libstdc++
target = "x86_64-unknown-linux-gnu" # Target triple

# === Build Configuration ===
[build]
type = "binary"                     # binary, static-lib, shared-lib
optimization = "release"            # debug, release, size, speed
lto = true                          # Link-time optimization
parallel = true                     # Parallel compilation
incremental = true                  # Incremental builds
sources = ["src/"]                  # Source directories (default: ["src"])
exclude = ["*_test.cc", "test/**"]  # Glob patterns to exclude from source discovery

# === Testing Configuration ===
[test]
framework = "catch2"                # catch2, gtest, custom
test_patterns = ["tests/**/*.cpp"]
exclude_patterns = ["tests/integration/**"]

# === Publishing Configuration ===
[publish]
registry = "https://crates.cmod.io" # Future registry URL
include = ["src/**", "README.md", "LICENSE"]
exclude = ["tests/**", "examples/**", ".git/**"]
tags = ["math", "utilities"]

# === Workspace Configuration (RFC-0019) ===
[workspace]
resolver = "2"                      # Dependency resolver version
members = ["core", "utils", "cli"]
exclude = ["examples/*"]

[workspace.dependencies]
github.com/fmtlib/fmt = "^10.2"     # Workspace-wide dependencies

# === Metadata (RFC-0017) ===
[metadata]
category = "Mathematics"
keywords = ["math", "utilities", "algebra"]
readme = "README.md"
documentation = "https://docs.example.com/my_math"

[metadata.links]
crates_io = "https://crates.io/crates/my_math"
documentation = "https://docs.rs/my_math"
repository = "https://github.com/user/my_math"

# === Security Configuration (RFC-0009) ===
[security]
sign_commits = true                 # Require signed commits
verify_checksums = true              # Verify artifact checksums
trusted_sources = ["github.com", "gitlab.com"]  # Trusted domains
signing_key = "ABCDEF1234567890"    # Signing key (GPG ID, SSH key path, etc.)
signing_backend = "pgp"             # "pgp", "ssh", or "sigstore"
signature_policy = "warn"           # "none", "warn", or "require"
oidc_issuer = "https://accounts.google.com"  # OIDC issuer for Sigstore keyless signing
certificate_identity = "user@example.com"    # Certificate identity for Sigstore verification

# === Cache Configuration ===
[cache]
local_path = "~/.cache/cmod"         # Local cache directory
shared_url = "https://cache.cmod.io" # Optional shared cache
ttl = "7d"                          # Cache time-to-live

# === IDE Integration (RFC-0010) ===
[ide]
lsp_server = true                   # Enable LSP server
code_completion = true              # Enable code completion
diagnostics = true                  # Enable compile diagnostics
format_on_save = true              # Auto-format on save

# === Plugin Configuration (RFC-0018) ===
[plugins]
formatter = "rust-fmt"              # Code formatter plugin
linter = "clang-tidy"               # Linting plugin
profiler = "perf"                   # Profiling plugin
```

## Schema Sections

### Required Sections
- `[package]`: Basic package identity (RFC-0001)
- `[module]`: Module name and root (RFC-0002)

### Common Optional Sections
- `[dependencies]`: External dependencies (RFC-0006)
- `[toolchain]`: Compiler and build settings (RFC-0001, RFC-0008)
- `[build]`: Build configuration (RFC-0004, RFC-0007)
- `[compat]`: Compatibility constraints (RFC-0006)

### Advanced Optional Sections
- `[workspace]`: Multi-project workspaces (RFC-0019)
- `[features]`: Feature flags (RFC-0017)
- `[metadata]`: Package metadata (RFC-0017)
- `[security]`: Security settings (RFC-0009)
- `[cache]`: Cache configuration (RFC-0005, RFC-0007)
- `[ide]`: IDE integration (RFC-0010)
- `[plugins]`: Plugin system (RFC-0018)

## Validation Rules

1. **Module name must match `[module].name`**: The `export module` declaration must match the configured module name
2. **Version must be valid SemVer**: Follows `MAJOR.MINOR.PATCH` format (RFC-0006)
3. **Dependencies must be resolvable**: All external dependencies must be valid Git URLs or local paths
4. **Compatibility constraints must be satisfiable**: Toolchain requirements must be compatible
5. **Workspace configuration consistency**: Workspaces must have compatible member configurations

## Migration Path

Existing configurations will be automatically upgraded:
- Old `[package]` sections remain valid
- New sections are added as needed
- Deprecated fields generate warnings but continue to work

## Implementation Notes

- The schema is extensible - new sections can be added in future RFCs
- All sections are optional except for basic package identity
- Configuration is hierarchical - workspace settings can override defaults
- Validation happens early in the build process to provide clear error messages