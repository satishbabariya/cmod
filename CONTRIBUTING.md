# Contributing to cmod

Thank you for your interest in contributing to cmod! This guide will help you get started.

## Prerequisites

- **Rust 1.80+** — install via [rustup](https://rustup.rs/)
- **LLVM/Clang 17+** — required for C++ module compilation; cmod resolves the compiler via `CXX` → `clang++` on `PATH` (see [compiler detection](docs/guide/toolchains.md#compiler-detection))
- **Git** — for dependency resolution and version control

## Getting Started

```bash
git clone https://github.com/satishbabariya/cmod.git
cd cmod
cargo build
cargo test
git config core.hooksPath .githooks   # enable the fmt/clippy git hooks (recommended)
```

The hooks mirror CI's gates: `pre-commit` runs `cargo fmt --all --check`
(fast), `pre-push` runs `cargo clippy --all-targets -- -D warnings`. Bypass a
single run with `--no-verify` if you must; CI still enforces both.

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
| `cmod-lsp` | LSP server for editor integration |

Dependencies flow downward: `cli -> {resolver, build, cache, workspace, security, lsp} -> core`.

## Code Conventions

- Follow standard Rust idioms (`snake_case`, standard module layout)
- Keep each crate focused on its responsibility
- Prefer extending existing modules over creating new files
- All cross-crate dependencies must flow downward toward `cmod-core`

## Commit Conventions

- **Conventional-commit prefixes**: `fix(scope):`, `feat(scope):`, `docs:`,
  `chore:` — match the existing `git log` style.
- **One logical change per commit.** Bug-fix series get one commit per bug
  (see the `v0.1.0-alpha.2` audit series for the pattern); an unrelated
  drive-by fix goes in its own commit, not folded into the feature.
- **Link the issue**: `Fixes #N` / `Closes #N` in the commit or PR body so
  merges close issues automatically.
- PRs are **squash-merged**; the PR title becomes the commit subject on
  `main`, so write it like a commit subject.

## Pull Request Checklist

Before submitting a PR, please ensure:

- [ ] `cargo test --all` passes
- [ ] `cargo clippy --all-targets -- -D warnings` reports no warnings
- [ ] `cargo fmt --all --check` passes
- [ ] New functionality includes tests (written before the fix/feature when
      practical — regression tests should fail on the unfixed code)
- [ ] The PR is attached to the current milestone if it implements a
      tracked issue

## CI

Every PR runs the `CI` and `Examples` workflows:

| Job | What it checks |
|---|---|
| Format | `cargo fmt --all --check` |
| Clippy | `cargo clippy --all-targets -- -D warnings` (latest stable) |
| MSRV (1.80) | `cargo check --all` on the minimum supported Rust |
| Test | `cargo test --all` on ubuntu/macos/windows × stable/nightly |
| E2E Tests | end-to-end CLI validation on ubuntu/macos |
| Examples | builds the `examples/` projects on ubuntu/macos |
| CodeQL | static security analysis |

All jobs must be green before merge; the git hooks above catch the Format
and Clippy failures locally first.

## Releases

Releases follow a branch + changelog pattern (maintainers only):

1. Branch `release/vX.Y.Z`, bump the workspace `version` in `Cargo.toml`
   (regenerating `Cargo.lock`), and document the release in `CHANGELOG.md`.
2. Write user-facing notes in `RELEASE.md` (upgrade notes, behavior changes).
3. PR, merge, then tag `vX.Y.Z` on `main` — the tag triggers the `Release`
   workflow, which builds and publishes multi-platform binaries.

## RFCs

Design decisions are documented as RFCs under `docs/rfc/`. If your change involves architectural decisions or new features, consider referencing or proposing an RFC. See the RFC tiers in `CLAUDE.md` for priority ordering.

## License

By contributing to cmod, you agree that your contributions will be licensed under the [Apache License 2.0](LICENSE).
