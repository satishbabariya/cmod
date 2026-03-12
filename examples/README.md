# cmod Examples

Working reference projects demonstrating cmod features and conventions.

## Examples

| Example | Description | Key concepts |
|---|---|---|
| [hello](hello/) | Minimal binary, no dependencies | `cmod.toml` basics, module interface + implementation units, `cmod build`, `cmod run` |
| [library](library/) | Static library with module partitions | Partitions (`:ops`, `:stats`), `export import :partition;`, `cmod test` |
| [shared-lib](shared-lib/) | Shared (dynamic) library | `type = "shared-lib"`, platform-correct extension (`.dylib`/`.so`/`.dll`) |
| [with-deps](with-deps/) | Git dependencies (fmt + json) | `cmod add`, semver constraints, branch pinning, `cmod.lock`, `--locked` |
| [workspace](workspace/) | Multi-member monorepo | `[workspace]`, inter-member path deps, `{ workspace = true }`, shared lockfile |
| [path-deps](path-deps/) | Local path dependencies | `path = "libs/..."`, co-located library development, `cmod deps --tree` |
| [nested-deps](nested-deps/) | Path dep with its own git dep | Transitive dependency chains, path dep lockfile propagation |
| [header-only](header-only/) | Header-only library as path dep | `include/` convention, no compilable sources, global module fragment |
| [include-dirs](include-dirs/) | Project using `include/` convention | Auto-detected `-I` flags, mixing headers with modules |
| [ixx-modules](ixx-modules/) | `.ixx` module extension (MSVC) | Non-`.cppm` extensions, `-x c++-module` flag |
| [multi-binary](multi-binary/) | Multiple binaries from one module | Shared module library, multiple `main()` entry points |
| [with-tests](with-tests/) | Testing with `cmod test` | `[test]` configuration, `tests/` directory convention, standalone test binaries |
| [plugin](plugin/) | Plugin system | `[plugins]`, `plugin.toml` manifest, JSON IPC protocol, `cmod plugin run` |

## Getting started

Each example is a self-contained cmod project. To try one:

```bash
cd examples/hello
cmod build
cmod run
```

For examples with dependencies (`with-deps`, `workspace`, `nested-deps`), resolve dependencies first:

```bash
cmod resolve
cmod build
```

## Prerequisites

- **cmod** installed (`cargo install --path crates/cmod-cli` from the repo root)
- **Clang 17+** with C++20 module support

## Fork dependencies

The `with-deps`, `workspace`, and `nested-deps` examples depend on:

- [cmod-ecosystem/fmt](https://github.com/cmod-ecosystem/fmt/tree/cmod-support) — {fmt} with cmod support
- [cmod-ecosystem/json](https://github.com/cmod-ecosystem/json/tree/cmod-support) — nlohmann/json with cmod support
