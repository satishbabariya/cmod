# workspace

Multi-member workspace (monorepo) example.

## What this demonstrates

- Workspace root `cmod.toml` with `[workspace]` section
- Multiple members with inter-member path dependencies
- Topological build ordering across members

## Project structure

```
workspace/
├── cmod.toml           # Workspace root: members list
├── core/
│   ├── cmod.toml       # static-lib, no deps
│   └── src/
│       └── lib.cppm    # export module local.core; (Record, make_record, lookup)
├── utils/
│   ├── cmod.toml       # static-lib, path dep on core
│   └── src/
│       └── lib.cppm    # export module local.utils; import local.core;
└── app/
    ├── cmod.toml       # binary, path deps on core+utils
    └── src/
        ├── lib.cppm    # export module local.app;
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

# Build the entire workspace
cmod build

# Run the app member
cmod run -p app
```

Expected output:

```
alpha: 10 -> 20
beta: 20 -> 40
gamma: 30 -> 60
Found 'beta' with value 20
```

## Key concepts

- **Workspace root**: the top-level `cmod.toml` lists `members` — each member is a sub-directory with its own `cmod.toml`.
- **Path dependencies**: `core = { path = "../core" }` references a sibling workspace member by relative path.
- **Topological build order**: cmod automatically builds `core` first, then `utils` (which depends on `core`), then `app` (which depends on both).
- **PCM propagation**: when building `utils`, cmod passes the PCM from `core` so `import local.core;` resolves correctly.
