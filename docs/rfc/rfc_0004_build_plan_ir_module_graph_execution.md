# RFC-0004: Build Plan IR & Module Graph Execution

## Status
Draft

## Summary
This RFC defines the **Build Plan Intermediate Representation (IR)** used by `cmod` to translate a resolved module graph into a **deterministic, incremental, and parallelizable build execution plan**.

The Build Plan IR sits between:
- Dependency resolution (`cmod.lock`) 
- Module graph construction (RFC-0007)
- Compiler backends (Clang, future GCC/MSVC)

It is the **core execution model** of `cmod`, while RFC-0007 defines the **build graph data model**. This RFC focuses on the IR representation and execution planning, whereas RFC-0007 focuses on graph construction and caching.

---

## Motivation

Existing C++ build systems:
- Discover dependencies during compilation
- Interleave dependency analysis and execution
- Rebuild excessively or incorrectly

C++ modules require:
- Full dependency knowledge **before** compilation
- Precise ordering of interface units
- Explicit control over BMI generation

`cmod` addresses this by constructing a **complete build graph upfront**.

---

## Design Goals

1. Fully deterministic build plans
2. Incremental rebuilds with minimal invalidation
3. Maximum safe parallelism
4. Compiler-agnostic IR
5. Debuggable and inspectable execution

## Non-Goals

- Replacing compiler optimizers
- Encoding ABI compatibility rules
- Handling runtime linking semantics

---

## Inputs to the Build Plan

The Build Plan IR is constructed from:

1. `cmod.toml`
2. `cmod.lock`
3. Module source tree
4. Compiler backend metadata
5. Target triple and build profile

---

## Core Concepts

### Module Unit

A **Module Unit** is a single translation unit, one of:

- Module Interface Unit (`export module ...`)
- Module Implementation Unit
- Module Partition Unit
- Legacy Translation Unit (non-module)

Each unit is immutable once hashed.

---

### Build Node

Each compilation step is represented as a **Build Node**.

```text
BuildNode {
  id: Hash
  kind: Interface | Implementation | Object | Link
  inputs: [BuildNode]
  command: CompilerInvocation
  outputs: [Artifact]
}
```

---

### Artifact

Artifacts are explicit outputs of build nodes:

- PCM/BMI
- Object file
- Static or shared library
- Executable

Artifacts are **content-addressed**.

---

## Build Plan IR Structure

```text
BuildPlan {
  target: TargetTriple
  profile: Debug | Release
  nodes: DAG<BuildNode>
}
```

Properties:
- Acyclic by construction
- Fully ordered for interface units
- Lazily executed

---

## Module Graph Construction

### Phase 1: Dependency Scan

- Invoke compiler backend dependency scanner
- Clang: `clang-scan-deps`
- Output: raw module dependency graph (JSON)

---

### Phase 2: Canonicalization

- Normalize module names (RFC-0002)
- Bind modules to Git sources (RFC-0003)
- Reject ambiguous or illegal imports

---

### Phase 3: Graph Expansion

- Expand partitions
- Attach implementation units
- Attach legacy includes

Result: **Complete module graph**.

---

## Build Node Generation

### Interface Nodes

Rules:
- One node per module interface unit
- Must execute before dependents
- Produce exactly one PCM artifact

---

### Implementation Nodes

Rules:
- Depend on corresponding interface PCM
- Produce object files
- Can execute in parallel

---

### Link Nodes

Rules:
- Consume object files
- Produce final binary or library
- Executed last

---

## Incremental Rebuild Rules

### Hash Inputs

Each BuildNode hash includes:

- Source content
- Compiler identity
- Compiler version
- Compilation flags
- Target triple
- Dependent artifact hashes

---

### Invalidation Rules

| Change | Effect |
|------|--------|
| Interface change | Rebuild dependents |
| Implementation change | Rebuild object only |
| Flag change | Rebuild affected nodes |
| Lockfile change | Full graph rebuild |

---

## Parallel Execution Model

- Nodes execute when all inputs are ready
- Interface units limit parallelism by design
- Implementation units maximize parallelism

Scheduler is:
- Deterministic
- Work-stealing
- Deadlock-free

---

## Compiler Backend Interface

```text
CompilerBackend {
  scan_deps()
  compile_interface()
  compile_implementation()
  link()
}
```

Backends map BuildNodes to concrete invocations.

---

## Debugging & Introspection

```bash
cmod build --plan
cmod build --explain github.fmtlib.fmt
```

Outputs:
- Human-readable DAG
- JSON IR dump
- Node-level timing

---

## Failure Handling

- Node failures abort dependents
- Partial outputs are discarded
- Cache is not poisoned

---

## Comparison

| System | Pre-built DAG | Modules-first | Deterministic |
|------|---------------|---------------|---------------|
| CMake | ❌ | ❌ | ❌ |
| Bazel | ✅ | ⚠️ | ✅ |
| build2 | ✅ | ✅ | ✅ |
| **cmod** | ✅ | ✅ | ✅ |

---

## Open Questions

- How to represent header units explicitly?
- Should unity builds be supported?
- Cross-language (C/ObjC) nodes?

---

## Relationship to RFC-0007

This RFC (RFC-0004) defines the **Build Plan IR** for execution planning, while RFC-0007 defines the **Build Graph Model** for construction and caching:

- **RFC-0004**: BuildNode DAG, execution order, compiler interface
- **RFC-0007**: Module DAG, cache layout, incremental rules

Together they provide a complete build system: RFC-0007 constructs the module dependency graph and handles caching, then RFC-0004 converts it into an executable build plan.

---

## Conclusion

The Build Plan IR is the execution backbone of `cmod`.

By separating **graph construction** from **execution**, `cmod` achieves correctness, performance, and debuggability—without inheriting legacy C++ build system complexity.

