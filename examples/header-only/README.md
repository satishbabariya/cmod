# header-only

Example consuming a header-only library as a path dependency.

## Structure

```
header-only/
├── cmod.toml
├── src/
│   ├── lib.cppm           # uses #include <math/constants.h>
│   └── main.cpp
└── libs/
    └── math-headers/
        ├── cmod.toml      # declares include_dirs = ["include"]
        └── include/
            └── math/
                └── constants.h    # header-only math utilities
```

## Usage

```bash
cmod build
cmod run
```

## Key concepts

- Header-only dependencies have no compilable source files
- cmod auto-detects `include/` directories and adds `-I` flags
- The `[build].include_dirs` in the dep's `cmod.toml` declares additional include paths
- `build_module()` is skipped for header-only deps (no sources to compile)
- Headers are consumed via `#include` in the global module fragment (`module; ... export module`)
