# cmod vs CMake, Conan, vcpkg, and Bazel: An Honest Comparison

*Every C++ tool has strengths. Here's where cmod fits in — and where it doesn't.*

---

Choosing a build tool for C++ is a high-stakes decision. Migrating later is painful. So we want to be upfront about what cmod does well, what the alternatives do well, and where each falls short.

This isn't a takedown of existing tools. CMake, Conan, vcpkg, and Bazel have served the C++ community for years and continue to be excellent choices for many use cases. cmod exists because C++20 modules created a gap that none of them fully address.

## The Comparison Matrix

| Feature | cmod | CMake | Conan | vcpkg | Bazel |
|---------|------|-------|-------|-------|-------|
| **C++20 module support** | Native | Experimental | None | Partial | Partial |
| **Package management** | Built-in (Git) | None (external) | Yes (registry) | Yes (registry) | Yes (rules) |
| **Dependency resolution** | Semver + Git tags | Manual / FetchContent | Semver + recipes | Versioned manifests | BUILD rules |
| **Lockfile** | Mandatory | None | Optional | Partial | Implicit |
| **Configuration format** | TOML | CMake DSL | Python / TOML | JSON / TOML | Starlark |
| **Registry model** | Decentralized (Git) | N/A | Centralized | Centralized | Custom |
| **Workspace / monorepo** | Native | Subdirectories | Workspaces | Overlay ports | Native |
| **Artifact caching** | Content-addressed | ccache (external) | Binary packages | Binary caching | Remote execution |
| **Security verification** | Built-in (TOFU + sigs) | None | Package signing | SHA verification | Hermetic |
| **Setup complexity** | Low | Medium | Medium-High | Medium | High |
| **Learning curve** | Cargo-like | Steep CMake DSL | Moderate | Moderate | Steep Starlark |

## cmod vs CMake

**CMake** is the de facto standard for C++ build generation. It's supported by virtually every IDE, CI system, and platform. That ecosystem advantage is enormous and shouldn't be underestimated.

### Where CMake excels
- **Universal platform support.** CMake generates build files for Make, Ninja, Visual Studio, Xcode, and more.
- **Mature ecosystem.** Thousands of `FindXxx.cmake` modules and decades of community knowledge.
- **IDE integration.** Every major C++ IDE supports CMake projects natively.

### Where cmod improves on CMake
- **Dependency management.** CMake has no built-in package manager. `FetchContent` and `ExternalProject` are workarounds, not solutions. cmod resolves, fetches, and pins dependencies automatically.
- **Lockfiles.** CMake has no concept of a lockfile. The same `CMakeLists.txt` can produce different dependency versions on different machines.
- **C++20 modules.** CMake's module support is experimental and requires manual configuration. cmod discovers module dependencies automatically via `clang-scan-deps`.
- **Simplicity.** A `cmod.toml` is 15 lines. An equivalent `CMakeLists.txt` is often 50–100+ lines with arcane syntax.

### When to choose CMake over cmod
- You need to support compilers beyond Clang (MSVC, GCC) today
- You have a large existing CMake codebase and migration cost is too high
- You need to generate build files for IDEs that only support CMake

### Interoperability
cmod includes `cmod compile-commands` for IDE integration and plans for `cmod emit-cmake` to generate `CMakeLists.txt` for projects that need CMake compatibility.

## cmod vs Conan

**Conan** is the most established C++ package manager, with a large central repository of pre-built binaries and a flexible recipe system.

### Where Conan excels
- **Large package ecosystem.** ConanCenter has thousands of packages with pre-built binaries for common platforms.
- **Binary management.** Conan excels at distributing pre-compiled binaries, avoiding build-from-source overhead.
- **Flexibility.** Conan recipes (Python) can handle complex build configurations, patching, and cross-compilation.

### Where cmod improves on Conan
- **No central registry required.** cmod uses Git URLs directly. No need to create Conan recipes, submit to ConanCenter, or run a private Conan server.
- **Module-native.** Conan is header-first. It doesn't understand C++20 modules, BMIs, or module partitions.
- **Simpler configuration.** `cmod.toml` vs `conanfile.py` — TOML vs Python class hierarchies.
- **Mandatory lockfiles.** Conan's lockfiles are optional and not enforced by default.
- **No Python dependency.** cmod is a single Rust binary.

