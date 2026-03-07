# Configuration Reference

cmod projects are configured through a `cmod.toml` manifest file at the project root. This document covers every available section and field.

## `[package]`

Required. Basic project metadata.

```toml
[package]
name = "my-project"          # Required. Project name (alphanumeric, _, -)
version = "1.0.0"            # Required. Semantic version (semver)
edition = "2023"             # Optional. C++ module edition year
description = "My library"   # Optional. Short description
authors = ["Jane <j@x.com>"] # Optional. List of authors
license = "MIT"              # Optional. SPDX license identifier
repository = "https://github.com/user/repo"  # Optional. Source repository
homepage = "https://example.com"             # Optional. Project homepage
```

**Validation rules:**
- `name` must contain only alphanumeric characters, `_`, and `-`
- `version` must be valid semver (e.g., `1.0.0`, `0.1.0-alpha.1`)

## `[module]`

Optional. Defines the C++20 module identity.

```toml
[module]
name = "com.github.user.my_math"   # Module name in reverse-domain format
root = "src/lib.cppm"               # Path to the primary module interface unit
```

**Module naming conventions:**
- Git-hosted modules use reverse-domain Git path: `github.fmtlib.fmt`
- Local-only modules use the `local.*` prefix: `local.hello`
- Reserved prefixes: `std.*` and `stdx.*` are reserved for the C++ standard

## `[dependencies]`

Git-native dependencies. Keys are Git URL paths; values are version constraints or detailed specifications.

```toml
[dependencies]
# Simple version constraint (key is the Git URL path)
"github.com/fmtlib/fmt" = "^10.2"

# Detailed dependency with options
"github.com/nlohmann/json" = { version = "^3.11", features = ["diagnostics"] }

# Pin to a specific branch
"github.com/acme/utils" = { version = "^1.0", branch = "stable" }

# Pin to an exact revision
"github.com/acme/core" = { rev = "a1b2c3d4e5f6" }

# Pin to a specific tag
"github.com/acme/core" = { tag = "v2.0.0" }

# Local path dependency
my_utils = { path = "./libs/utils" }

# Explicit Git URL (when key differs from URL)
math = { git = "https://github.com/acme/math.git", version = "^1.0" }

# Optional dependency (only included when feature is enabled)
simd_accel = { version = "^1.0", optional = true }

# Inherit from workspace
"github.com/fmtlib/fmt" = { workspace = true }

# Disable default features
"github.com/acme/lib" = { version = "^1.0", default_features = false, features = ["core"] }
```

**Dependency fields:**

| Field | Type | Description |
|-------|------|-------------|
| `version` | string | Semver constraint (e.g., `"^1.2"`, `"~1.0"`, `">=1.0,<2.0"`) |
| `git` | string | Explicit Git URL |
| `branch` | string | Git branch name |
| `rev` | string | Exact Git commit hash |
| `tag` | string | Git tag name |
| `path` | string | Local filesystem path |
| `features` | list | Features to enable |
| `optional` | bool | Only include when activated by a feature (default: `false`) |
| `default_features` | bool | Use the dependency's default features (default: `true`) |
| `workspace` | bool | Inherit version/details from `[workspace.dependencies]` (default: `false`) |

**Constraint:** A dependency cannot specify both `git` and `path`.

## `[dev-dependencies]`

Dependencies used only for testing. Same format as `[dependencies]`.

```toml
[dev-dependencies]
"github.com/catchorg/Catch2" = "^3.4"
```

## `[build-dependencies]`

Dependencies needed at build time (e.g., code generators). Same format as `[dependencies]`.

```toml
[build-dependencies]
"github.com/acme/codegen" = "^1.0"
```

## `[features]`

Feature flags for conditional compilation.

```toml
[features]
default = ["logging"]
logging = []
simd = ["simd_accel"]    # Enables the optional "simd_accel" dependency
full = ["logging", "simd"]
```

Enable features via CLI: `cmod build --features logging,simd`

## `[compat]`

Compatibility constraints.

```toml
[compat]
cpp = ">=20"                        # Minimum C++ standard
llvm = ">=17"                       # Minimum LLVM version
abi = "itanium"                     # ABI variant: "itanium" or "msvc"
platforms = ["linux", "macos"]      # Supported platforms
```

## `[toolchain]`

Compiler and build toolchain configuration.

```toml
[toolchain]
compiler = "clang"                          # "clang" (default), "gcc", or "msvc"
version = "18.1.0"                          # Compiler version requirement
cxx_standard = "20"                         # C++ standard: "20" or "23"
stdlib = "libc++"                           # Standard library: "libc++" or "libstdc++"
target = "aarch64-unknown-linux-gnu"        # Target triple for cross-compilation
sysroot = "/opt/aarch64-sysroot"            # Sysroot path for cross-compilation
```

## `[build]`

Build configuration.

```toml
[build]
type = "binary"          # "binary" (default), "static-lib", or "shared-lib"
optimization = "debug"   # "debug" (default), "release", "size", or "speed"
lto = false              # Link-time optimization (default: false)
parallel = true          # Parallel compilation (default: true)
incremental = true       # Incremental builds (default: true)
sources = ["src/"]       # Source directories (default: ["src"])
exclude = ["*_test.cc"]  # Glob patterns to exclude from source discovery
include_dirs = ["include/", "third_party/"]  # Additional include directories
extra_flags = ["-Wall", "-Wextra"]           # Extra compiler flags
```

### `[build.distributed]`

Distributed build configuration.

