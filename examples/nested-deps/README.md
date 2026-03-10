# nested-deps

Example where the main project has a path dependency (`formatter`), and that path dependency itself has a git dependency (`fmt-cmod`).

## Structure

```
nested-deps/
├── cmod.toml                        # depends on formatter via path
├── src/
│   ├── lib.cppm                     # import local.formatter;
│   └── main.cpp
└── libs/
    └── formatter/
        ├── cmod.toml                # depends on fmt-cmod via git
        ├── cmod.lock                # locks the git dependency
        └── src/
            └── lib.cppm             # formatting utilities
```

## Usage

```bash
# First resolve the formatter's own dependencies
cd libs/formatter && cmod resolve && cd ../..

# Then build the main project
cmod build
cmod run
```

## Key concepts

- Path dependencies can have their own `cmod.lock` with git dependencies
- cmod loads the path dep's lockfile and builds its git deps transitively
- The dependency chain: `nested-deps` → (path) `formatter` → (git) `fmt-cmod`
- This pattern is common in monorepos where internal libraries wrap external ones
