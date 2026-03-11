# include-dirs

Example demonstrating the `include/` directory convention for mixing C++ headers with C++20 modules.

## Structure

```
include-dirs/
├── cmod.toml
├── include/
│   └── utils/
│       └── config.h       # traditional header-based utility
└── src/
    ├── lib.cppm           # module that #include's the header
    └── main.cpp           # consumer binary
```

## Usage

```bash
cmod build
cmod run
```

## Key concepts

- cmod auto-detects `include/` at the project root and adds `-I include/`
- No need to declare `include_dirs` in `cmod.toml` for the conventional path
- Headers are consumed in the **global module fragment** (`module; #include <...>`)
- This pattern is useful for gradually migrating header-based code to modules
- The same auto-detection works for dependencies (both path and git)