```toml
[build.distributed]
enabled = false                                      # Enable distributed builds
workers = ["https://w1.example.com:8443"]            # Worker endpoint URLs
scheduler = "least_loaded"                           # "least_loaded", "round_robin", "target_affinity"
auth_token_env = "CMOD_DISTRIBUTED_AUTH_TOKEN"       # Env var for auth token
task_timeout = 300                                   # Per-task timeout in seconds
```

## `[test]`

Test configuration.

```toml
[test]
framework = "catch2"                         # Test framework hint
test_patterns = ["tests/**/*.cpp"]           # File patterns for test discovery
exclude_patterns = ["tests/bench_*.cpp"]     # Patterns to exclude from tests
```

## `[cache]`

Build cache configuration.

```toml
[cache]
local_path = "~/.cache/cmod"            # Local cache directory (default: system cache dir)
shared_url = "https://cache.example.com" # Remote cache URL
ttl = "7d"                               # Cache entry time-to-live (e.g., "7d", "24h")
max_size = "1G"                          # Maximum cache size (e.g., "500M", "2G")
auth_token_env = "CMOD_CACHE_AUTH_TOKEN" # Env var for remote cache auth
timeout = 30                             # HTTP timeout in seconds (default: 30)
retries = 3                              # Retry attempts for remote operations (default: 3)
compression = true                       # Compress with zstd before upload (default: true)
```

## `[security]`

Security and signing configuration.

```toml
[security]
signing_key = "ABCD1234"                             # Signing key ID
signing_backend = "pgp"                              # "pgp", "ssh", or "sigstore"
verify_checksums = true                              # Verify content hashes on fetch
trusted_sources = ["github.com/*", "gitlab.com/org/*"]  # Trusted source patterns
signature_policy = "warn"                            # "none", "warn", or "require"
oidc_issuer = "https://accounts.google.com"          # OIDC issuer for Sigstore
certificate_identity = "user@example.com"            # Certificate identity for Sigstore
```

## `[publish]`

Publishing configuration for releases.

```toml
[publish]
registry = "https://registry.example.com"   # Target registry
include = ["src/**", "cmod.toml", "LICENSE"] # Files to include
exclude = ["tests/**", ".git"]              # Files to exclude
tags = ["v1.0.0", "latest"]                 # Release tags
```

## `[hooks]`

Build lifecycle hooks. Shell commands that run at specific points. A non-zero exit code fails the build.

```toml
[hooks]
pre-build = "echo 'Starting build...'"
post-build = "echo 'Build complete!'"
pre-test = "echo 'Running tests...'"
post-test = "echo 'Tests done.'"
pre-resolve = "echo 'Resolving deps...'"
pre-publish = "./scripts/validate-release.sh"
```

## `[workspace]`

Workspace (monorepo) configuration. See the [Workspaces guide](workspaces.md) for details.

```toml
[workspace]
name = "my-workspace"
version = "0.1.0"                # Unified version for all members (optional)
members = ["core", "utils", "app"]
exclude = ["experimental/*"]
resolver = "2"

[workspace.dependencies]
"github.com/fmtlib/fmt" = "^10.2"

[workspace.patch]
fmt = { path = "../my-local-fmt" }  # Override for local development
```

## `[metadata]`

Project metadata for discoverability.

```toml
[metadata]
category = "math"
keywords = ["linear-algebra", "simd"]
documentation = "https://docs.example.com"
readme = "README.md"

[metadata.links]
homepage = "https://example.com"
issues = "https://example.com/issues"
```

## `[ide]`

IDE integration settings.

```toml
[ide]
lsp_server = "auto"       # "auto", "on", or "off"
code_completion = true     # Enable code completion
diagnostics = true         # Enable real-time diagnostics
format_on_save = true      # Format files on save
```

## `[plugins]`

Plugin configuration. Each key is a plugin name.

```toml
[plugins.my-linter]
path = ".cmod/plugins/linter"
capabilities = ["lint", "diagnostics"]

[plugins.code-gen]
path = "/usr/local/lib/cmod-plugins/codegen"
capabilities = ["build-hook"]
```

## `[abi]`

ABI compatibility metadata for BMI distribution.

```toml
[abi]
version = "1.0"                          # ABI version
variant = "itanium"                      # "itanium" or "msvc"
stable = true                            # Stable ABI guarantee
min_cpp_standard = "20"                  # Minimum C++ standard for ABI compat
verified_platforms = ["x86_64-linux-gnu", "arm64-apple-darwin"]
breaking_changes = ["Removed foo::bar() in v1.0"]
```

## Target-Specific Dependencies

Use `cfg()` expressions to conditionally include dependencies by platform:

```toml
[target.'cfg(target_os = "linux")'.dependencies]
liburing = "^2.0"

[target.'cfg(windows)'.dependencies]
winapi = "^0.3"

[target.'cfg(all(target_os = "linux", target_arch = "x86_64"))'.dependencies]
intel-intrinsics = "^1.0"

[target.'cfg(any(target_os = "linux", target_os = "macos"))'.dependencies]
posix-utils = "^1.0"

[target.'cfg(not(windows))'.dependencies]
unix-utils = "^1.0"
```

**Supported `cfg()` predicates:**
- `target_os` — `"linux"`, `"macos"`, `"darwin"`, `"windows"`, `"freebsd"`, etc.
- `target_arch` — `"x86_64"`, `"aarch64"`, etc.
- `target_family` — `"unix"` or `"windows"`
- `target_env` — `"gnu"`, `"msvc"`, `"musl"`, etc.
- `unix` — shorthand for `target_family = "unix"`
- `windows` — shorthand for `target_family = "windows"`

**Combinators:** `all(...)`, `any(...)`, `not(...)`

You can also use a literal target triple as the key:

```toml
[target.'x86_64-unknown-linux-gnu'.dependencies]
linux-x64-specific = "^1.0"
```
