# RFC-0012: Advanced Build Strategies & Performance Optimizations

## Status
Draft

## Summary
This RFC defines **advanced build strategies and performance optimizations** for **cmod**, focusing on accelerating C++ module builds while preserving correctness, reproducibility, and cache integrity.

Goals:
- Optimize incremental compilation
- Parallelize BMI and object generation
- Support distributed builds
- Reduce redundant rebuilds
- Integrate with IDEs and CI for low-latency feedback

---

## Motivation

Even with precompiled modules and a content-addressed cache, large C++ projects may suffer slow build times due to:
- Deep module dependency graphs
- Large module interfaces
- Multi-target builds
- Inefficient parallelization

cmod must provide **deterministic yet fast builds** at scale.

---

## Parallel Build Strategy

### Node-Level Parallelism
- Modules with no unresolved dependencies can compile concurrently
- Scheduler respects CPU core limits
- Dependencies of a node must complete before its compilation

### Partition-Level Parallelism
- Module partitions (`foo:bar`) compiled before parent module
- Partitions of different modules may compile concurrently

### Cross-Target Parallelism
- Builds for multiple targets may run simultaneously in separate graphs
- Each target has isolated cache and BMI nodes

---

## Incremental Compilation Optimizations

- Track content hash of source, imports, and compiler flags
- Only rebuild nodes where input hash changed
- Upstream change triggers dependent rebuilds automatically
- IDE integration receives immediate notifications for changed nodes

### Partial Rebuilds
- For small interface edits, downstream modules may reuse existing BMIs if unaffected
- Requires precise dependency analysis (RFC-0007)

---

## Distributed Builds

- Optional remote execution of module compilation
- Each node can be dispatched to a worker
- Cache-aware: remote workers fetch artifacts if available
- Network-safe verification ensures correctness

---

## Cache Optimization

- Content-addressed keys reused across local and remote caches
- LRU or size-based eviction policies recommended
- Optional shared artifact cache across CI jobs or developer machines
- Precompiled module distribution (RFC-0011) integrated

---

## Incremental Linking

- Only link modules whose object files changed
- Parallel link across independent targets
- Integration with LTO/ThinLTO supported where possible

---

## IDE Feedback Optimization

- Pre-fetch BMIs in the background for faster code completion
- Use dependency graph to determine minimal set of nodes to re-analyze
- Incremental diagnostics updated per node change

---

## Open Questions

- Granularity of partial rebuilds: partition-level or module-level?
- Optimal distributed build protocol (gRPC, HTTP, custom)
- Balancing cache reuse vs remote execution latency
- Best practices for large monorepo scaling

---

## Next RFCs
- RFC-0013: Distributed Builds & Remote Execution (network protocol, worker scheduling, cache consistency)
- RFC-0014: Module Graph Visualization & Developer Tools Enhancements

