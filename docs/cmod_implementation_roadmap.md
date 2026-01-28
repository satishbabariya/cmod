# cmod Implementation Roadmap

## Phase 0 — Foundations (MVP)
**Goal:** Deterministic dependency resolution + lockfiles

- `cmod.toml` parser
- Git-based dependency resolver
- Semver + commit pinning
- `cmod.lock` generation
- CLI: `init`, `add`, `resolve`

Outcome: reproducible dependency graph, no building yet

---

## Phase 1 — Module-Aware Builds
**Goal:** Build real C++20 modules

- LLVM / Clang driver integration
- Module graph (BMI DAG)
- Incremental rebuilds
- CLI: `build`, `deps`

Outcome: fast local builds using C++ modules

---

## Phase 2 — Workspaces & Caching
**Goal:** Scale to large repos

- Workspace manager
- Shared build cache
- Cache key computation
- Parallel module builds

Outcome: monorepo-ready performance

---

## Phase 3 — Distributed Builds
**Goal:** CI & team-scale acceleration

- Remote cache protocol
- Artifact upload/download
- CI-friendly workflows

Outcome: Bazel-like speed without Bazel

---

## Phase 4 — Security & Verification
**Goal:** Supply-chain integrity

- Signature verification
- Secure cache enforcement
- `--locked --verify` modes

Outcome: production-grade trust

---

## Phase 5 — Tooling & Ecosystem
**Goal:** Adoption

- LSP integration
- Plugin SDK
- Visualization tools

Outcome: developer ecosystem
