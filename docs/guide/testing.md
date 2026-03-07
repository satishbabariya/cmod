# Testing Guide

Complete guide to discovering, building, running, and reporting tests with cmod.

## Overview

cmod treats testing as a first-class workflow. The `cmod test` command discovers test source files via glob patterns, compiles each into a standalone executable, runs them, and reports results. Tests can import the project module just like any other consumer.

**Execution model:**

1. Resolve dependencies (including `[dev-dependencies]`).
2. Build the project module and its dependencies.
3. Discover test sources matching `[test].test_patterns` (minus `[test].exclude_patterns`).
4. Compile each test source into a separate binary, linking against the project module.
5. Execute test binaries (in parallel when `--jobs` > 1).
6. Collect exit codes: `0` = pass, non-zero = fail.
7. Print summary and exit with `0` if all tests passed, `1` otherwise.

## Project Structure

By convention, test sources live in a `tests/` directory at the project root:

```text
my-project/
  cmod.toml
  src/
    lib.cppm
    main.cpp
  tests/
    test_basic.cpp
    test_math.cpp
    test_edge_cases.cpp
```

Each `.cpp` file under `tests/` is compiled into its own test binary. There is no requirement for a shared `main()` harness -- each file is self-contained.

## Configuration

The `[test]` table in `cmod.toml` controls test discovery and execution:

```toml
[test]
framework = "catch2"                         # Test framework hint: "catch2", "gtest", "custom"
test_patterns = ["tests/**/*.cpp"]           # Glob patterns for test file discovery
exclude_patterns = ["tests/bench_*.cpp"]     # Glob patterns to exclude
runner = ""                                  # Custom test runner command (empty = direct execution)
extra_flags = ["-fsanitize=address"]         # Extra compiler flags applied only to test builds
timeout = 300                                # Per-test timeout in seconds (0 = no timeout)
```

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `framework` | string | `"custom"` | Framework hint. Affects how cmod links the test binary (e.g., auto-link Catch2 main). Values: `"catch2"`, `"gtest"`, `"custom"`. |
| `test_patterns` | list of strings | `["tests/**/*.cpp"]` | Glob patterns for discovering test source files. |
| `exclude_patterns` | list of strings | `[]` | Glob patterns to exclude from discovered test files. |
| `runner` | string | `""` | Custom test runner command. When set, cmod passes the compiled test binary path as an argument to this command instead of executing it directly. |
| `extra_flags` | list of strings | `[]` | Additional compiler flags applied only when compiling test sources. Useful for sanitizers, coverage instrumentation, or relaxed warning levels. |
| `timeout` | integer | `300` | Per-test timeout in seconds. If a test binary does not exit within this duration, it is killed and marked as failed. Set to `0` to disable. |

## CLI Reference

```text
cmod test [TESTNAME] [OPTIONS]
```

### Arguments

| Argument | Description |
|----------|-------------|
| `[TESTNAME]` | Optional positional argument. Run only tests whose name contains this string. |

### Options

| Option | Short | Description |
|--------|-------|-------------|
| `--release` | | Build tests in release mode. |
| `--filter <GLOB>` | | Filter test files by glob pattern (e.g., `"test_math*"`). |
| `--jobs <N>` | `-j` | Number of test binaries to run in parallel (0 = auto, default: 0). |
| `--no-fail-fast` | | Continue running remaining tests after a failure (default: stop on first failure). |
| `--timeout <SECS>` | | Override per-test timeout from `[test].timeout`. |
| `--package <NAME>` | `-p` | In a workspace, run tests for a specific member only. |
| `--coverage` | | Instrument test builds for code coverage and generate a report. |
| `--sanitize <LIST>` | | Enable sanitizers (comma-separated): `address`, `undefined`, `thread`, `memory`. |
| `--format <FMT>` | | Output format: `human` (default), `json`, `junit`, `tap`. |

### Examples

```bash
# Run all tests in debug mode
cmod test

# Run all tests in release mode
cmod test --release

# Run only tests with "math" in the name
cmod test math

# Run tests matching a glob filter
cmod test --filter "test_edge*"

# Run tests in parallel with 4 workers
cmod test -j 4

# Continue after failures
cmod test --no-fail-fast

# Set a 60-second timeout per test
cmod test --timeout 60

# Run tests for a specific workspace member
cmod test -p my-lib

# Generate code coverage
cmod test --coverage

# Enable address and undefined behavior sanitizers
cmod test --sanitize address,undefined

# Output results as JUnit XML
cmod test --format junit > results.xml

# Combine flags
cmod test --release --no-fail-fast -j 8 --format json
```

