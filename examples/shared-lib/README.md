# shared-lib

Shared (dynamic) library example. Produces `.dylib` on macOS, `.so` on Linux, `.dll` on Windows.

## Structure

```text
shared-lib/
├── cmod.toml         # type = "shared-lib"
└── src/
    ├── lib.cppm      # export module with codec utilities
    └── main.cpp      # consumer binary
```

## Usage

```bash
cmod build
cmod run
```

## Key concepts

- `type = "shared-lib"` in `[build]` section
- Platform-correct output extension based on target triple
- Shared library linked with `-shared` flag
