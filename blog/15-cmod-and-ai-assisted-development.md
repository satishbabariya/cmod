# cmod and AI-Assisted C++ Development

*How structured project manifests and deterministic builds make C++ projects more accessible to AI coding assistants and LLM-based tools.*

---

AI coding assistants have transformed how developers write Python, JavaScript, and TypeScript. But C++ has been left behind. Not because LLMs can't generate C++ — they can — but because C++ projects are so difficult to set up, build, and validate that even a correct code suggestion often leads to a broken build.

cmod changes this equation.

## Why AI Struggles with C++ Today

An AI assistant needs to:

1. **Understand project structure** — What files exist? Dependencies? Targets?
2. **Generate correct code** — What APIs are available?
3. **Validate the result** — Does it compile? Do tests pass?

With CMake, step 1 requires parsing a Turing-complete scripting language. Step 3 requires a multi-step build process that may fail for unrelated reasons.

## How cmod Makes C++ AI-Friendly

### Machine-readable manifest
`cmod.toml` is simple TOML. An AI reads it instantly to understand project name, dependencies, module structure, and build config.

### Explicit module boundaries
C++20 `export` declarations give AI tools a clear picture of the public API. No guessing which symbols are "public."

### Simple build validation
```bash
cmod build  # One command. Exit code 0 = success.
```

### Easy dependency management
```bash
cmod add github.com/fmtlib/fmt@10.0  # One command to add a dep
```

## Practical AI + cmod Workflows

```bash
# AI generates code, then validates
cmod build          # Compile check
cmod test           # Test check
cmod deps --tree    # Explore dependencies
cmod graph --format json  # Machine-readable output
```

## Structured Output for Tooling

Every command supports JSON output for programmatic consumption:
```bash
cmod graph --format json
cmod sbom --output sbom.json
cmod compile-commands  # For clangd integration
```

## The Bigger Picture

AI-assisted development works best when:
- Project structure is declarative
- Build commands are simple
- APIs are explicit (`export`)
- Builds are deterministic
- Errors are clear

cmod checks all of these boxes. By making C++ projects as structured as Rust projects, cmod opens the door to AI-powered C++ development.

[Get started with cmod →](https://github.com/satishbabariya/cmod)
