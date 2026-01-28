# RFC-0005: Cache & Artifact Model

## Status
Draft

## Summary
This RFC defines the **cache and artifact model** for `cmod`, specifying how compiler outputs—especially **C++ module artifacts (PCMs/BMIs)**—are stored, reused, invalidated, and shared safely.

The cache model is designed to be:
- Correct by construction
- Compiler- and flag-specific
- Deterministic
- CI- and enterprise-friendly

---

## Motivation

C++ module artifacts introduce new constraints:

- PCMs are **not portable** across compilers
- Often not portable across compiler *versions*
- Sensitive to flags, target triples, and standard libraries

Incorrect reuse leads to:
- ODR violations
- Silent miscompilations
- Non-reproducible builds

`cmod` therefore treats cache correctness as a **hard requirement**, not a best-effort optimization.

---

## Design Goals

1. Never reuse an incompatible artifact
2. Maximize safe reuse within a workspace
3. Enable fast CI rebuilds
4. Keep cache logic independent of resolution and planning
5. Make cache behavior inspectable and debuggable

## Non-Goals

- Sharing binary compatibility across compilers
- Solving ABI stability
- Long-term artifact archival

---

## Artifact Types

`cmod` recognizes the following artifact categories:

| Artifact | Description |
|--------|-------------|
| PCM/BMI | Compiled module interface |
| OBJ | Object file |
| LIB | Static or shared library |
| BIN | Executable |

Only **PCMs and OBJs** are cached by default.

---

## Cache Scope

### Local Cache (Default)

```text
~/.cmod/cache/
└── clang-18.1.2/
    └── x86_64-apple-darwin/
        └── <hash>/
            ├── module.pcm
            └── module.obj
```

Properties:
- User-local
- Compiler-specific
- Target-specific
- Safe by default

---

### Workspace Cache (Optional)

```text
.project/.cmod/cache/
```

Properties:
- Faster rebuilds within repo
- Not shared across machines
- Ignored by VCS

---

### CI Cache (Explicit)

CI systems MAY persist the local cache directory **verbatim**.

Rules:
- Cache keys must include compiler version
- Cache restores are advisory, not authoritative

---

## Cache Keys

Each artifact is indexed by a **content-addressed cache key**:

```
hash(
  source-content,
  module-name,
  compiler-id,
  compiler-version,
  stdlib-id,
  target-triple,
  compilation-flags,
  dependent-artifact-hashes
)
```

Properties:
- Immutable
- Deterministic
- Collision-resistant

---

## Cache Read Rules

Before executing a BuildNode:

1. Compute cache key
2. Check local cache
3. Validate artifact integrity
4. Use artifact if present

If validation fails, artifact is discarded.

---

## Cache Write Rules

After successful node execution:

1. Write artifact to temporary path
2. Verify outputs
3. Atomically commit to cache

Partial or failed outputs are never cached.

---

## Cache Invalidation

Cache entries are invalidated implicitly by key mismatch.

Explicit invalidation commands:

```bash
cmod cache clean
cmod cache gc
```

Rules:
- No manual deletion required
- Old artifacts are garbage-collected

---

## PCM-Specific Rules

### Strict Isolation

PCMs:
- Are never reused across compiler backends
- Are never reused across compiler versions
- Are never reused across standard libraries

---

### No PCM Distribution

`cmod` **does not distribute PCMs** via Git or registries.

Rationale:
- Unsafe
- Non-portable
- Difficult to validate

---

## Determinism Guarantees

Given:
- Same source tree
- Same `cmod.lock`
- Same compiler + flags

`cmod` guarantees:
- Identical cache keys
- Identical build plans
- Identical final artifacts

---

## Debugging & Introspection

```bash
cmod cache status
cmod cache explain <module>
```

Provides:
- Cache hit/miss reasons
- Artifact provenance
- Dependency hash breakdown

---

## Security Considerations

- Cache entries are content-verified
- No execution of cached artifacts
- Optional read-only cache mode

---

## Comparison

| Tool | PCM Safety | Content-Addressed | Deterministic |
|------|------------|------------------|---------------|
| CMake | ❌ | ❌ | ❌ |
| Bazel | ⚠️ | ✅ | ✅ |
| build2 | ✅ | ⚠️ | ✅ |
| **cmod** | ✅ | ✅ | ✅ |

---

## Open Questions

- Should remote cache protocols be supported?
- Cache size limits and eviction policies?
- Debug vs Release cache separation?

---

## Conclusion

C++ module builds demand **strict cache correctness**.

By using content-addressed, compiler-scoped artifacts, `cmod` achieves fast rebuilds without compromising safety—while remaining honest about C++’s ABI realities.

