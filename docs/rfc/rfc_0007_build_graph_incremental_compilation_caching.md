# RFC-0007: Build Graph, Incremental Compilation & Caching

## Status
Draft

## Summary
This RFC defines how **cmod** constructs the build graph for C++ modules, performs incremental compilation, and manages local and shared caches — leveraging LLVM and Clang module infrastructure.

Primary goals:
- Fast incremental builds
- Correct module dependency ordering
- Safe reuse of compiled artifacts
- Predictable rebuild triggers

---

## Motivation

C++ modules fundamentally change compilation order and caching semantics:
- Modules must be compiled before importers
- BMI (Binary Module Interface) files are compiler- and flag-sensitive
- Header-based heuristics are insufficient

**cmod** treats the build as a **module DAG**, not a file list.

---

## Build Graph Model

### Nodes

Each node represents a **module unit**:
- Primary module interface (`export module foo;`)
- Module partitions (`export module foo:bar;`)
- Module implementation units

Headers are not graph nodes.

---

### Edges

Edges represent `import` relationships:

```text
foo → bar → baz
```

Properties:
- DAG only (cycles forbidden)
- Partition edges resolved before external edges
- Std modules treated as implicit roots

---

## Graph Construction

Steps:

1. Scan sources for `export module` / `import`
2. Resolve module names to packages (RFC-0002)
3. Load dependency manifests
4. Build a global module DAG
5. Topologically sort

Graph construction is deterministic.

---

## Compilation Units

cmod distinguishes:

| Unit Type | Output |
|--------|-------|
| Module interface | BMI + object |
| Module partition | BMI + object |
| Module implementation | object only |
| Legacy TU | object only |

---

## Incremental Compilation Rules

A node is **rebuilt if any input changes**:

Inputs include:
- Source file contents
- Imported module BMIs
- Compiler version
- Compiler flags
- Target triple
- Stdlib selection

No timestamp-based invalidation.

---

## Cache Architecture

### Local Cache

Default location:

```text
~/.cache/cmod/
```

Structure:

```text
<module-id>/<hash>/
  ├── module.bmi
  ├── object.o
  └── metadata.json
```

---

### Cache Key

Cache key is a content hash of:

- Module source
- Imported module hashes
- Compiler identity
- Compile flags
- Target triple
- ABI

Identical keys guarantee reuse.

---

## Shared Cache (Optional)

cmod MAY support shared caches:
- CI cache servers
- Networked developer cache

Properties:
- Read-through
- Write-back optional
- Trust controlled by config

---

## Parallelism

Rules:
- Nodes with no incoming edges may compile in parallel
- Partitions compile before parent module
- CPU core-aware scheduling

Goal: maximize parallel BMI generation.

---

## Failure Handling

If a module fails to compile:
- All downstream nodes are skipped
- Error context preserved
- Partial cache entries discarded

---

## Integration with LLVM

cmod relies on:
- Clang dependency scanning (`clang-scan-deps`)
- Clang module serialization
- LLVM target triples

LLVM is the authoritative source of truth.

---

## Legacy Headers

Header-based TUs:
- Are supported
- Cannot be imported by modules
- Do not participate in BMI caching

Encourages gradual migration.

---

## Determinism Guarantees

Given:
- Same `cmod.lock`
- Same toolchain
- Same sources

Result:
- Identical build graph
- Identical cache keys

---

## Open Questions

- Should BMI format be standardized across Clang versions?
- Should cache eviction be LRU or size-based?
- How to expose graph introspection (`cmod graph`)?

---

## Next RFCs

- RFC-0008: Toolchains, Targets & Cross-compilation
- RFC-0009: Security, Trust & Supply Chain
- RFC-0010: IDE Integration & Developer Experience

