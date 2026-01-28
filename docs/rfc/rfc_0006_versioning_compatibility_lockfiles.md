# RFC-0006: Versioning, Compatibility & Lockfiles

## Status
Draft

## Summary
This RFC defines how **cmod** handles module versioning, compatibility guarantees, dependency resolution, and reproducible builds via lockfiles — inspired by Go modules and Cargo, but adapted to **C++ modules + ABI realities**.

The goals are:
- Reproducible builds
- Predictable upgrades
- Explicit ABI and compiler compatibility
- Minimal central coordination (Git-native)

---

## Motivation

C++ lacks:
- Stable ABI across compilers
- Unified module version semantics
- Reproducible dependency resolution

Without strong rules, module-based ecosystems fragment quickly.

**cmod** treats versioning as a *build contract*, not just a semantic label.

---

## Version Scheme

### Module Version Format

```text
MAJOR.MINOR.PATCH
```

Rules:
- **MAJOR**: breaking API *or* ABI change
- **MINOR**: backward-compatible API additions
- **PATCH**: bug fixes, no interface change

This aligns with SemVer, but ABI is first-class.

---

## Version Declaration

Each module declares its version in `cmod.toml`:

```toml
[module]
name = "github.com/acme/math"
version = "1.4.2"

[compat]
cpp = ">=20"
llvm = ">=17"
abi = "itanium"
```

---

## Compatibility Dimensions

cmod tracks compatibility across **multiple axes**:

| Dimension | Examples |
|--------|---------|
| C++ standard | c++20, c++23 |
| Compiler | clang-17, clang-18 |
| ABI | itanium, msvc |
| Platform | linux-x86_64, macos-arm64 |
| Stdlib | libc++, libstdc++ |

These constraints participate in resolution.

---

## Dependency Constraints

Dependencies are declared with **ranges**, not exact pins:

```toml
[deps]
github.com/acme/math = "^1.3"
github.com/acme/geo = ">=0.9,<2.0"
```

Rules:
- `^1.3` → `>=1.3.0,<2.0.0`
- Pre-1.0 versions are treated conservatively

---

## Resolution Algorithm (High-level)

1. Read root `cmod.toml`
2. Fetch dependency graphs (Git refs only)
3. Filter by compatibility constraints
4. Select **highest compatible version**
5. Emit a lockfile

Resolution is **deterministic**.

---

## Lockfile: `cmod.lock`

Generated automatically by `cmod resolve` / `cmod build`.

Example:

```toml
[[package]]
name = "github.com/acme/math"
version = "1.4.2"
commit = "a13f9c2"
source = "git"
compiler = "clang-18"
stdlib = "libc++"
platform = "macos-arm64"

[[package]]
name = "github.com/acme/geo"
version = "0.9.4"
commit = "e92bb11"
```

Properties:
- Checked into VCS
- Exact commits pinned
- Includes toolchain identity

---

## Upgrade Behavior

| Command | Behavior |
|------|---------|
| `cmod build` | Uses lockfile |
| `cmod update` | Re-resolves allowed ranges |
| `cmod update foo` | Update single dep |
| `cmod tidy` | Remove unused deps |

---

## ABI Break Detection

cmod MAY integrate with:
- LLVM IR diffing
- Module interface hash comparison
- Exported symbol set comparison

If detected:
- MAJOR bump required
- Warning or hard error

(Exact mechanism TBD)

---

## Multiple Versions in Graph

Unlike Cargo, **cmod forbids multiple versions** of the same module in a build graph.

Reason:
- ODR violations
- ABI mismatch risk
- Module import ambiguity

Conflicts must be resolved explicitly.

---

## Vendoring

Optional vendoring mode:

```bash
cmod vendor
```

- Copies resolved modules into `vendor/`
- Lockfile still authoritative
- Useful for hermetic or offline builds

---

## Reproducibility Guarantees

Given:
- Same `cmod.lock`
- Same compiler version
- Same platform

Result:
- Bit-for-bit identical module artifacts (best-effort)

---

## Prior Art

- Go Modules
- Cargo.lock
- Bazel toolchains
- Conan profiles

---

## Open Questions

- Should MSVC ABI be supported in v1?
- How strict should compiler patch version matching be?
- Should stdlib be versioned explicitly?

---

## Next RFCs

- RFC-0007: Build Graph & Incremental Compilation
- RFC-0008: Toolchain & Cross-compilation
- RFC-0009: Security, Trust & Checksums

