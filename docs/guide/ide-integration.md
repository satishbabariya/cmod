# IDE Integration Guide

cmod includes a built-in Language Server Protocol (LSP) server for IDE support.

## Starting the LSP Server

```bash
cmod lsp
```

The server communicates via JSON-RPC over stdin/stdout, following the LSP specification.

## Supported Features

| Feature | LSP Method | Description |
|---|---|---|
| Completion | `textDocument/completion` | Module names, partitions, C++ keywords |
| Hover | `textDocument/hover` | Module info, version, description |
| Go to Definition | `textDocument/definition` | Jump to module interface file |
| Document Symbols | `textDocument/documentSymbol` | Outline view (modules, classes, functions) |
| Find References | `textDocument/references` | Find all importers of a module |
| Diagnostics | `textDocument/publishDiagnostics` | Import errors, manifest validation, build errors |
| Code Actions | `textDocument/codeAction` | Quick fixes for missing imports, syntax errors |

## Custom cmod Methods

| Method | Direction | Description |
|---|---|---|
| `cmod/buildStatus` | Server → Client | Module build status after save |
| `cmod/dependencies` | Client → Server | Query module dependencies/dependents |
| `cmod/criticalPath` | Client → Server | Get the critical compilation path |
| `cmod/cacheStatus` | Client → Server | Query build cache statistics |

### `cmod/buildStatus` Notification

Sent after `textDocument/didSave`. Payload:

```json
{
  "modules": [
    { "name": "mylib", "status": "up-to-date" },
    { "name": "mylib:ops", "status": "needs-rebuild" }
  ],
  "summary": {
    "total": 2,
    "upToDate": 1,
    "needsRebuild": 1,
    "neverBuilt": 0
  }
}
```

### `cmod/dependencies` Request

Request params:

```json
{ "module": "mylib" }
```

Response:

```json
{ "dependencies": ["base"], "dependents": ["app"] }
```

### `cmod/criticalPath` Request

Response:

```json
{ "criticalPath": ["base", "mylib", "app"] }
```

### `cmod/cacheStatus` Request

Response:

```json
{ "entries": 42, "totalSizeBytes": 10485760 }
```

## Editor Configuration

### Neovim (nvim-lspconfig)

```lua
local lspconfig = require('lspconfig')
local configs = require('lspconfig.configs')

configs.cmod = {
  default_config = {
    cmd = { 'cmod', 'lsp' },
    filetypes = { 'cpp', 'cppm' },
    root_dir = lspconfig.util.root_pattern('cmod.toml'),
    settings = {},
  },
}

lspconfig.cmod.setup{}
```

### VS Code

Add to `.vscode/settings.json`:

```json
{
  "cmod.lsp.path": "cmod",
  "cmod.lsp.args": ["lsp"]
}
```

Or use a generic LSP client extension with the command `cmod lsp`.

### Emacs (eglot)

```elisp
(add-to-list 'eglot-server-programs
             '((c++-mode) . ("cmod" "lsp")))
```

## Troubleshooting

- **LSP not starting:** Ensure `cmod` is on your PATH and `cmod lsp` runs without errors.
- **No completions:** Verify `cmod.toml` exists in the project root and lists dependencies.
- **Missing diagnostics:** Check that source files are in the configured `[build].sources` directories (default: `src/`).
- **Stale graph:** The module graph is cached for 30 seconds. Save `cmod.toml` to force a refresh.
