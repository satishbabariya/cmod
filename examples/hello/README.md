# hello

Minimal cmod binary project with no external dependencies.

## What this demonstrates

- Basic `cmod.toml` manifest structure
- Module interface unit (`lib.cppm`) and implementation unit (`hello.cpp`)
- Building and running a binary with `cmod build` and `cmod run`

## Project structure

```
hello/
├── cmod.toml       # Project manifest
└── src/
    ├── lib.cppm    # Module interface: export module local.hello;
    ├── hello.cpp   # Module implementation: module local.hello;
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

- **Module interface unit** (`lib.cppm`): declares `export module local.hello;` and exports the public API.
- **Module implementation unit** (`hello.cpp`): declares `module local.hello;` (no `export`) and provides the function bodies.
- **Consumer** (`main.cpp`): uses `import local.hello;` to access the exported API.
