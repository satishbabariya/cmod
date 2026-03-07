# Building a C++ Tool in Rust: Lessons Learned

*Why we chose Rust to build a C++ package manager, and what we learned about cross-language tooling along the way.*

---

"Why would you build a C++ tool in Rust?" It's the first question we get, and it deserves a thoughtful answer.

cmod is a package manager and build orchestrator for C++20 modules, but its implementation is entirely in Rust. This isn't a philosophical statement about language superiority — it's a pragmatic engineering decision.

## Why Not C++?

Consider what cmod actually does:

- Parses TOML configuration files
- Resolves complex dependency graphs with semver constraints
- Invokes Git operations (clone, fetch, tag listing)
- Constructs DAGs and performs topological sorts
- Orchestrates parallel subprocess invocations
- Manages a content-addressed file cache
- Handles cryptographic hashing for verification

None of these operations require C++ features like templates, RTTI, or direct memory manipulation. They do require reliable error handling, safe concurrency, and fast string processing — areas where Rust excels.

## Why Rust Specifically

### Cargo as a role model
cmod is explicitly Cargo-inspired. Building it in Rust means we can study Cargo's implementation directly, borrow architectural patterns, and leverage the same ecosystem of libraries.

### Error handling
Rust's `Result` type and the `?` operator make exhaustive error handling the path of least resistance. Every error in cmod is typed, contextual, and produces a clear message with an exit code.

### Safe concurrency
cmod compiles modules in parallel, respecting DAG ordering. Rust's ownership system guarantees at compile time that our parallel build runner can't have data races. We've never had a concurrency bug — the compiler won't let us.

### Single binary distribution
Rust compiles to a single static binary with no runtime dependencies. No Python version conflicts, no shared library issues, no JVM to install.

## The Workspace Architecture

cmod is organized as a Cargo workspace with 7 crates:

| Crate | Responsibility |
|---|---|
| `cmod-cli` | CLI frontend, subcommand dispatch |
| `cmod-resolver` | Git operations, semver solving |
| `cmod-build` | DAG construction, Clang invocation |
| `cmod-cache` | Content-addressed artifact cache |
| `cmod-workspace` | Monorepo management |
| `cmod-security` | Verification, trust model |
| `cmod-core` | Core types, config parsing, error model |

## Key Libraries

- **clap** — CLI argument parsing with derive macros
- **serde + toml** — TOML serialization/deserialization
- **semver** — Semantic version parsing and constraint matching
- **sha2** — SHA-256 hashing for cache keys and verification
- **git2** — libgit2 bindings for Git operations
- **petgraph** — Graph data structures for the module DAG

## Lessons Learned

1. **The type system catches design errors.** Introducing `ModuleId` newtypes eliminated an entire class of string-confusion bugs.
2. **Test infrastructure is excellent.** 270+ tests running in seconds with `cargo test`.
3. **Cross-compilation works.** CI builds binaries for Linux, macOS, and Windows from a single workflow.
4. **Interacting with C++ tooling requires care.** Parsing Clang output and `clang-scan-deps` JSON required careful string handling.
5. **The Rust community is welcoming.** Contributors span both C++ and Rust communities.

## Would We Choose Rust Again?

Absolutely. Zero runtime crashes, no concurrency bugs, single-binary distribution, excellent test infrastructure, and compiler-enforced correctness that lets us refactor fearlessly.

[Explore the cmod source code →](https://github.com/satishbabariya/cmod)
