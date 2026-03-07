# cmod Examples

Working reference projects demonstrating cmod features and conventions.

## Examples

| Example | Description | Key concepts |
|---|---|---|
| [hello](hello/) | Minimal binary, no dependencies | `cmod.toml` basics, module interface + implementation units, `cmod build`, `cmod run` |
| [library](library/) | Static library with module partitions | Partitions (`:ops`, `:stats`), `export import :partition;`, `cmod test` |
| [with-deps](with-deps/) | Git dependencies (fmt + json) | `cmod add`, semver constraints, branch pinning, `cmod.lock`, `--locked` |
| [workspace](workspace/) | Multi-member monorepo | `[workspace]`, inter-member path deps, `{ workspace = true }`, shared lockfile |
| [path-deps](path-deps/) | Local path dependencies | `path = "libs/..."`, co-located library development, `cmod deps --tree` |
| [with-tests](with-tests/) | Testing with `cmod test` | `[test]` configuration, `tests/` directory convention, standalone test binaries |

## Getting started

Each example is a self-contained cmod project. To try one:

```bash
cd examples/hello
cmod build
cmod run
```

For examples with dependencies (`with-deps`, `workspace`), resolve dependencies first:

```bash
cmod resolve
cmod build
```

## Prerequisites

- **cmod** installed (`cargo install --path crates/cmod-cli` from the repo root)
- **Clang 17+** with C++20 module support

## Fork dependencies

The `with-deps` and `workspace` examples depend on:

- [satishbabariya/fmt-cmod](https://github.com/satishbabariya/fmt-cmod) — C++20 module wrapper for {fmt}
- [satishbabariya/json-cmod](https://github.com/satishbabariya/json-cmod) — C++20 module wrapper for nlohmann/json
