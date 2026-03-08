# Changelog

All notable changes to the cmod VS Code extension will be documented in this file.

## [0.1.0] - 2026-03-08

### Added
- Initial release of the cmod VS Code extension.
- LSP client integration with `cmod lsp` server over stdio.
- Build, test, run, and clean commands with problem matcher for Clang diagnostics.
- Task provider for VS Code task system (build, test, run, clean).
- Dependency tree view parsed from `cmod.toml`.
- Build status tree view updated via LSP notifications.
- Module graph visualization with force-directed layout.
- Status bar item showing real-time build progress.
- Format and lint commands with optional on-save triggers.
- Project initialization wizard (module or workspace).
- Explain module rebuild command.
- Cache status display.
- C++20 module snippets (module, import, partition, export, etc.).
- File association for `.cppm`, `.ixx`, `.mxx` extensions.
- Configurable cmod binary path, build profile, and parallel jobs.
- Keybinding `Ctrl+Shift+B` / `Cmd+Shift+B` for build.
