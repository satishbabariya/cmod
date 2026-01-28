# RFC-0014: Module Graph Visualization & Developer Tools Enhancements

## Status
Draft

## Summary
This RFC defines the **module graph visualization and developer tooling enhancements** for **cmod**, providing better insights into module dependencies, build states, and performance metrics, integrated with IDEs and CLI tools.

Goals:
- Visualize module dependency DAGs
- Highlight build status, incremental changes, and cache reuse
- Provide interactive exploration in IDEs
- Support debugging and optimization of build pipelines
- Enhance developer understanding of complex module graphs

---

## Motivation

C++ projects with large module-based codebases can have deep, complex dependency graphs. Without visualization and tooling, developers struggle with:
- Understanding rebuild triggers
- Identifying performance bottlenecks
- Detecting unnecessary dependencies

Enhanced tooling helps in faster debugging, optimization, and onboarding.

---

## Graph Visualization Features

### CLI Graph
- `cmod graph` outputs:
  - ASCII or DOT format DAG
  - Node labels: module name, version, toolchain, build state
  - Edge labels optional: import type or partition info
- Flags to highlight:
  - Up-to-date nodes
  - Nodes needing rebuild
  - Failed or missing nodes

### Interactive GUI / IDE Integration
- IDEs can render DAG in tree or network graph
- Clickable nodes to inspect:
  - Build logs
  - Source files
  - Dependencies
  - Cached BMIs
- Filtering options:
  - By target, toolchain, status
  - Highlight critical path

### Incremental Update Visualization
- Real-time updates as modules are built or modified
- Use color-coding for build progress
- Integration with IDE incremental compilation notifications (RFC-0010)

---

## Metrics & Insights

- Node-level build time statistics
- Cache hit/miss rates
- Critical path identification for performance optimization
- Suggest optimizations for reducing rebuilds or parallelism adjustments

---

## Developer Tooling Enhancements

- `cmod explain <module>`: Why a module rebuild is triggered
- `cmod status`: Show incremental build state and cache usage
- `cmod deps <module>`: Show flattened dependency tree
- IDE plugins utilize LSP to provide inline hints, navigation, and build state indicators

---

## Open Questions

- Optimal rendering for extremely large graphs (thousands of modules)?
- Should CLI and GUI provide interactive graph pruning and filtering?
- Integration with CI dashboards for centralized graph visualization?

---

## Next RFCs
- RFC-0015: Cmod Ecosystem Governance & Community Standards
- RFC-0016: Optional Language Server Protocol Enhancements & Advanced IDE Features