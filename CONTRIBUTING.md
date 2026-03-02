# Contributing to cmod

Thank you for your interest in contributing to cmod! This guide will help you get started.

## Prerequisites

- **Rust 1.74+** — install via [rustup](https://rustup.rs/)
- **LLVM/Clang 17+** — required for C++ module compilation
- **Git** — for dependency resolution and version control

## Getting Started

```bash
git clone https://github.com/satishbabariya/cmod.git
cd cmod
cargo build
cargo test
```

## Development Commands

```bash
cargo check              # Type-check all crates
cargo build              # Compile all crates
cargo test               # Run all tests
cargo clippy --all-targets -- -D warnings  # Lint
cargo fmt --all --check  # Check formatting
cargo build --release    # Release build
cargo run -- <subcommand>  # Run the CLI
```

## Project Structure

cmod is organized as a Cargo workspace with focused crates:

| Crate | Responsibility |
|---|---|
| `cmod-core` | Core types, config parsing, error model |
| `cmod-cli` | CLI frontend and subcommand dispatch |
| `cmod-resolver` | Git-based dependency resolution |
| `cmod-build` | Module DAG and build orchestration |
| `cmod-cache` | Content-addressed artifact caching |
| `cmod-workspace` | Monorepo and workspace management |
| `cmod-security` | Supply-chain integrity and verification |

Dependencies flow downward: `cli -> {resolver, build, cache, workspace, security} -> core`.

## Code Conventions

- Follow standard Rust idioms (`snake_case`, standard module layout)
- Keep each crate focused on its responsibility
- Prefer extending existing modules over creating new files
- All cross-crate dependencies must flow downward toward `cmod-core`

## Pull Request Checklist

Before submitting a PR, please ensure:

- [ ] `cargo test --all` passes
- [ ] `cargo clippy --all-targets -- -D warnings` reports no warnings
- [ ] `cargo fmt --all --check` passes
- [ ] New functionality includes tests
- [ ] Commit messages are clear and descriptive

## RFCs

Design decisions are documented as RFCs under `docs/rfc/`. If your change involves architectural decisions or new features, consider referencing or proposing an RFC. See the RFC tiers in `CLAUDE.md` for priority ordering.

## License

By contributing to cmod, you agree that your contributions will be licensed under the [Apache License 2.0](LICENSE).
