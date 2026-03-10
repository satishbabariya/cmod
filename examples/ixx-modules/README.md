# ixx-modules

Example using the `.ixx` file extension for module interfaces, the convention used by MSVC.

## Structure

```text
ixx-modules/
├── cmod.toml         # root = "src/lib.ixx"
└── src/
    ├── lib.ixx       # module interface using .ixx extension
    └── main.cpp      # consumer binary
```

## Usage

```bash
cmod build
cmod run
```

## Key concepts

- `.ixx` is the MSVC convention for module interface files
- Clang does not auto-detect `.ixx` as module source, so cmod adds `-x c++-module`
- The `[module].root` field points to the `.ixx` file
- Everything else works identically to `.cppm` modules
- `.mpp` extension (GCC convention) also works the same way
