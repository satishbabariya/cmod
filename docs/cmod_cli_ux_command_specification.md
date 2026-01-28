# cmod CLI UX & Command Specification

## CLI Philosophy

- Predictable, Cargo-inspired commands
- Minimal flags, strong defaults
- Workspace-aware by default

---

## Core Commands

### `cmod init`
Initialize a new module or workspace.

```
cmod init
cmod init --workspace
```

---

### `cmod add`
Add a dependency.

```
cmod add github.com/acme/math-core
cmod add github.com/acme/math-core@^1.2
```

---

### `cmod remove`
Remove a dependency.

```
cmod remove math-core
```

---

### `cmod build`
Build the current module or workspace.

```
cmod build
cmod build --locked
```

---

### `cmod test`
Run module tests.

```
cmod test
```

---

### `cmod resolve`
Resolve dependencies and generate lockfile.

```
cmod resolve
```

---

### `cmod update`
Update dependencies.

```
cmod update
cmod update fmt
```

---

### `cmod cache`
Manage caches.

```
cmod cache status
cmod cache clean
```

---

### `cmod verify`
Verify security and integrity.

```
cmod verify
cmod verify --signatures
```

---

### `cmod deps`
Inspect dependency graph.

```
cmod deps
cmod deps --tree
```

---

## Global Flags

- `--locked`
- `--offline`
- `--verbose`
- `--target <triple>`

---

## Exit Codes

- `0` success
- `1` build failure
- `2` resolution error
- `3` security violation

---

## Future Commands

- `cmod publish`
- `cmod doctor`
- `cmod audit`

---

## UX Guarantees

- No silent dependency changes
- Clear error messages
- Deterministic behavior

