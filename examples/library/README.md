# library

Static library example demonstrating C++20 module partitions.

## What this demonstrates

- Module partitions (`:ops`, `:stats`) for organizing code within a module
- `export import :partition;` to re-export partitions from the primary interface
- `static-lib` build type
- Running tests with `cmod test`

## Project structure

```
library/
├── cmod.toml       # Project manifest (build.type = "static-lib")
├── src/
│   ├── lib.cppm    # Primary interface: export import :ops; export import :stats;
│   ├── ops.cppm    # Partition: constexpr add, sub, mul, div
│   └── stats.cppm  # Partition: sum, mean, min_val, max_val
└── tests/
    └── main.cpp    # Assert-based tests for both partitions
```

## Usage

```bash
cd examples/library
cmod build
cmod test
```

## Key concepts

- **Module partitions**: split a module into logical units (`:ops`, `:stats`) that are re-exported through the primary interface.
- **`export import :partition;`**: the primary module interface re-exports partition interfaces so consumers only need `import local.math;`.
- **`constexpr` functions**: the `:ops` partition uses `constexpr` for compile-time evaluation, validated with `static_assert` in tests.
- **`std::span`**: the `:stats` partition accepts `std::span<const int>` for non-owning views over contiguous data.
