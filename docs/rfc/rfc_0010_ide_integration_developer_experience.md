# RFC-0010: IDE Integration & Developer Experience

## Status
Draft

## Summary
This RFC defines the **IDE integration and developer experience** for **cmod**, focusing on how C++ modules are surfaced in editors, how builds interact with IDE tooling, and how developers can inspect, debug, and navigate module-based projects efficiently.

Goals:
- Syntax-aware code completion and module browsing
- Module-aware diagnostics and error reporting
- Real-time build feedback
- Visualizing module dependency graphs
- Supporting cross-toolchain development

---

## Motivation

Modern IDEs (VSCode, CLion, Xcode) provide strong support for C++ header-based workflows, but **modules introduce new challenges**:
- Module boundaries are not files
- BMIs must be prebuilt before code completion
- Cross-target builds complicate navigation

cmod aims to **bridge this gap**.

---

## IDE Features

### Module-Aware Autocomplete
- IDE queries cmod for module symbols
- cmod provides indexed exports from BMIs
- Autocomplete works for:
  - Root module symbols
  - Module partitions
  - Imported modules
- Cache used to avoid repeated compilation

### Real-Time Diagnostics
- Errors in interface units propagate to dependents
- IDE can query cmod build graph to highlight affected files
- Incremental re-analysis avoids full rebuilds

### Graph Visualization
- `cmod graph --ide` outputs a format IDEs can render
- Shows module dependencies, partitions, and build status
- Supports coloring for:
  - Up-to-date
  - Needs rebuild
  - Failed compilation

### Cross-Toolchain Awareness
- IDE integrates with multiple toolchains as defined in RFC-0008
- Switch target context for code completion, diagnostics, and navigation
- Ensures correct standard library and ABI context

### Lockfile Awareness
- IDE uses `cmod.lock` to provide reproducible context
- Prevents “works in IDE but fails on CI” scenarios

---

## Developer CLI Features
- `cmod status` → shows incremental build state per module
- `cmod explain <module>` → why a rebuild is triggered
- `cmod graph` → textual or JSON DAG
- `cmod cache status` → show artifact reuse

---

## Incremental Feedback Loop
1. Developer edits source or interface
2. IDE notifies cmod of changed files
3. cmod updates build graph and cache hashes
4. IDE receives updated diagnostics and completion info
5. Optional: prefetch BMIs for faster access

---

## Integration Approach
- Language Server Protocol (LSP) is recommended
- cmod acts as LSP backend exposing:
  - Completion items
  - Diagnostics
  - Build graph queries
- Minimal editor-specific plugins required

---

## Open Questions
- How to handle legacy header-only TUs alongside modules in IDEs?
- Should IDE integration prefetch all BMIs for very large monorepos?
- Best practices for caching symbols to balance memory vs latency?

---

## Next RFCs
- RFC-0011: Optional Precompiled Module Distribution (safe sharing with signatures, cache integration, enterprise support)
- RFC-0012: Advanced Build Strategies & Performance Optimizations (incremental linking, parallel BMI generation, distributed builds)

