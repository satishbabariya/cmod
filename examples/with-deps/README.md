# with-deps

Example project using Git-based dependencies.

## What this demonstrates

- Declaring Git dependencies in `cmod.toml`
- Semver version constraints (`^0.1`)
- Branch-pinned dependencies (`branch = "develop"`)
- Dependency resolution with `cmod resolve`
- Reproducible builds with `cmod.lock` and `--locked`

## Project structure

```
with-deps/
├── cmod.toml       # Manifest with [dependencies]
└── src/
    ├── lib.cppm    # import fmt; import nlohmann.json;
    └── main.cpp    # import local.with_deps;
```

## Dependencies

| Dependency | Git URL | Constraint | Module name |
|---|---|---|---|
| fmt-cmod | `github.com/satishbabariya/fmt-cmod` | `^0.1` | `fmt` |
| json-cmod | `github.com/satishbabariya/json-cmod` | `^0.1` (branch: develop) | `nlohmann.json` |

## Usage

```bash
cd examples/with-deps

# Resolve dependencies (fetches Git repos, generates cmod.lock)
cmod resolve

# Build the project
cmod build

# Run
cmod run
```

Expected output:

```json
{
  "age": 30,
  "greeting": "Hello, Alice! You are 30 years old.",
  "name": "Alice"
}
```

## Key concepts

- **Git dependencies**: cmod fetches source directly from Git repositories. No binary registry needed.
- **Semver constraints**: `"^0.1"` means any compatible version `>=0.1.0, <0.2.0`.
- **Branch pinning**: `{ version = "^0.1", branch = "develop" }` resolves from a specific branch.
- **Lockfile**: `cmod resolve` generates `cmod.lock` pinning exact commit hashes. Use `--locked` to fail if the lockfile is outdated.
- **Module names**: dependencies export their own module names (`fmt`, `nlohmann.json`) which you import directly.
