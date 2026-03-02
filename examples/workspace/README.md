# workspace

Multi-member workspace (monorepo) example.

## What this demonstrates

- Workspace root `cmod.toml` with `[workspace]` section
- Multiple members with inter-member path dependencies
- Shared workspace-level dependency (`{ workspace = true }`)
- Topological build ordering across members

## Project structure

```
workspace/
├── cmod.toml           # Workspace root: members + shared deps
├── core/
│   ├── cmod.toml       # static-lib, no deps
│   └── src/
│       ├── lib.cppm    # export module local.core; (Record, make_record, lookup)
│       └── core.cpp    # module local.core; (lookup implementation)
├── utils/
│   ├── cmod.toml       # static-lib, path dep on core
│   └── src/
│       ├── lib.cppm    # export module local.utils; import local.core;
│       └── utils.cpp   # module local.utils;
└── app/
    ├── cmod.toml       # binary, path deps on core+utils, workspace dep on fmt-cmod
    └── src/
        ├── lib.cppm    # export module local.app;
        ├── app.cpp     # module local.app; (run implementation)
        └── main.cpp    # import local.app; int main()
```

## Build order

cmod resolves the dependency graph and builds members in topological order:

```
core -> utils -> app
```

## Usage

```bash
cd examples/workspace

# Resolve all workspace dependencies
cmod resolve

# Build the entire workspace
cmod build

# Run the app member
cmod run -p app
```

## Key concepts

- **Workspace root**: the top-level `cmod.toml` lists `members` and can declare `[workspace.dependencies]` shared across all members.
- **Path dependencies**: `core = { path = "../core" }` references a sibling workspace member by relative path.
- **`{ workspace = true }`**: inherits the dependency version from `[workspace.dependencies]` in the workspace root, ensuring all members use the same version.
- **Unified lockfile**: the workspace shares a single `cmod.lock` for all external dependencies.
