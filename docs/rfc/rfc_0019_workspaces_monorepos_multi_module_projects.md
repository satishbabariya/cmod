# RFC-0019: Workspaces, Monorepos & Multi-Module Projects

## Status
Draft

## Summary
This RFC introduces **workspaces** to **cmod**, enabling first-class support for monorepos and large multi-module codebases. A workspace allows multiple C++ modules to be developed, built, and versioned together while remaining independently consumable.

---

## Motivation

Modern C++ projects often:
- Contain many tightly related modules
- Share build configuration and tooling
- Require coordinated dependency resolution
- Want fast iteration without publishing intermediate versions

Without workspace support, developers must duplicate configuration, manually manage paths, or publish private modules prematurely.

---

## Goals

- Native support for monorepos
- Shared dependency resolution and caching
- Independent module identities
- Deterministic builds across the workspace
- Zero central registry requirements

## Non-Goals

- Forcing a single version for all modules
- Replacing git submodules or subtrees
- Implicit publishing of workspace modules

---

## Workspace Definition

A workspace is defined by a `cmod.workspace.toml` file at the repository root.

```toml
[workspace]
name = "github.com/acme/engine"

members = [
  "core",
  "math",
  "render",
  "tools/asset-pipeline"
]

exclude = ["experimental/*"]

[workspace.dependencies]
fmt = "^10.2"
spdlog = "^1.13"
```

---

## Member Modules

Each workspace member:
- Contains its own `cmod.toml`
- Has an independent module path and version
- Can be built standalone or as part of the workspace

Example:
```
render/
 ├─ cmod.toml
 └─ src/
```

---

## Dependency Resolution Rules

1. Workspace members override external dependencies
2. Shared workspace dependencies are unified
3. Conflicts across members produce hard errors
4. Path-based resolution is preferred during development

### Example

```toml
[dependencies]
core = { workspace = true }
```

This resolves to the local workspace module, not an external repo.

---

## Build Behavior

- `cmod build` at workspace root builds all members
- Incremental builds reuse shared caches
- Parallel builds across modules are allowed
- Failure in one module stops the workspace build

---

## Versioning & Publishing

- Workspace modules may have different versions
- Publishing a module ignores other workspace members
- Optional `workspace.version` can enforce a shared version

```toml
[workspace]
version = "0.8.0"
```

---

## Lockfiles

- A single `cmod.lock` exists at the workspace root
- Captures resolved dependencies for all members
- Ensures reproducible builds across the repo

---

## Tooling Integration

- IDEs treat workspace as a single logical project
- Plugins operate at workspace or module scope
- Dependency graphs can be visualized per-module or global

---

## Comparison

| System | Workspace Equivalent |
|------|----------------------|
| Cargo | Workspaces |
| Go | Multi-module repos |
| Bazel | Packages |

---

## Backward Compatibility

- Single-module repos remain unchanged
- Workspace support is opt-in

---

## Open Questions

- Should cross-workspace dependencies be allowed?
- Should workspace-local patches be supported?
- How strict should version unification be?

---

## Next RFCs

- RFC-0003: Lockfiles & Reproducible Builds (already exists)
- RFC-0005: Binary Artifacts & Distributed Build Caches (already exists)
- RFC-0009: Security, Trust & Supply-Chain Integrity (already exists)
- RFC-UNIFIED: Unified cmod.toml Schema Specification
