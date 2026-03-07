# Monorepo Patterns with cmod Workspaces

*How to structure large C++ codebases with cmod workspaces: shared dependencies, inter-member builds, and CI strategies.*

---

As C++ projects grow, they naturally split into multiple libraries, applications, and test suites. Managing these as separate repositories creates dependency hell. Managing them as a single monolithic build creates a different kind of hell. cmod workspaces provide a middle path.

## What Is a cmod Workspace?

A workspace is a collection of cmod members that share a single `cmod.lock` and can depend on each other via path dependencies. Think of it as Cargo workspaces for C++.

```
my_project/
├── cmod.toml          # Workspace root
├── cmod.lock          # Shared lockfile
├── core/
│   ├── cmod.toml
│   └── src/
├── networking/
│   ├── cmod.toml
│   └── src/
└── app/
    ├── cmod.toml
    └── src/
```

## Setting Up

```bash
cmod init my_project --workspace
cd my_project
cmod workspace add core
cmod workspace add networking
cmod workspace add app
```

## Pattern 1: Layered Architecture

```toml
# networking/cmod.toml
[dependencies]
core = { workspace = true }

# app/cmod.toml
[dependencies]
core = { workspace = true }
networking = { workspace = true }
```

## Pattern 2: Shared Dependencies

When multiple members depend on the same external library, the workspace resolves to one version. Only one copy is compiled.

## Pattern 3: Test Harness

A dedicated test member that depends on all other members for integration testing.

## Pattern 4: Plugin Architecture

Core engine + multiple plugins, each depending on the engine but not on each other.

## Workspace Build Behavior

1. Load workspace manifest, discover all members
2. Resolve dependencies against shared lockfile
3. Construct combined dependency graph
4. Compile in topological order
5. Share BMIs and object files across members

## Best Practices

- Keep members focused with a single responsibility
- Minimize cross-member dependencies (clean DAG)
- Commit the shared lockfile
- Use workspace-level commands from the root

## When to Use a Workspace vs. Separate Repos

**Use a workspace when:** components are developed together, frequent cross-component changes, unified dep resolution needed.

**Use separate repos when:** independent release cycles, different team ownership, fine-grained access control needed.

[Get started with cmod workspaces →](https://github.com/satishbabariya/cmod)
