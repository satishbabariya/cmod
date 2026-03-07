# Understanding cmod's Content-Addressed Cache

*Deep dive into how cmod's SHA-256 artifact cache works: cache keys, invalidation strategy, and the path to distributed caching.*

---

Compilation is expensive. A medium-sized C++ project might take minutes to build from scratch. When you're iterating on code, most of that time is wasted recompiling modules that haven't changed. cmod's artifact cache eliminates this waste through content-addressed storage.

## Content-Addressed Storage Explained

In a content-addressed system, each artifact is stored under a key derived from its contents and inputs. If the inputs haven't changed, the key is the same, and the cached artifact is returned instead of recompiling.

This is fundamentally different from timestamp-based caching (like Make uses). Timestamps can lie — touching a file changes its timestamp without changing its content. Content hashes are deterministic and portable.

## What Goes Into a Cache Key

cmod computes cache keys using SHA-256 hashes of:

- **Source file content** — the SHA-256 hash of the source file
- **Compiler identity** — path, version string, target triple
- **Compilation flags** — optimization level, warnings, feature flags
- **Dependency BMI hashes** — cache keys of all imported modules
- **C++ standard** — c++20, c++23, etc.

The dependency BMI hashes are critical. If module A imports module B, and B's source changes, then B's cache key changes, which changes A's cache key. Cascading invalidation is automatic and precise.

## Cache Operations

```bash
cmod cache status      # See cache size, hit rate
cmod cache gc          # Remove entries older than 30 days
cmod cache clean       # Clear everything
cmod build --no-cache  # Build without cache
```

## Why This Matters for Incremental Builds

Consider a project with 50 modules. You change one leaf module:

1. The changed module is recompiled (cache miss)
2. Its direct dependents are recompiled (their keys changed)
3. Everything else is a cache hit

Build goes from 30 seconds to 2 seconds.

## Cache Correctness Guarantees

- **No false hits** — key includes every input that affects output
- **No stale entries** — content-addressed means it's right or doesn't exist
- **Safe concurrent access** — atomic file operations
- **Portable across machines** — content-based, not path-based

## The Path to Distributed Caching

Content-addressed caching naturally extends to remote caching (Phase 3 roadmap). Because keys are deterministic and machine-independent, a cache entry produced on one machine is valid on any machine with the same compiler setup.

## Comparison with Other Approaches

- **vs. ccache/sccache** — Those cache individual compilations. cmod is module-aware, tracking BMI dependencies.
- **vs. Bazel remote cache** — Similar principle, but cmod provides it with a much simpler tool.
- **vs. precompiled headers** — PCH are compiler-specific and fragile. Module BMIs are a language-level feature.

[Get started with cmod →](https://github.com/satishbabariya/cmod)
