# cmod User Guide

Complete documentation for **cmod** — a Cargo-inspired, Git-native package and build tool for C++20 modules.

## Quick Navigation

### Getting Started

- **[Getting Started](getting-started.md)** — Installation, first project, key concepts

### Reference

- **[Configuration Reference](configuration.md)** — Complete `cmod.toml` reference (all sections and fields)
- **[CLI Reference](cli-reference.md)** — Every command, flag, and option
- **[Toolchains](toolchains.md)** — Compilers, C++ standards, cross-compilation

### Guides

- **[Dependencies](dependencies.md)** — Version constraints, lockfiles, vendoring, features
- **[C++20 Modules](modules.md)** — Module naming, partitions, build graph
- **[Building](building.md)** — Build profiles, parallelism, hooks, IDE integration
- **[Testing](testing.md)** — Test discovery, frameworks, coverage, sanitizers, CI integration
- **[Workspaces](workspaces.md)** — Monorepos, shared dependencies, multi-member builds
- **[Caching](caching.md)** — Local and remote build cache management
- **[Security](security.md)** — Trust model, verification, signing, auditing
- **[Publishing](publishing.md)** — Releasing, signing tags, SBOM generation

### Learning

- **[Examples](examples.md)** — Walkthrough of all example projects

## Quick Reference

### Common Commands

```bash
cmod init                      # Initialize a new project
cmod build                     # Build (debug)
cmod build --release           # Build (release)
cmod test                      # Run tests
cmod run                       # Build and run
cmod add "<dep>@<version>"     # Add a dependency
cmod resolve                   # Resolve dependencies and generate lockfile
cmod update                    # Update dependencies
cmod clean                     # Remove build artifacts
```

### CI/CD Commands

```bash
cmod build --locked --verify   # Strict build (fail if lockfile outdated or tampered)
cmod test --release            # Run tests in release mode
cmod audit                     # Audit dependencies for security issues
cmod sbom -o sbom.json         # Generate SBOM
```

### Project Info

```bash
cmod status                    # Project overview
cmod deps --tree               # Dependency tree
cmod toolchain show            # Active toolchain info
cmod graph                     # Module dependency graph
```

## Exit Codes

| Code | Meaning |
|------|---------|
| `0` | Success |
| `1` | Build failure |
| `2` | Resolution error |
| `3` | Security violation |
