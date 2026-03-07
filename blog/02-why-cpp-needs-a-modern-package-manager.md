# Why C++ Still Doesn't Have a Real Package Manager — And How We're Fixing It

*The most widely-used systems language in the world has the worst dependency story. Here's why, and what we're doing about it.*

---

## A Brief History of C++ Dependency Management

In 1998, when the first C++ standard was published, the concept of a "package manager" barely existed. You downloaded tarballs, ran `./configure && make && make install`, and hoped your system's include paths were set up correctly.

Nearly three decades later, the core workflow hasn't fundamentally changed.

Sure, we have tools. CMake became the de facto build system generator. Conan and vcpkg emerged as package managers. Bazel brought hermetic builds from Google's monorepo. But each of these solves a piece of the puzzle while creating new problems:

| Tool | What It Does Well | What It Gets Wrong |
|------|------------------|--------------------|
| **CMake** | Generates build files for any platform | No dependency management. No lockfiles. DSL is notoriously complex. |
| **Conan** | Large package ecosystem | Centralized registry. Header-first. Requires Python. |
| **vcpkg** | Microsoft-backed, binary caching | Centralized manifest. Limited module support. |
| **Bazel** | Hermetic, reproducible builds | Heavy setup. Proprietary rule language. Not C++-native. |

None of them understand C++20 modules. All of them require significant ceremony to get started. And none of them provide the seamless, integrated experience that Rust developers take for granted with Cargo.

## The Three Fundamental Problems

### 1. Headers Are the Root of All Build Evil

The `#include` preprocessor directive is textual substitution. When you write `#include <vector>`, the compiler literally pastes the contents of that file into your translation unit. Every. Single. Time.

This means:
- **Redundant work.** The same header gets parsed thousands of times across a project.
- **Macro pollution.** A macro defined in one header can silently change the meaning of code in another.
- **Fragile ordering.** Include order matters, and getting it wrong produces cryptic errors.
- **No isolation.** There's no module boundary — everything leaks.

C++20 modules fix all of this. A module is compiled once into a Binary Module Interface (BMI), and consumers import the interface — not the source text. It's faster, safer, and deterministic.

But our tools were built for headers. They don't know what to do with modules.

### 2. There's No Universal Identity for C++ Libraries

In Rust, a crate is identified by its name on crates.io. In Go, a module is identified by its Git URL. In C++, a library is identified by... whatever the person who packaged it decided to call it.

Is it `fmt` or `fmtlib` or `libfmt`? Is the Conan package `fmt/10.0.0` or `fmtlib/fmt@10.0.0`? Does the vcpkg port use the same version numbering as the upstream release?

This ambiguity creates real problems: version conflicts, namespace collisions, and an ecosystem where the same library exists under different names in different package managers.

### 3. Reproducibility Is an Afterthought

Ask a C++ developer if their build is reproducible. Most will say "probably." Few can prove it.

Without mandatory lockfiles, pinned compiler versions, and deterministic dependency resolution, C++ builds are inherently non-reproducible. The same `CMakeLists.txt` can produce different binaries on different machines, different days, or different phases of the moon.

This isn't just an inconvenience — it's a security risk. If you can't verify exactly what went into a binary, you can't verify that it hasn't been tampered with.

## How cmod Addresses Each Problem

### Modules as the Unit of Composition

cmod doesn't treat modules as an optional feature bolted onto a header-based system. Modules are the foundation.

When you run `cmod build`, here's what happens:

1. cmod reads your `cmod.toml` manifest
2. It resolves all dependencies from their Git repositories
3. It uses `clang-scan-deps` to discover the full module dependency graph
4. It performs a topological sort to determine optimal compilation order
5. It compiles each module once into a BMI
6. It links the final binary

The key insight: **the full build graph is known before any compilation begins.** This enables parallel compilation, correct ordering, and cached BMIs that skip redundant work.

### Git URLs as Universal Identity

In cmod, a module's identity is its Git URL:

```toml
[dependencies]
"github.com/fmtlib/fmt" = ">=10.0"
"github.com/nlohmann/json" = "^3.11"
"gitlab.com/your-org/internal-lib" = "~2.0"
```

There's no ambiguity. No registry to search. No name squatting. The URL points to exactly one repository, and the version constraint maps to Git tags.

This also means:
- **Private dependencies work the same way.** If you can `git clone` it, cmod can resolve it.
- **No vendor lock-in.** Your dependencies aren't tied to any company's infrastructure.
- **Forking is trivial.** Change the URL, and you're using your fork.

### Lockfiles Are Mandatory

Every cmod project has a `cmod.lock` file that records:

- The exact Git commit hash for every dependency
- The resolved version for every constraint
- The toolchain version used for resolution

This file is checked into version control. When you build with `--locked`, cmod will refuse to proceed if the lockfile doesn't match the manifest. No surprises. No "it worked yesterday."

## The Developer Experience Gap

Here's the real test: how long does it take to go from zero to a working project with an external dependency?

**With CMake + Conan:**
```bash
# Install Conan (requires Python)
pip install conan
# Create conanfile.txt with dependency
# Create CMakeLists.txt (50+ lines for a simple project)
# Run conan install
# Run cmake -B build
# Run cmake --build build
```

That's at least 6 steps, two configuration files, and two different tools to learn.

**With cmod:**
```bash
cmod init my_project
cmod add github.com/fmtlib/fmt@10.0
cmod build
```

Three commands. One configuration file. One tool.

This isn't about being clever or cutting corners. It's about recognizing that developer time is valuable, and ceremony that doesn't add safety or correctness is waste.

## Who Is cmod For?

**Systems programmers** who want fast, deterministic builds without fighting their build system.

**Game engine teams** managing complex dependency graphs across large codebases, who need monorepo support and incremental compilation that actually works.

**Compiler engineers and language enthusiasts** who want a tool that respects what C++20 modules actually are — not a header-compatibility shim.

**Open-source maintainers** who want to publish C++ libraries without creating accounts on package registries or writing packaging metadata in someone else's format.

**Infrastructure teams** who need reproducible CI/CD pipelines and can't afford "works on my machine" failures in production.

## The Road Ahead

cmod's foundation is solid: 30+ CLI commands, Git-based resolution, mandatory lockfiles, LLVM/Clang build backend, workspace support, and artifact caching — all backed by 737+ tests.

What's coming next will make it production-ready for teams of any size:

- **Distributed caching** — Share build artifacts across your team
- **Signature verification** — Cryptographic proof of dependency integrity
- **Plugin SDK** — Extend cmod for your workflow
- **LSP integration** — First-class IDE support

C++ is evolving. Its tooling should evolve with it.

---

*cmod is open source under Apache-2.0. We'd love your feedback — [star the repo](https://github.com/nickshouse/cmod), file issues, and help us build the package manager C++ deserves.*
