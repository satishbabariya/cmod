# Workspaces & Monorepos

cmod supports workspaces for managing multiple related modules in a single repository (monorepo). This guide covers workspace setup, shared dependencies, and multi-member builds.

## What Are Workspaces?

A workspace is a collection of modules (members) managed together:

- **Shared lockfile** — all members share a single `cmod.lock` at the workspace root
- **Shared dependencies** — common dependencies are defined once and inherited
- **Unified builds** — `cmod build` at the root builds all members
- **Path-based references** — members reference each other via local paths

## Setting Up a Workspace

### Initialize a workspace

```bash
mkdir my-workspace && cd my-workspace
cmod init --workspace --name my-workspace
```

This creates a `cmod.toml` with a `[workspace]` section:

```toml
[package]
name = "my-workspace"
version = "0.1.0"
edition = "2023"

[workspace]
name = "my-workspace"
members = []
resolver = "2"
```

### Add members

```bash
cmod workspace add core
cmod workspace add utils
cmod workspace add app
```

Each command creates a subdirectory with its own `cmod.toml` and `src/` directory, and adds it to the workspace members list:

```toml
[workspace]
members = ["core", "utils", "app"]
```

`cmod init` in a subdirectory does the same with the full package template: the new package is added to the members of the workspace it is in (unless a member pattern such as `"libs/*"` already matches it, or `exclude` covers it), and gets no git repository of its own when the workspace has one.

```bash
mkdir -p libs/geometry && cd libs/geometry
cmod init --name geometry   # members = [..., "libs/geometry"]
```

### Project structure

```
my-workspace/
├── cmod.toml              # Workspace manifest
├── cmod.lock              # Shared lockfile
├── core/
│   ├── cmod.toml          # Member manifest
│   └── src/
│       └── lib.cppm
├── utils/
│   ├── cmod.toml
│   └── src/
│       └── lib.cppm
└── app/
    ├── cmod.toml
    └── src/
        ├── lib.cppm
        └── main.cpp
```

## Workspace Configuration

### `[workspace]` section

```toml
[workspace]
name = "my-workspace"            # Workspace name
version = "0.1.0"                # Unified version (optional, applied to all members)
members = ["core", "utils", "app"]  # Member directories (globs allowed: "libs/*")
exclude = ["experimental/*"]     # Directories to exclude, with what is inside them
resolver = "2"                   # Dependency resolver version
```

A member is named by its `[package] name`, wherever its directory is: `libs/util` with `name = "util"` is the member `util`, as `cmod run -p`, `cmod test -p` and `cmod workspace list` show it; a member with no name is named after its directory. Two members may not share a name, and a name may not contain `/`, `\` or `:`.

### Shared dependencies

Define dependencies once at the workspace level:

```toml
# Root cmod.toml
[workspace.dependencies]
"github.com/fmtlib/fmt" = "^10.2"
"github.com/nlohmann/json" = "^3.11"
```

Members inherit these by setting `workspace = true`:

```toml
# core/cmod.toml
[dependencies]
"github.com/fmtlib/fmt" = { workspace = true }
```

This ensures all members use the same version of shared dependencies.

### Dependency patches

Override dependencies with local paths during development:

```toml
[workspace.patch]
fmt = { path = "../my-local-fmt" }
```

## Inter-Member Dependencies

Members reference each other using path dependencies:

```toml
# app/cmod.toml
[dependencies]
core = { path = "../core" }
utils = { path = "../utils" }
```

A path dependency on a member's directory makes that member build first, and its modules importable, whatever the dependency is keyed.

## Working with Workspaces

### List members

```bash
cmod workspace list
```

### Build all members

```bash
cmod build                    # Build all members in debug mode
cmod build --release          # Build all in release mode
```

### Run a specific member

```bash
cmod run -p app               # Run the "app" member binary
cmod run -p app --release     # Run in release mode
```

### Remove a member

```bash
cmod workspace remove experimental
```

A member is removed by name or by directory. The `members` entry naming its directory is dropped; a member that a glob matches is added to `exclude` instead (refused when that would exclude members inside its directory too). The directory is left in place.

## Example Workspace

Here's the structure from the `examples/workspace/` project:

**Root `cmod.toml`:**

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

**`core/cmod.toml`** — a static library with no external dependencies:

```toml
[package]
name = "core"
version = "0.1.0"
edition = "2023"
description = "Core data types for the workspace example"
authors = ["cmod contributors"]
license = "Apache-2.0"

[module]
name = "local.core"
root = "src/lib.cppm"

[toolchain]
compiler = "clang"
cxx_standard = "20"

[build]
type = "static-lib"
```

**`app/cmod.toml`** — a binary that depends on other members:

```toml
[package]
name = "app"
version = "0.1.0"
edition = "2023"
description = "Application binary for the workspace example"
authors = ["cmod contributors"]
license = "Apache-2.0"

[module]
name = "local.app"
root = "src/lib.cppm"

[dependencies]
core = { path = "../core" }
utils = { path = "../utils" }

[toolchain]
compiler = "clang"
cxx_standard = "20"

[build]
type = "binary"
```
