# Dependency Management

cmod uses a Git-native dependency model. This guide covers how dependencies work, version constraints, lockfiles, and related workflows.

## How Dependencies Work

In cmod, **Git is the registry**. There is no central package server. Dependencies are identified by their Git URL path:

```toml
[dependencies]
"github.com/fmtlib/fmt" = "^10.2"
```

When you run `cmod resolve`, cmod:

1. Clones or fetches the Git repository
2. Lists available tags as version numbers
3. Selects the best version matching your constraint
4. Records the exact commit hash in `cmod.lock`

## Version Constraints

cmod uses [Semantic Versioning](https://semver.org/) (semver). Version constraints specify which versions are acceptable.

### Constraint Syntax

| Syntax | Meaning | Example |
|--------|---------|---------|
| `^1.2` | `>=1.2.0, <2.0.0` | Caret (default) — compatible updates |
| `^0.2` | `>=0.2.0, <0.3.0` | Pre-1.0: caret is more conservative |
| `~1.2` | `>=1.2.0, <1.3.0` | Tilde — patch-level updates only |
| `>=1.0,<2.0` | Explicit range | Range — exact bounds |
| `=1.4.2` | Exactly `1.4.2` | Exact — pins to one version |
| `*` | Any version | Wildcard |

A leading `v` prefix is stripped automatically (e.g., `v1.2.0` is treated as `1.2.0`).

### Choosing a Constraint

- Use **caret** (`^`) for most dependencies — allows minor and patch updates
- Use **tilde** (`~`) when you need API stability within a minor version
- Use **exact** (`=`) when you must pin to a specific release
- For pre-1.0 dependencies, caret is conservative: `^0.2` means `>=0.2.0, <0.3.0`

## Adding Dependencies

### By URL with version

```bash
cmod add "github.com/fmtlib/fmt@^10.2"
```

This adds to `cmod.toml`:

```toml
[dependencies]
"github.com/fmtlib/fmt" = "^10.2"
```

### By branch

```bash
cmod add "github.com/acme/utils" --branch develop
```

Produces:

```toml
[dependencies]
"github.com/acme/utils" = { branch = "develop" }
```

### By exact revision

```bash
cmod add "github.com/acme/core" --rev a1b2c3d4
```

### By path (local dependency)

```bash
cmod add my-utils --path ./libs/utils
```

Produces:

```toml
[dependencies]
my-utils = { path = "./libs/utils" }
```

### With features

```bash
cmod add "github.com/acme/lib@^1.0" --features simd,logging
```

## Lockfiles

### What is `cmod.lock`?

`cmod.lock` records the exact resolved state of all dependencies:

- Package name, version, and source type
- Git repository URL and exact commit hash
- Content hash of resolved sources
- Toolchain information (compiler, version, stdlib, target)
- Dependency relationships
- Activated features

### When to commit `cmod.lock`

**Always commit `cmod.lock` to version control.** It ensures that all collaborators and CI systems build with the exact same dependencies.

### `--locked` mode

Use `--locked` to enforce that the lockfile is up-to-date:

```bash
cmod build --locked
cmod resolve --locked
```

If the lockfile is missing or outdated, the command fails with exit code 2 instead of silently updating. This is recommended for CI pipelines.

### Lockfile integrity

The lockfile includes an optional `integrity` hash (SHA-256) that covers all package data. Use `--verify` during builds to check it:

```bash
cmod build --verify
```

## Updating Dependencies

### Update all dependencies

```bash
cmod update
```

Re-resolves all dependencies within their constraints, updating `cmod.lock` to the latest compatible versions.

### Update a specific dependency

```bash
cmod update "github.com/fmtlib/fmt"
```

### Conservative updates

```bash
cmod update --patch
```

Only allows patch-level updates (e.g., `1.2.3` to `1.2.4`), preventing minor version bumps.

## Inspecting Dependencies

### List dependencies

```bash
cmod deps
```

### Dependency tree

```bash
cmod deps --tree
```

Shows the full transitive dependency tree.

### Why is a dependency included?

```bash
cmod deps --why "github.com/fmtlib/fmt"
```

Traces the dependency chain to explain why a package is required.

### Show conflicts

```bash
cmod deps --conflicts
```

Displays transitive dependency version conflicts.

## Removing Dependencies

### Remove from manifest

```bash
cmod remove "github.com/fmtlib/fmt"
```

### Detect and remove unused

```bash
cmod tidy           # Dry run — show what would be removed
cmod tidy --apply   # Actually remove unused dependencies
```

## Vendoring

Vendor dependencies into your repository for offline or hermetic builds:

```bash
cmod vendor
```

To re-synchronize vendored deps with the lockfile:

```bash
cmod vendor --sync
```

## Feature Flags

Features allow optional functionality that consumers can enable:

```toml
# In your cmod.toml
[features]
default = ["logging"]
logging = []
simd = ["simd_accel"]

[dependencies]
simd_accel = { version = "^1.0", optional = true }
```

Enable features at build time:

```bash
cmod build --features simd
cmod build --no-default-features --features simd
```

Enable features on a dependency:

```toml
[dependencies]
"github.com/acme/lib" = { version = "^1.0", features = ["simd"] }
```

Disable a dependency's default features:

```toml
[dependencies]
"github.com/acme/lib" = { version = "^1.0", default_features = false, features = ["core"] }
```

## Target-Specific Dependencies

Include dependencies only for certain platforms using `cfg()` expressions:

```toml
[target.'cfg(target_os = "linux")'.dependencies]
liburing = "^2.0"

[target.'cfg(windows)'.dependencies]
winapi = "^0.3"

[target.'cfg(not(windows))'.dependencies]
unix-utils = "^1.0"
```

See the [Configuration Reference](configuration.md#target-specific-dependencies) for the full `cfg()` syntax.

## Workspace Dependencies

In a workspace, shared dependencies can be defined once and inherited by members:

```toml
# Root cmod.toml
[workspace.dependencies]
"github.com/fmtlib/fmt" = "^10.2"
```

```toml
# Member cmod.toml
[dependencies]
"github.com/fmtlib/fmt" = { workspace = true }
```

See the [Workspaces guide](workspaces.md) for details.

## Searching for Modules

Search for modules by name:

```bash
cmod search fmt                # Search broadly
cmod search fmt --local-only   # Search only local deps and lockfile
```
