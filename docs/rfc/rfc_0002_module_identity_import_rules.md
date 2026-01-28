# RFC-0002: Module Identity & Import Rules

## Status
Draft

## Summary
This RFC defines **canonical module identity rules** for `cmod`, ensuring that every C++ module name is:

- Globally unique
- Deterministic
- Derivable from its Git repository
- Resistant to squatting and accidental collisions

The model is inspired by **Go modules**, adapted for **C++20 modules** and LLVM-based tooling.

---

## Motivation

C++ modules introduce a *global namespace problem*:

```cpp
export module math;
```

Without strict rules, this leads to:
- Name collisions
- Ambiguous imports
- Undetectable ODR violations
- Insecure dependency substitution

`cmod` solves this by **binding module identity to source provenance**.

---

## Design Principles

1. **Identity is explicit** — no implicit or inferred names
2. **Git URL is the root of trust**
3. **Module names are stable over time**
4. **Forks must rename**
5. **Imports are auditable by inspection**

---

## Canonical Module Identity

### Rule 1: Module Name = Reverse-Domain Git Path

Every public module MUST use a reverse-domain prefix derived from its Git URL.

| Git Repository | Module Prefix |
|--------------|---------------|
| https://github.com/fmtlib/fmt | `github.fmtlib.fmt` |
| https://git.satyavis.com/math/core | `com.satyavis.math.core` |
| ssh://git@gitlab.com/org/infra/log | `gitlab.org.infra.log` |

```cpp
export module github.fmtlib.fmt;
```

This rule is **mandatory** for published modules.

---

### Rule 2: One Root Module per Repository

Each Git repository defines **exactly one root module**.

```text
repo = github.com/fmtlib/fmt
root = github.fmtlib.fmt
```

Submodules are expressed via partitions or submodules.

---

## Partitions and Submodules

### Partitions (Preferred)

```cpp
export module github.fmtlib.fmt:core;
export module github.fmtlib.fmt:format;
```

Rules:
- Partitions MUST belong to the same repository
- Partitions cannot be imported directly across repos
- Partitions are private unless explicitly exported

---

### Submodules (Optional)

```cpp
export module github.fmtlib.fmt.io;
```

Rules:
- Submodules MUST reside under the same Git root
- Submodules map to subdirectories
- Avoid unless semantic separation is required

---

## Import Rules

```cpp
import github.fmtlib.fmt;
import github.fmtlib.fmt:core;
```

Rules:
- Imports MUST be fully qualified
- No relative or shorthand imports
- No aliasing at the language level

---

## Forks and Mirrors

### Fork Rule

Forked repositories **MUST change their module prefix**.

Example:

```text
Original: github.fmtlib.fmt
Fork:     github.yourname.fmt
```

`cmod` enforces this via Git remote validation.

---

### Mirrors

Mirrors MAY retain the original module name **only if**:
- Commit hashes are identical
- The mirror is declared as trusted

```toml
[trust]
mirrors = ["https://mirror.company.com/fmt"]
```

---

## Local and Private Modules

### Local Modules

Local-only modules MAY omit the domain prefix:

```cpp
export module local.utils;
```

Rules:
- Cannot be published
- Cannot be depended upon externally

---

### Private Organization Modules

```cpp
export module com.company.project.auth;
```

Rules:
- Must use a domain owned by the organization
- Enforced during `cmod publish`

---

## Standard Library Modules

Standard modules are reserved:

```text
std.*
stdx.*
```

Rules:
- Cannot be defined by user packages
- Compiler-provided only

---

## Validation & Enforcement

`cmod check` validates:

- Git URL ↔ module prefix match
- One-root-module rule
- No forbidden prefixes
- No cross-repo partition imports

Violations are **hard errors**.

---

## Security Implications

This model prevents:
- Dependency confusion attacks
- Module squatting
- Silent fork substitution

Every import implies a verifiable Git origin.

---

## Comparison

| System | Global Names | Provenance-bound |
|------|-------------|------------------|
| C++ (raw) | ❌ | ❌ |
| Java | ✅ | ⚠️ |
| Go | ✅ | ✅ |
| Rust | ✅ | Central |
| **cmod** | ✅ | ✅ (Git-native) |

---

## Open Questions

- Should submodules be discouraged entirely?
- Domain verification mechanisms?
- Migration path for legacy codebases?

---

## Conclusion

C++ modules require **strong identity guarantees**.

By binding module names to Git provenance, `cmod` provides:
- Global uniqueness
- Human-readable imports
- Supply-chain security

This RFC establishes the foundation for all higher-level tooling.