## Test Frameworks

### No framework (standalone tests)

The simplest approach. Each test file has its own `main()` and uses `assert()` or manual checks. Return `0` for pass, non-zero for fail.

```cpp
import local.my_math;
#include <cassert>

int main() {
    assert(add(2, 3) == 5);
    assert(multiply(4, 5) == 20);
    return 0;
}
```

No special configuration is needed. This is the default when `framework` is omitted or set to `"custom"`.

### Catch2

Set `framework = "catch2"` and add Catch2 as a dev-dependency:

```toml
[test]
framework = "catch2"

[dev-dependencies]
"github.com/catchorg/Catch2" = "^3.4"
```

cmod will auto-link the Catch2 main entry point. Test files use Catch2 macros:

```cpp
#include <catch2/catch_test_macros.hpp>
import local.my_math;

TEST_CASE("addition works", "[math]") {
    REQUIRE(add(2, 3) == 5);
}
```

### GoogleTest

Set `framework = "gtest"` and add GoogleTest as a dev-dependency:

```toml
[test]
framework = "gtest"

[dev-dependencies]
"github.com/google/googletest" = "^1.14"
```

Test files use GoogleTest macros:

```cpp
#include <gtest/gtest.h>
import local.my_math;

TEST(MathTest, Addition) {
    EXPECT_EQ(add(2, 3), 5);
}
```

### Custom

Set `framework = "custom"` (or omit the field) for any other framework or hand-written test harness. cmod makes no assumptions about linking or entry points.

## Dev-Dependencies

Dependencies listed under `[dev-dependencies]` are resolved and linked only when building tests. They are never included in the production build.

```toml
[dev-dependencies]
"github.com/catchorg/Catch2" = "^3.4"
"github.com/acme/test-utils" = "^1.0"
```

This keeps test-only libraries out of the final binary or library artifact.

## Filtering and Selection

There are two ways to select which tests to run:

### Positional name filter

Pass a substring as a positional argument. Only test binaries whose file name (without extension) contains the string will run:

```bash
cmod test math          # Runs test_math, math_ops, etc.
cmod test edge          # Runs test_edge_cases
```

### Glob filter

The `--filter` flag accepts a glob pattern matched against test file names:

```bash
cmod test --filter "test_math*"
cmod test --filter "*edge*"
```

Both filters can be combined. A test must match both to be selected.

## Parallel Execution

By default (`--jobs 0`), cmod auto-detects the number of CPU cores and runs that many test binaries concurrently. Override with an explicit count:

```bash
cmod test -j 1          # Sequential execution
cmod test -j 4          # 4 tests in parallel
cmod test -j 0          # Auto-detect (default)
```

When running in parallel, output from each test binary is captured and printed after completion to avoid interleaving. The summary is printed once all tests finish.

## Workspace Testing

In a workspace, `cmod test` iterates over members in dependency (build) order:

1. For each member, discover and run its tests.
2. Print a per-member summary after each member completes.
3. Print an aggregate summary at the end.

Use `--package` to restrict testing to a single member:

```bash
cmod test                   # Test all workspace members
cmod test -p core           # Test only the "core" member
cmod test -p utils          # Test only the "utils" member
```

Per-member summary example:

```text
--- core: 5 passed, 0 failed (0.42s)
--- utils: 3 passed, 1 failed (0.31s)
--- app: 2 passed, 0 failed (0.18s)

=== 10 passed, 1 failed (0.91s)
```

## Coverage

The `--coverage` flag instruments test builds with source-based code coverage (LLVM profile instrumentation). After tests complete, cmod runs `llvm-profdata` and `llvm-cov` to produce a report.

```bash
cmod test --coverage
```

Workflow under the hood:

1. Add `-fprofile-instr-generate -fcoverage-mapping` to compiler flags.
2. Run test binaries; each writes a `.profraw` file.
3. Merge profiles: `llvm-profdata merge -sparse *.profraw -o cmod.profdata`.
4. Generate report: `llvm-cov report <binary> -instr-profile=cmod.profdata`.

The coverage summary is printed to stdout. Raw profile data is written to `build/coverage/`.

**Requirements:** `llvm-profdata` and `llvm-cov` must be available in `PATH`.

## Sanitizers

The `--sanitize` flag enables Clang sanitizers by injecting the corresponding `-fsanitize=` flags during test compilation and linking.

