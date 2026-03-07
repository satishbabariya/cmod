# Introducing cmod: A Cargo-Inspired Package Manager for Modern C++

*C++ has modules now. It deserves a build tool that knows it.*

---

C++ powers the world's most critical software — operating systems, game engines, databases, compilers, embedded systems. Yet it remains the only major programming language without a standard, unified package manager.

Rust has Cargo. Go has `go mod`. Python has pip. JavaScript has npm. C++ has... a fragmented landscape of CMake scripts, Conan recipes, vcpkg manifests, and Bazel BUILD files — none of which understand C++20 modules natively.

Today, we're introducing **cmod**: a Cargo-inspired, Git-native package and build tool built from the ground up for modern C++20 modules.

## The Problem

If you've worked on a C++ project of any significant size, you know the pain:

- **No standard package manager.** Every team reinvents dependency management with shell scripts, Git submodules, or third-party tools that require their own ecosystems.
- **Header-based compilation is fundamentally broken.** Textual `#include` means redundant parsing, macro pollution, and non-deterministic builds. A single header change can trigger a full rebuild.
- **Existing tools don't understand modules.** C++20 introduced modules three years ago, but the tooling ecosystem never caught up. CMake treats modules as an afterthought. Conan and vcpkg are still header-first.
- **Reproducibility is optional.** Without mandatory lockfiles and pinned toolchains, "works on my machine" remains the norm.

## What cmod Does Differently

### Git Is Your Registry

cmod doesn't require a central package server. Module identity is bound directly to Git URLs:

```toml
[dependencies]
"github.com/fmtlib/fmt" = ">=10.0"
"github.com/nlohmann/json" = "^3.11"
```

Want to publish a C++ module? Push a Git tag. That's it. No accounts to create, no packages to upload, no approval queues. Your repository *is* your package.

### Modules Are First-Class

cmod understands C++20 modules, partitions, and Binary Module Interfaces (BMIs) natively. It uses `clang-scan-deps` to discover module dependencies automatically and builds a full dependency graph before compilation begins.

Modules are compiled once into BMIs, enabling genuinely fast incremental builds — not the "reparse every header" approach that existing tools rely on.

### Deterministic by Default

Every `cmod` project has a `cmod.lock` file that pins exact commit hashes and toolchain versions. This isn't optional. Reproducible builds are a guarantee, not a best practice you hope your team follows.

```bash
cmod resolve    # Generate/update the lockfile
cmod build --locked  # Build with exact pinned versions — CI-ready
```

### Cargo-Like Simplicity

If you've used Cargo, cmod will feel immediately familiar:

```bash
cmod init my_project          # Create a new project
cmod add github.com/fmtlib/fmt@10.0  # Add a dependency
cmod build                    # Build
cmod test                     # Run tests
cmod run                      # Run the binary
```

No 200-line CMakeLists.txt. No `conanfile.py` with class inheritance. Just a clean `cmod.toml` and predictable commands.

## A Quick Look

Here's what a `cmod.toml` looks like:

```toml
[package]
name = "my_project"
version = "0.1.0"
edition = "2024"
authors = ["Your Name <you@example.com>"]
license = "MIT"

[module]
name = "com.github.yourname.my_project"
root = "src/my_project.cppm"

[dependencies]
"github.com/fmtlib/fmt" = ">=10.0"

[toolchain]
compiler = "clang"
version = ">=18.0"
std = "c++23"

[build]
type = "binary"
optimization = "2"
```

Clean. Readable. No domain-specific language to learn.

## Architecture

cmod is built in Rust as a modular, layered system:

```
CLI → Dependency Resolver → Workspace Manager → Build Orchestrator → LLVM/Clang → Artifact Cache → Security
```

Each layer has a focused responsibility:

- **cmod-resolver** — Fetches Git repositories, solves semver constraints, generates lockfiles
- **cmod-build** — Constructs the module DAG, plans compilation order, invokes Clang
- **cmod-cache** — Content-addressed artifact caching with SHA-256 keys
- **cmod-workspace** — Monorepo support with unified dependency resolution
- **cmod-security** — Hash verification, trust-on-first-use, SBOM generation

## What's Ready Today

cmod is in active development with Phases 0–2 complete:

- Full CLI with 30+ commands
- Git-based dependency resolution with semver solving
- Mandatory lockfile generation
- LLVM/Clang build backend with module DAG construction
- Workspace and monorepo support
- Local artifact caching
- 750+ passing tests

## What's Coming

- **Phase 3** — Distributed cache, BMI distribution, distributed build workers (complete)
- **Phase 4** — Cryptographic signing (PGP/SSH/Sigstore), auditing, SBOM, policy enforcement (complete)
- **Phase 5** — LSP server, plugin sandboxing, module registry, feature resolution (in progress)

## Get Started

```bash
# Clone and build cmod
git clone https://github.com/nickshouse/cmod.git
cd cmod
cargo build --release

# Create your first project
cmod init hello_world
cd hello_world
cmod build
cmod run
```

Check out the [examples/](https://github.com/nickshouse/cmod/tree/main/examples) directory for real-world project templates — from minimal binaries to workspaces with Git dependencies.

## Why Now?

C++20 modules have been standardized. C++23 and C++26 are pushing the language forward. But the tooling remains stuck in the `#include` era.

cmod is built for the C++ that exists today and the C++ that's coming tomorrow. It treats modules as the fundamental unit of composition, Git as the universal package registry, and developer experience as a first-class concern.

C++ is powerful. Its tooling should be too.

---

*cmod is open source under the Apache-2.0 license. Star the repo, file issues, and join the conversation on [GitHub](https://github.com/nickshouse/cmod).*
