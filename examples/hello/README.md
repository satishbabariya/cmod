# hello

Minimal cmod binary project with no external dependencies.

## What this demonstrates

- Basic `cmod.toml` manifest structure
- Module interface unit with inline implementation
- Global module fragment for standard library includes
- Building and running a binary with `cmod build` and `cmod run`

## Project structure

```
hello/
├── cmod.toml       # Project manifest
└── src/
    ├── lib.cppm    # Module interface: export module local.hello;
    └── main.cpp    # Entry point: import local.hello;
```

## Usage

```bash
cd examples/hello
cmod build
cmod run
```

Expected output:

```
Hello, world!
```

## Key concepts

- **Global module fragment**: the `module;` ... `export module` preamble allows `#include` of standard headers before the module declaration.
- **Module interface unit** (`lib.cppm`): declares `export module local.hello;` and exports the public API with inline definitions.
- **Consumer** (`main.cpp`): uses `import local.hello;` to access the exported API. Standard headers are included normally with `#include`.
