# cmod Reference Implementation Skeleton

## Language Choice

**Primary:** Rust
- Excellent CLI ecosystem
- Safe concurrency
- Easy LLVM bindings

**Core build hooks:** Clang / LLVM C++ APIs

---

## Repository Structure

```
cmod/
├─ crates/
│  ├─ cmod-cli/
│  ├─ cmod-core/
│  ├─ cmod-resolver/
│  ├─ cmod-build/
│  ├─ cmod-cache/
│  ├─ cmod-security/
│  └─ cmod-workspace/
├─ docs/
├─ rfcs/
└─ tests/
```

---

## Core Modules

### cmod-core
- Config loading
- Global context
- Error model

### cmod-resolver
- Git fetch
- Version solving
- Lockfile writer

### cmod-build
- Module DAG
- Clang invocation
- Incremental logic

### cmod-cache
- Local cache
- Remote cache client

### cmod-security
- Hashing
- Signature verification

---

## External Integrations

- libgit2
- LLVM/Clang driver
- TOML parser

---

## Minimal Milestone

- `cmod init`
- `cmod add`
- `cmod resolve`
- `cmod build`
