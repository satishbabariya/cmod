# RFC-0013: Distributed Builds & Remote Execution

## Status
Draft

## Summary
This RFC defines **distributed build strategies and remote execution** for **cmod**, enabling large C++ projects to leverage multiple hosts or CI workers to compile modules and generate artifacts while maintaining correctness, reproducibility, and cache integrity.

Goals:
- Execute module compilation nodes remotely
- Ensure deterministic builds across hosts
- Integrate with cache for artifact reuse
- Support secure verification of remote results
- Minimize network overhead and latency

---

## Motivation

Large C++ module-based projects can face slow build times due to deep dependency graphs and large module interfaces. Remote execution allows parallelization beyond a single machine, making CI and multi-developer builds faster and more efficient.

---

## Remote Execution Model

### Worker Nodes
- Each worker runs cmod agent capable of compiling modules
- Worker environment must match target triple, compiler, and stdlib of the node
- Workers maintain local cache and optionally share with central repository

### Scheduler
- Orchestrates node dispatch based on DAG (RFC-0007)
- Respects dependencies: parent nodes wait for child node completion
- May prioritize nodes affecting IDE feedback for low-latency builds

### Node Dispatch
- Each node includes:
  - Source code hash
  - Dependent BMI hashes
  - Toolchain identity
- Scheduler sends node to worker, waits for result and verification

---

## Network Protocol

- Transport via gRPC or HTTP/S
- Node metadata includes:
  - Build inputs (source hash, compiler flags, imports)
  - Target triple
  - Expected artifact hash
- Responses include compiled BMI/object and verification signature
- Optional streaming for incremental logs

---

## Artifact Verification

- All remote artifacts are **verified against content hash and signature** (RFC-0009)
- Failures trigger local rebuild or rescheduling
- Ensures cache poisoning or malicious artifacts cannot affect build correctness

---

## Cache Integration

- Remote cache may be queried before node execution
- Locally cached artifacts avoid redundant compilation
- Remote cache writes are atomic
- Optional read-only mode for CI

---

## Fault Tolerance

- Worker failure: node is rescheduled on another worker
- Network failure: retry or fallback to local build
- Partial artifacts discarded automatically
- Deterministic rebuild guarantees correctness

---

## Security Considerations

- Only trusted workers allowed in the pool
- Signed artifacts and secure transport required
- Audit logs may be maintained for compliance

---

## IDE & Developer Integration

- IDE can dispatch compilation nodes to local or remote workers
- Incremental feedback maintained by tracking node completion
- Remote execution transparent to developer

---

## Open Questions

- Optimal scheduling algorithms for heterogeneous clusters?
- Should workers cache all modules or only frequently used ones?
- How to efficiently handle cross-target remote builds?
- Network protocol choice: gRPC vs HTTP/2 vs custom?

---

## Next RFCs
- RFC-0014: Module Graph Visualization & Developer Tools Enhancements
- RFC-0015: Cmod Ecosystem Governance & Community Standards