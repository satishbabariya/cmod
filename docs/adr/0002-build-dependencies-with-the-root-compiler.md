# ADR 0002: Build every dependency with the root package's compiler

- Status: Proposed
- Date: 2026-09-30
- Issue: #111

## Context

`cmod build` builds git dependencies in `build_vendored_dependencies` and path
dependencies in `build_path_dependencies`, both in
`crates/cmod-cli/src/commands/build.rs`. Each one loaded the dependency's
`cmod.toml` and picked the backend from that file's `[toolchain] compiler`.

Every cmod-ecosystem port says `compiler = "clang"`, because that is what
`cmod init` writes. A consumer with `compiler = "gcc"` and `CXX=g++-14`
therefore built each port with `ClangBackend`, and `ClangBackend` runs `$CXX`.
g++ got clang's command line and stopped at the first flag:

```
g++-14: error: unrecognized command-line option '--target=x86_64-unknown-linux-gnu'
g++-14: error: unrecognized command-line option '--precompile'; did you mean '--compile'?
```

With `CXX` unset the build would have gone further and still been wrong. Clang
would build the dependency and write `.pcm` files, and g++ cannot read them.
A BMI is only usable by the compiler that wrote it, so the importer and every
module it imports must use one compiler.

The GCC job in CI passed only because it rewrote each fetched dependency's
`cmod.toml` with `sed` after `cmod resolve`.

The lockfile already records it this way. The resolver writes the root
manifest's `[toolchain]` into each package's `toolchain` entry, not the
dependency's. The build did not follow what the lock said.

## Decision

The root package's compiler builds the whole graph.

- The root compiler is the root manifest's `[toolchain] compiler`, or clang if
  it is not set. That is the same rule `cmod build` uses for the root.
- Before a git or path dependency is built, its loaded `Config` gets the root
  compiler in `[toolchain] compiler`. Nothing is written to disk. A path
  dependency passes the root compiler on to its own path and git
  dependencies, because it is built with the same function as the root.
- Only the compiler family is taken from the root. The dependency keeps its
  own `cxx_standard`, `stdlib`, `sysroot`, `[build]` flags and include
  directories, as before.
- A dependency's `compiler` is no longer a choice. When it names a different
  compiler from the root, `cmod build --verbose` prints a note that names the
  dependency, the compiler it declares and the one used. It is not a warning:
  every ecosystem port declares clang, so a warning would fire on every GCC
  build and mean nothing.
- Workspace members already used the workspace root's config. They do not
  change.

## Consequences

- **Cache keys change for some dependencies.** The compiler family is part
  of `CompilerBackend::fingerprint`, and the fingerprint is part of every
  cache key. A dependency whose manifest names the same compiler as the root
  keeps its key. For the usual case, a clang root with clang ports, nothing
  changes. A dependency whose manifest names a different compiler gets a new
  key, so its first build after upgrading is a cold one. Before this change
  such a build either failed, as above, or produced BMIs the root could not
  import. The one case that worked is a dependency with no module units whose
  objects linked across compilers. It is now rebuilt once with the root's
  compiler.
- **The key format does not change.** `CacheKey`, the fingerprint layout, and
  the cache directory layout are all as before. Only the input value (which
  backend builds the dependency) differs.
- **The lockfile does not change.** Its format and content are unchanged. The
  build now does what the lock's `toolchain` entries already said.
- **The module DAG does not change.** Only the backend building each node
  differs.
- **Signing and SBOM output do not change.** Neither reads the dependency's
  compiler.
- The CI `sed` workaround for fetched dependencies is removed. The GCC job
  builds fmt, json, spdlog and Catch2 from their manifests as published.
- A dependency cannot say "this only builds with clang". If that is needed
  later, it should be an explicit list of supported compilers that
  `cmod build` checks and rejects with an error before compiling. It should
  not be the existing `compiler` field, which every port sets to clang by
  default.
- MSVC consumers of static-lib ports now get `cl` instead of a clang command
  line. Whether those ports build under MSVC depends on their `[build]` flags,
  which is separate from this change.
