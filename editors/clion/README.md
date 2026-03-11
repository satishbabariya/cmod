# cmod CLion Plugin

IntelliJ/CLion plugin for [cmod](https://github.com/satishbabariya/cmod), a Cargo-inspired, Git-native package and build tool for modern C++20+ modules.

## Features

- **LSP Integration** — Code intelligence via the `cmod lsp` server (completions, diagnostics, go-to-definition)
- **Run Configurations** — Build, Test, and Run configurations with profile, jobs, and sanitizer options
- **Tool Window** — Module graph visualization, dependency tree, and cache status panel
- **Actions** — Build, test, run, clean, format, lint, graph, and explain accessible from the Build menu
- **File Types** — Syntax support for `.cppm`, `.ixx`, `.mxx` module interface files and `cmod.toml` manifests
- **Settings** — Configurable cmod binary path, default profile, parallel jobs, and on-save hooks

## Requirements

- CLion 2024.2 or later (IntelliJ 2024.2+)
- [cmod](https://github.com/satishbabariya/cmod) installed and available on PATH
- Clang 17+ for C++20 module support

## Installation

### From JetBrains Marketplace

1. Open CLion
2. Go to **Settings > Plugins > Marketplace**
3. Search for "cmod"
4. Click **Install**

### From Disk

1. Download the latest release `.zip` from the [releases page](https://github.com/satishbabariya/cmod/releases)
2. Open CLion
3. Go to **Settings > Plugins > Install Plugin from Disk**
4. Select the downloaded `.zip` file

### From Source

```bash
cd editors/clion
./gradlew buildPlugin
```

The plugin `.zip` will be in `build/distributions/`.

## Configuration

Go to **Settings > Tools > cmod** to configure:

| Setting | Default | Description |
|---|---|---|
| cmod binary path | (auto-detect) | Path to the `cmod` executable |
| Default profile | debug | Build profile for new run configurations |
| Default jobs | 0 (auto) | Parallel compilation jobs |
| Auto-start LSP | true | Start LSP server when a cmod project is opened |
| Show notifications | true | Show balloon notifications for build results |
| Format on save | false | Run `cmod fmt` when saving files |
| Lint on save | false | Run `cmod lint` when saving files |

## Usage

### Run Configurations

Create run configurations from **Run > Edit Configurations > + > cmod Build/Test/Run**.

- **cmod Build** — Configures `cmod build` with profile, parallel jobs, force rebuild, and timing display
- **cmod Test** — Configures `cmod test` with filter pattern, coverage, and sanitizer selection
- **cmod Run** — Configures `cmod run` with profile and program arguments

### Actions

Available from **Build > cmod** menu:

- **Build** — Run `cmod build`
- **Test** — Run `cmod test`
- **Run** — Run `cmod run`
- **Clean** — Run `cmod clean`
- **Format** — Run `cmod fmt`
- **Lint** — Run `cmod lint`
- **Show Module Graph** — Open the module graph visualization
- **Explain Module** — Explain why a module would be rebuilt

### Tool Window

The **cmod** tool window (right sidebar) has three tabs:

1. **Module Graph** — Interactive visualization of the module dependency graph with status colors (green = ok, yellow = needs rebuild, gray = unknown) and build timing
2. **Dependencies** — Tree view of the project's cmod.toml manifest showing package info, module name, and dependency versions
3. **Cache** — Cache statistics with refresh, clean, and garbage collection controls

## Development

### Prerequisites

- JDK 17+
- Gradle 8.x (wrapper included)

### Building

```bash
./gradlew build
```

### Running in IDE

```bash
./gradlew runIde
```

### Testing

```bash
./gradlew test
```

## License

Apache-2.0 — see [LICENSE](LICENSE).
