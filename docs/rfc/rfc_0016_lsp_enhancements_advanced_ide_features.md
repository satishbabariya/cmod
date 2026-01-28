# RFC-0016: Optional Language Server Protocol Enhancements & Advanced IDE Features

## Status
Draft

## Summary
This RFC defines **advanced LSP and IDE features** for **cmod**, building upon RFC-0010 and RFC-0014, to improve developer productivity, real-time feedback, and deep code insights in module-based C++ projects.

Goals:
- Extend Language Server Protocol (LSP) support for module-specific features
- Provide real-time diagnostics, dependency notifications, and incremental updates
- Enable advanced code navigation and exploration
- Enhance IDE performance using cached BMIs and parallel analysis
- Integrate with build system and distributed execution

---

## Motivation

Modules introduce complexities that standard LSP cannot fully address:
- BMIs must be prebuilt for accurate completion
- Dependencies and partitions need real-time awareness
- Cross-toolchain and cross-target contexts require careful tracking

Advanced LSP features improve IDE experience, reduce rebuild overhead, and prevent developer friction.

---

## Advanced LSP Features

### Module-Aware Code Completion
- Completion items derived from BMIs of imported modules
- Supports module partitions and nested exports
- Context-aware based on target triple and toolchain

### Real-Time Dependency Notifications
- LSP server notifies IDE when upstream modules are rebuilt
- IDE highlights affected code and suggests incremental rebuilds
- Integration with `cmod graph --ide` for visual context

### Incremental Diagnostics
- Errors propagate through module DAG incrementally
- IDE shows affected nodes with precise error localization
- Reduces full project rebuilds and maintains fast feedback

### Cross-Target Awareness
- LSP server maintains separate contexts for multiple targets
- Completion and diagnostics respect target-specific BMIs
- IDE can switch contexts seamlessly

### Build Graph Queries
- IDE can query LSP server for:
  - Module dependencies
  - Cache status (hits/misses)
  - Build progress and critical path
- Enables interactive optimization suggestions

---

## IDE Performance Optimizations

- Pre-fetch BMIs in background for common modules
- Parallel analysis of independent nodes
- Smart caching of diagnostic results
- Minimize memory footprint by unloading unused module contexts

---

## Integration with CI/CD and Distributed Builds

- LSP server aware of remote build status (RFC-0013)
- Provides real-time feedback from remote workers
- Tracks verification status of remote BMIs and artifacts

---

## Open Questions

- Should the LSP server support incremental parsing of very large monorepos?
- Optimal strategy for module partition caching in IDE memory?
- Best approach for visualizing cross-target dependencies in IDE?

---

## Next RFCs
- RFC-0017: Module Metadata Extensions & Advanced Dependency Features
- RFC-0018: Optional Tooling Plugins & Ecosystem Utilities

