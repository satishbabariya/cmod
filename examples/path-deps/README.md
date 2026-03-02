# path-deps

Example project using local path dependencies.

## What this demonstrates

- `path = "libs/..."` dependencies for co-located library development
- Multiple local libraries consumed by a single binary
- Module partitions in a path dependency (geometry uses `:vec2`)
- Inspecting the dependency tree with `cmod deps --tree`

## Project structure

```
path-deps/
├── cmod.toml               # Binary, path deps on geometry + colors
├── src/
│   └── main.cpp            # import local.geometry; import local.colors;
└── libs/
    ├── geometry/
    │   ├── cmod.toml       # static-lib
    │   └── src/
    │       ├── lib.cppm    # export module local.geometry; export import :vec2;
    │       ├── vec2.cppm   # export module local.geometry:vec2; (Vec2 struct)
    │       └── geometry.cpp # module local.geometry; (distance, lerp)
    └── colors/
        ├── cmod.toml       # static-lib
        └── src/
            └── lib.cppm    # export module local.colors; (Color, lerp, to_argb)
```

## Usage

```bash
cd examples/path-deps

# View the dependency tree
cmod deps --tree

# Build and run
cmod build
cmod run
```

Expected output:

```
Point A: (0, 0)
Point B: (3, 4)
Distance: 5
Midpoint: (1.5, 2)
Red:    ARGB = 0xffff0000
Blue:   ARGB = 0xff0000ff
Purple: ARGB = 0xff7f007f
```

## Key concepts

- **Path dependencies**: `geometry = { path = "libs/geometry" }` points to a local directory containing its own `cmod.toml`.
- **Co-located development**: libraries under `libs/` are developed alongside the main project, with changes reflected immediately.
- **Partition in a dep**: the geometry library uses a `:vec2` partition internally — consumers see it through the re-exported primary interface.
- **`cmod deps --tree`**: visualizes the full dependency graph including transitive path deps.
