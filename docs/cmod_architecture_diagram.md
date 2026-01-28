# cmod Architecture (Logical View)

```
┌────────────────────────────┐
│        User / IDE          │
│  (CLI, LSP, Plugins)       │
└──────────────┬─────────────┘
               │
┌──────────────▼─────────────┐
│          cmod CLI          │
│  Command Parsing & UX      │
└──────────────┬─────────────┘
               │
┌──────────────▼─────────────┐
│      Dependency Resolver   │
│  - Git resolution          │
│  - Version constraints     │
│  - Lockfile generation     │
└──────────────┬─────────────┘
               │
┌──────────────▼─────────────┐
│      Workspace Manager     │
│  - Monorepos               │
│  - Module graph            │
└──────────────┬─────────────┘
               │
┌──────────────▼─────────────┐
│      Build Orchestrator    │
│  - DAG execution           │
│  - Incremental builds      │
└──────────────┬─────────────┘
               │
┌──────────────▼─────────────┐
│    LLVM / Clang Toolchain  │
│  - Module compilation      │
│  - BMI generation          │
└──────────────┬─────────────┘
               │
┌──────────────▼─────────────┐
│        Artifact Cache      │
│  - Local                   │
│  - Remote (optional)       │
└──────────────┬─────────────┘
               │
┌──────────────▼─────────────┐
│  Security & Verification   │
│  - Hashing                 │
│  - Signing                 │
│  - Lock enforcement        │
└────────────────────────────┘
```

---

## Key Data Flows

1. **Resolution Flow**
   - `cmod.toml` → Dependency graph → `cmod.lock`

2. **Build Flow**
   - Lockfile → Build DAG → LLVM invocations → Artifacts

3. **Cache Flow**
   - Cache key → Local cache → Remote cache (optional)

4. **Security Flow**
   - Git commit → Hash verification → Optional signature checks

---

## Extensibility Points

- CLI plugins (RFC-0018)
- IDE/LSP integrations
- Remote cache backends
- Security providers (PGP, Sigstore)

