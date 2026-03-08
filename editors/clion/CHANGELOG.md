# Changelog

All notable changes to the cmod CLion plugin will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.1.0] - 2026-03-08

### Added

- LSP server integration via `cmod lsp` over stdio
- Build, Test, and Run configuration types with full settings editors
- Tool window with three tabs: Module Graph, Dependencies, Cache Status
- Module graph visualization with circular layout, status colors, and timing annotations
- Dependency tree panel parsing cmod.toml manifests
- Cache status panel with refresh, clean, and garbage collection controls
- Eight menu actions: Build, Test, Run, Clean, Format, Lint, Show Graph, Explain
- File type support for C++20 module interface files (.cppm, .ixx, .mxx)
- File type support for cmod.toml manifest files
- Application-level settings: binary path, default profile, jobs, LSP auto-start, notifications, format/lint on save
- Clang diagnostic output parser (file:line:col: severity: message)
- Auto-detection of cmod binary on PATH and common installation directories
- Lightweight cmod.toml TOML parser for manifest reading