```bash
cmod test --sanitize address              # AddressSanitizer (ASan)
cmod test --sanitize undefined            # UndefinedBehaviorSanitizer (UBSan)
cmod test --sanitize thread               # ThreadSanitizer (TSan)
cmod test --sanitize memory               # MemorySanitizer (MSan)
cmod test --sanitize address,undefined    # ASan + UBSan combined
```

Supported sanitizers:

| Value | Sanitizer | Detects |
|-------|-----------|---------|
| `address` | AddressSanitizer | Buffer overflows, use-after-free, leaks |
| `undefined` | UBSan | Undefined behavior (signed overflow, null deref, etc.) |
| `thread` | ThreadSanitizer | Data races |
| `memory` | MemorySanitizer | Uninitialized memory reads |

**Note:** `thread` and `address` cannot be combined. `memory` is only available on Linux.

## Structured Output

The `--format` flag controls how test results are reported.

### `human` (default)

Human-readable colored terminal output:

```text
Running 3 tests...

  PASS  test_basic (0.02s)
  PASS  test_math (0.01s)
  FAIL  test_edge_cases (0.03s)
         assertion failed: factorial(0) == 1
         at tests/test_edge_cases.cpp:12

2 passed, 1 failed (0.06s)
```

### `json`

Machine-readable JSON. Each test is an object in the `tests` array:

```json
{
  "suite": "my-project",
  "tests": [
    {"name": "test_basic", "status": "pass", "duration_ms": 20},
    {"name": "test_math", "status": "pass", "duration_ms": 10},
    {"name": "test_edge_cases", "status": "fail", "duration_ms": 30, "output": "assertion failed..."}
  ],
  "passed": 2,
  "failed": 1,
  "duration_ms": 60
}
```

### `junit`

JUnit XML format, compatible with most CI systems:

```xml
<?xml version="1.0" encoding="UTF-8"?>
<testsuites tests="3" failures="1" time="0.06">
  <testsuite name="my-project" tests="3" failures="1" time="0.06">
    <testcase name="test_basic" time="0.02"/>
    <testcase name="test_math" time="0.01"/>
    <testcase name="test_edge_cases" time="0.03">
      <failure message="assertion failed: factorial(0) == 1"/>
    </testcase>
  </testsuite>
</testsuites>
```

### `tap`

Test Anything Protocol (TAP) format:

```tap
TAP version 13
1..3
ok 1 - test_basic (0.02s)
ok 2 - test_math (0.01s)
not ok 3 - test_edge_cases (0.03s)
  ---
  message: "assertion failed: factorial(0) == 1"
  ...
```

## Custom Test Runners

Set `[test].runner` to delegate test execution to an external command. cmod passes the compiled test binary path as an argument:

```toml
[test]
runner = "valgrind --leak-check=full"
```

With this configuration, cmod runs:

```bash
valgrind --leak-check=full build/tests/test_basic
```

The runner command receives the test binary as its last argument. The runner's exit code determines pass/fail.

## Hooks

The `[hooks]` table supports `pre-test` and `post-test` commands:

```toml
[hooks]
pre-test = "./scripts/setup-test-fixtures.sh"
post-test = "./scripts/cleanup-test-fixtures.sh"
```

- `pre-test` runs before any test compilation or execution. A non-zero exit code aborts the test run.
- `post-test` runs after all tests complete, regardless of pass/fail status.

## CI/CD Integration

### Basic CI configuration

```bash
cmod test --locked --release --format junit > test-results.xml
```

The `--locked` flag ensures the lockfile is up to date (fails otherwise). The `--format junit` flag produces output that most CI systems can ingest natively.

### Coverage in CI

```bash
cmod test --coverage --format json > test-results.json
```

`--format json` captures test pass/fail results. Coverage data is written separately to `build/{profile}/coverage/` by `llvm-cov`.

### Sanitizer sweep

Run each sanitizer in a separate job to isolate failures:

```bash
# Job 1
cmod test --sanitize address,undefined

# Job 2
cmod test --sanitize thread
```

### Fail-fast vs. full run

- Default behavior (`cmod test`) stops on the first failure. This is useful for local development.
- Use `--no-fail-fast` in CI to see all failures in one run:

```bash
cmod test --no-fail-fast --format junit > results.xml
```

### Workspace CI

Test all workspace members and produce a combined report:

```bash
cmod test --no-fail-fast --format junit > results.xml
```

Or test members individually for clearer CI job separation:

```bash
cmod test -p core --format junit > core-results.xml
cmod test -p utils --format junit > utils-results.xml
```
