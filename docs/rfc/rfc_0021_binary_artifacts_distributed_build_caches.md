# RFC-0021: Binary Artifacts & Distributed Build Caches

## Status
Draft

## Summary
This RFC defines **binary artifact generation, caching, and distributed reuse** in **cmod**. The goal is to dramatically reduce build times by reusing previously built module artifacts while preserving correctness, reproducibility, and safety.

---

## Motivation

C++ module builds are expensive due to:
- Large translation units
- Repeated template instantiations
- Rebuilding identical dependencies across machines

Modern workflows (CI, monorepos, remote teams) require fast, cacheable builds similar to:
- Cargo incremental + shared caches
- Bazel remote cache

---

## Goals

- Reuse compiled module artifacts safely
- Support local and remote caches
- Be transparent to developers
- Integrate with lockfiles and toolchains

## Non-Goals

- Caching final application binaries
- Hiding ABI incompatibilities
- Replacing full build systems

---

## Cached Artifact Types

`cmod` may cache:
- C++ module BMIs (Binary Module Interfaces)
- Compiled object files
- Precompiled headers (if used)
- Dependency metadata

Artifacts are always associated with **exact build inputs**.

---

## Cache Key Design

A cache key is derived from:
- Module source hash
- Dependency graph hash (from `cmod.lock`)
- Compiler identity + version
- Standard library
- Target triple
- Enabled features and flags

```
cache_key = hash(
  module_sources,
  dependency_lock,
  toolchain,
  target,
  features
)
```

Any mismatch invalidates the cache entry.

---

## Local Cache

- Default cache location: `~/.cache/cmod/`
- Shared across projects and workspaces
- LRU eviction strategy

Commands:
```
cmod cache status
cmod cache clean
```

---

## Remote Cache

Remote caches are **optional** and explicitly configured.

```toml
[cache]
remote = "https://cache.acme.internal"
mode = "read-write"
```

Supported modes:
- `read-only`
- `read-write`
- `off`

---

## Upload & Download Policy

- Artifacts are uploaded only after successful builds
- Downloads require cache key match
- Corrupt artifacts are discarded automatically

---

## CI Integration

Typical CI flow:
```
cmod build --locked
```

- CI nodes pull artifacts from remote cache
- Builds populate cache for future runs

---

## Workspace Behavior

- Workspace modules share cache entries
- Cross-module reuse enabled when keys match
- Parallel cache fetch supported

---

## Security Considerations

- Cache entries are content-addressed
- Optional signing of artifacts (see RFC-0022)
- Remote cache authentication required

---

## Comparison

| System | Cache Type |
|------|-----------|
| Cargo | Incremental + target dir |
| Bazel | Remote cache |
| Buck | Distributed cache |

---

## Backward Compatibility

- Cache disabled by default
- Builds without cache behave identically

---

## Open Questions

- Should final binaries ever be cached?
- Cache eviction policies for CI environments?
- Multi-toolchain cache coexistence strategy?

---

## Next RFC

- RFC-0022: Security, Signing & Supply-Chain Integrity
