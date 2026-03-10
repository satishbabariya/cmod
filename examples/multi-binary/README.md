# multi-binary

Multiple binaries sharing a common library, structured as a workspace.

## Structure

```
multi-binary/
├── cmod.toml              # workspace root: members = ["dice", "roller", "stats"]
├── dice/
│   ├── cmod.toml          # static-lib
│   └── src/lib.cppm       # dice rolling utilities
├── roller/
│   ├── cmod.toml          # binary, depends on dice
│   └── src/
│       ├── lib.cppm       # interactive roller
│       └── main.cpp
└── stats/
    ├── cmod.toml          # binary, depends on dice
    └── src/
        ├── lib.cppm       # statistical analysis (10k roll histogram)
        └── main.cpp
```

## Usage

```bash
# Build all workspace members
cmod build

# Build and run individual members
cd roller && cmod build && cmod run
cd stats && cmod build && cmod run
```

## Key concepts

- Workspace with shared library + multiple binary members
- Each binary is an independent cmod project with its own `cmod.toml`
- Common pattern for CLI tools, benchmarks, and test harnesses sharing a library
- Path dependencies (`dice = { path = "../dice" }`) link members together
