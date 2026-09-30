# ADR 0001: Compile test binaries through `CompilerBackend`

- Status: Proposed
- Date: 2026-09-30
- Issue: #112

## Context

`cmod build` sends every compile through `dyn CompilerBackend`, built by
`make_backend`. `cmod test` did not. `compile_tests` in
`crates/cmod-cli/src/commands/test.rs` wrote its own clang command line:
`--target=<triple>`, `-fmodule-file=<name>=<path>.pcm`, and the
`-fprofile-instr-generate -fcoverage-mapping` coverage pair. With
`compiler = "gcc"`, g++ stops at the first of these with
`unrecognized command-line option '--target=...'`. No test compiles.

#113 hid this, because `cmod test` exited 0 when nothing compiled. The fix for
#113 makes it a hard failure, so the GCC E2E job needs this change first.

## Decision

Add two methods to `CompilerBackend` in `cmod-build`, and one input type.

```rust
pub struct TestBinary<'a> {
    pub source: &'a Path,
    pub output: &'a Path,
    pub bmis: &'a [(String, PathBuf)],
    pub flags: &'a [String],
    pub objects: &'a [PathBuf],
}

fn test_binary_command(&self, test: &TestBinary<'_>) -> Result<Command, CmodError>;
fn coverage_flags(&self) -> Option<Vec<String>>;
```

`test_binary_command` returns a `Command` that compiles and links one test in
one driver call. The caller runs it and keeps the stderr. The method may write
side files next to `output`.

- **Clang** gives the same argument list `cmod test` built before, in the same
  order: `-std`, `--target`, one `-fmodule-file=` per BMI, the pass-through
  flags, `-o <output> <source>`, then the objects. The program is the
  backend's `clang_path`. That is `$CXX` if set, which matches the old code. If
  `$CXX` is not set, it is `clang++` resolved on `PATH` rather than the bare
  name.
- **GCC** uses the same flags `cmod build` uses for g++: `-std`,
  `-fmodules-ts`, and the profile's `-g -O0` or `-O2 -DNDEBUG`. It writes a
  module mapper to `<output>.map` that lists the test's BMIs and passes
  `-fmodule-mapper=`. It never passes `--target=`.
- **MSVC** keeps the default, which is an error that names the compiler.
  Before this change `cmod test` sent a clang command line to `cl`. The error
  says what is missing.

`coverage_flags` returns `Some` only for clang, because the report step runs
`llvm-profdata` and `llvm-cov`. With gcc or msvc, `cmod test --coverage` fails
before it compiles anything, and the error names the compiler.

`cmod test` builds its backend from `[toolchain] compiler`, `cxx_standard`,
the profile and the target triple. It does not take `[build]` extra flags,
include dirs or feature defines. Test compiles never had them, and keeping
that avoids changing the clang command line.

`collect_bmis` looks for the backend's `bmi_extension()` in the project's BMI
directory. Before, it only looked for `.pcm`.

## Consequences

- The public API of `cmod-build` grows by one struct and two trait methods.
  Both methods have defaults, so implementations outside this repo still
  compile. On such a backend, `cmod test` returns the "not supported" error.
- The module DAG, the lockfile format, cache keys and fingerprints do not
  change. Test binaries are not cached.
- GCC and clang test compiles differ in one way. GCC gets the profile's
  optimization and debug flags, because its CMIs were built with them. Clang
  does not, as before. If we want both to use the full build flags, that is a
  separate change, because it changes the clang command line.
- Dependency BMIs still come from `common::collect_*_dep_artifacts`, which
  only look for `.pcm`. A GCC project that imports a module from a dependency
  still cannot build its tests. That is a separate issue, and `cmod build`
  has the same gap.
- Adding MSVC means implementing `test_binary_command` for `MsvcBackend`
  (`/reference`, `/Fe`). Nothing else has to change.
