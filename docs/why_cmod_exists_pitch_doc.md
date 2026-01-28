# Why cmod Exists

## The Problem

C++ has:
- No standard package manager
- Fragile header-based builds
- Slow, non-deterministic compilation
- Fragmented tooling

Modern C++ *has modules*, but the ecosystem never caught up.

---

## Existing Tools Fall Short

- **CMake**: build scripts, not dependency management
- **Conan/vcpkg**: binary packages, header-first mindset
- **Bazel**: powerful but heavy and centralized

None are module-native.

---

## cmod’s Answer

- Git-native module identity
- First-class C++20 modules
- Deterministic lockfiles
- Fast incremental & cached builds
- Decentralized by default

cmod treats C++ like a modern language again.

---

## Target Users

- Compiler engineers
- Systems programmers
- Game engines
- Infra-heavy C++ teams

---

## Long-Term Vision

cmod becomes:
- The default way to consume C++ modules
- A foundation for future C++ tooling
- A bridge between compilers and developers
