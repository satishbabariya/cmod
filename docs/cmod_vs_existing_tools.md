# cmod vs Existing C++ Tools

## High-Level Comparison

| Feature | cmod | CMake | Conan | vcpkg | Bazel |
|------|------|------|------|-------|-------|
| C++20 Modules | ✅ Native | ❌ | ❌ | ❌ | ⚠️ Partial |
| Git-native deps | ✅ | ❌ | ❌ | ❌ | ❌ |
| Lockfiles | ✅ | ❌ | ⚠️ | ⚠️ | ✅ |
| Monorepos | ✅ | ⚠️ | ❌ | ❌ | ✅ |
| Remote cache | ✅ | ❌ | ❌ | ❌ | ✅ |
| Central registry | ❌ | ❌ | ✅ | ✅ | ⚠️ |

---

## Key Differentiators

### vs CMake
- cmod manages *modules and dependencies*
- CMake generates build files

### vs Conan / vcpkg
- cmod is source-first and module-native
- No global binary registry

### vs Bazel
- cmod is lightweight and Git-native
- No proprietary rule language

---

## When NOT to Use cmod

- Legacy C++98/11 codebases
- Non-module projects
- Teams requiring centralized governance

---

## When cmod Shines

- Modern C++20+ code
- Large dependency graphs
- CI-heavy environments
- Compiler-aligned workflows