### When to choose Conan over cmod
- You need pre-built binaries for a wide range of platforms immediately
- Your dependencies are primarily header-only libraries that aren't on Git
- You need the flexibility of Python-based build recipes

## cmod vs vcpkg

**vcpkg** is Microsoft's C++ package manager, deeply integrated with Visual Studio and MSBuild.

### Where vcpkg excels
- **Microsoft ecosystem integration.** vcpkg works seamlessly with Visual Studio, MSBuild, and Azure DevOps.
- **Binary caching.** vcpkg's binary caching reduces CI build times significantly.
- **Low barrier for Windows developers.** If you're already in the Microsoft toolchain, vcpkg is frictionless.

### Where cmod improves on vcpkg
- **Decentralized.** vcpkg relies on a centralized port registry. cmod uses Git URLs — no gatekeeping.
- **Module-native.** vcpkg's module support is limited and experimental.
- **Cross-platform from day one.** cmod doesn't favor any vendor's toolchain.
- **Stronger reproducibility.** cmod's mandatory lockfiles with exact commit hashes provide stronger guarantees than vcpkg's version constraints.

### When to choose vcpkg over cmod
- Your team is primarily on Windows with Visual Studio
- You need tight MSBuild / Azure DevOps integration
- You want a Microsoft-supported solution with commercial backing

## cmod vs Bazel

**Bazel** is Google's build system, designed for massive monorepos with hermetic, reproducible builds.

### Where Bazel excels
- **Hermeticity.** Bazel's sandbox ensures builds are truly isolated and reproducible.
- **Scale.** Bazel handles monorepos with millions of lines of code and remote execution across build farms.
- **Language-agnostic.** Bazel builds C++, Java, Python, Go, and more in a single workspace.

### Where cmod improves on Bazel
- **C++-native.** cmod understands C++ modules, BMIs, and the Clang toolchain natively. Bazel treats C++ as one of many languages.
- **Simpler setup.** A `cmod.toml` vs Bazel's `WORKSPACE`, `BUILD`, and Starlark rules.
- **Git-native dependencies.** Bazel's dependency model requires explicit `http_archive` rules. cmod resolves from Git tags automatically.
- **Lower barrier to entry.** You don't need to learn Starlark or understand Bazel's execution model to build a C++ project.

### When to choose Bazel over cmod
- You have a polyglot monorepo (C++ + Java + Python + Go)
- You need remote build execution across a cluster
- You're already invested in the Bazel ecosystem

## What cmod Doesn't Do (Yet)

We believe in being honest about limitations:

- **Compiler support is LLVM/Clang-first.** GCC and MSVC support is planned but not implemented.
- **No pre-built binary distribution.** cmod builds from source. Distributed caching (Phase 3) will address team-wide build sharing.
- **Smaller ecosystem.** cmod is new. It doesn't have Conan's package catalog or CMake's 20-year ecosystem.
- **No IDE project generation.** cmod generates `compile_commands.json` but doesn't create Visual Studio solutions or Xcode projects.

## The cmod Sweet Spot

cmod is the right choice when:

1. **You're starting a new C++20+ project** and want modern tooling from day one
2. **You care about C++20 modules** and want a tool that understands them natively
3. **You prefer decentralized dependencies** over centralized registries
4. **You value simplicity** and want a Cargo-like experience for C++
5. **Reproducibility is non-negotiable** and you want mandatory lockfiles
6. **You're building with Clang** and want deep LLVM integration

## Try It Yourself

The best way to evaluate a tool is to use it:

```bash
git clone https://github.com/nickshouse/cmod.git
cd cmod && cargo build --release

cmod init my_project
cd my_project
cmod add github.com/fmtlib/fmt@10.0
cmod build
cmod run
```

Compare that to your current workflow and decide for yourself.

---

*cmod is open source under Apache-2.0. We welcome feedback, comparisons, and honest critique — [join the conversation on GitHub](https://github.com/nickshouse/cmod).*
