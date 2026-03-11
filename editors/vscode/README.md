# cmod - C++ Module Manager for VS Code

A VS Code extension for [cmod](https://github.com/satishbabariya/cmod), the Cargo-inspired, Git-native package and build tool for modern C++20+ modules.

## Features

### Build Integration
- **Build, Test, Run, Clean** commands with problem matcher for Clang diagnostics
- Task provider for VS Code task system integration
- Configurable default build profile (debug/release) and parallel jobs
- Keybinding: `Ctrl+Shift+B` / `Cmd+Shift+B` to build

### Language Server Protocol
- Full LSP integration via `cmod lsp` for diagnostics, completions, and hover info
- Auto-restart on crash (up to 5 retries)
- Real-time build status notifications

### Dependency Management
- **Dependencies** tree view in the activity bar, parsed from `cmod.toml`
- Auto-refresh when `cmod.toml` changes
- Show dependency details (git URL, version, branch, path)

### Module Graph Visualization
- Interactive force-directed graph of your module dependency DAG
- Color-coded by build status (up-to-date, needs-rebuild, never-built)
- Timing annotations for build performance analysis
- Click any node to open its source file
- Filter and zoom controls

### Code Quality
- **Format** and **Lint** commands (`cmod fmt` / `cmod lint`)
- Optional format-on-save and lint-on-save for C++ files
- Problem matcher maps Clang diagnostic format to VS Code Problems panel

### Project Management
- **Initialize** new projects with QuickPick for module vs workspace
- **Explain** why a module would be rebuilt
- **Cache Status** display

### C++20 Module Snippets
- `module` - Module declaration
- `import` - Import statement
- `partition` - Module partition
- `export` / `exportblock` / `exportns` / `exportclass` / `exportfunc`
- `globalfrag` - Global module fragment
- `privatefrag` - Private module fragment
- `cmodtoml` - Scaffold a cmod.toml manifest

## Requirements

- [cmod](https://github.com/satishbabariya/cmod) installed and available on PATH (or configure `cmod.path`)
- Clang/LLVM toolchain for C++20 module compilation

## Extension Settings

| Setting | Default | Description |
|---------|---------|-------------|
| `cmod.path` | `""` | Path to the cmod binary. Leave empty to use PATH. |
| `cmod.lsp.enabled` | `true` | Enable the cmod LSP server. |
| `cmod.build.defaultProfile` | `"debug"` | Default build profile (debug or release). |
| `cmod.build.jobs` | `0` | Number of parallel build jobs. 0 = system default. |
| `cmod.format.onSave` | `false` | Run `cmod fmt` on save for C++ files. |
| `cmod.lint.onSave` | `false` | Run `cmod lint` on save for C++ files. |

## Commands

All commands are available from the Command Palette (`Ctrl+Shift+P` / `Cmd+Shift+P`):

| Command | Description |
|---------|-------------|
| `cmod: Build` | Build the project |
| `cmod: Build (Release)` | Build with release profile |
| `cmod: Test` | Run tests |
| `cmod: Run` | Build and run the binary |
| `cmod: Clean` | Remove build artifacts |
| `cmod: Initialize Project` | Create a new cmod project |
| `cmod: Format` | Format C++ source files |
| `cmod: Lint` | Lint C++ source files |
| `cmod: Cache Status` | Show build cache information |
| `cmod: Explain Module Rebuild` | Explain why a module needs rebuilding |
| `cmod: Show Module Graph` | Visualize the module dependency graph |
| `cmod: Show Dependencies` | Focus the dependencies tree view |

## File Associations

The extension associates `.cppm`, `.ixx`, and `.mxx` file extensions with the C++ language mode.

## Development

```bash
cd editors/vscode
npm install
npm run compile
# Press F5 in VS Code to launch Extension Development Host
```

## License

Apache-2.0
