# cmod Editor Extensions

IDE integrations for [cmod](https://github.com/satishbabariya/cmod), a Cargo-inspired C++20+ module manager.

## Structure

```
editors/
├── shared/          # Shared assets used by all extensions
│   ├── d3.v7.min.js # D3.js library for graph visualization
│   └── graph/       # Graph renderer (HTML, CSS, JS)
├── vscode/          # VS Code extension
└── clion/           # CLion / IntelliJ plugin
```

### `shared/`

Contains the D3.js-based module graph renderer used by both extensions. The graph assets (`graph.html`, `graph.css`, `graph.js`) provide force-directed visualization of the C++ module dependency graph. `d3.v7.min.js` is the vendored D3 v7 library.

### `vscode/`

TypeScript extension for Visual Studio Code. Provides:

- LSP integration via `cmod lsp`
- Build/test/run commands and tasks
- Module graph webview panel
- Format-on-save and lint-on-save
- Dependency tree and build status views
- C++20 module file type support (`.cppm`, `.ixx`, `.mxx`)
- cmod.toml snippets and problem matchers

**Build:**

```bash
cd editors/vscode
npm install
npx webpack --mode development   # Development build
npx webpack --mode production    # Production build
npx @vscode/vsce package         # Package as .vsix
```

### `clion/`

Kotlin plugin for CLion and other JetBrains IDEs. Provides:

- LSP integration via `cmod lsp`
- Run configurations for build, test, and run
- Tool window with graph visualization, dependency tree, and cache status
- Format-on-save and lint-on-save
- cmod.toml and C++20 module file type support
- Settings panel under Tools > cmod

**Build:**

```bash
cd editors/clion
./gradlew buildPlugin            # Build plugin ZIP
./gradlew runIde                 # Launch sandbox IDE for testing
```

## How Shared Assets Are Distributed

The shared graph assets are used differently by each extension:

- **VS Code**: Copies `d3.v7.min.js` to `vscode/resources/webview/`. The graph CSS and JS are read at runtime from `shared/graph/` and inlined into the webview HTML for CSP compliance.
- **CLion**: Loads `shared/graph/graph.html` via JCEF browser component, with D3 loaded from the shared directory.

When updating shared assets, ensure both extensions are tested.
