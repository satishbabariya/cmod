# RFC-0018: Tooling Plugins & Ecosystem Utilities

## Summary
This RFC defines an extensible plugin system and ecosystem utilities for **cmod**, enabling third-party tools, IDEs, CI systems, and custom workflows to integrate cleanly without bloating the core.

## Motivation
- Keep `cmod` core small and stable
- Allow ecosystem innovation without central control
- Support diverse workflows (embedded, HPC, games, enterprise)

## Goals
- Stable plugin API
- Cross-platform support
- Language-agnostic plugin capability
- Secure and sandboxable execution

## Non-Goals
- Hosting or distributing plugins centrally
- Executing untrusted plugins without user consent

## Plugin Architecture

### Plugin Types
1. **CLI Plugins**
   - Extend `cmod` commands
   - Example: `cmod lint`, `cmod audit`

2. **Build Hooks**
   - Pre-build / post-build hooks
   - Artifact inspection

3. **IDE Adapters**
   - VS Code, CLion, Vim, Emacs
   - LSP and diagnostics integration

4. **CI/CD Integrations**
   - GitHub Actions
   - GitLab CI
   - Buildkite, Jenkins

5. **Analysis & Visualization Tools**
   - Dependency graph exporters
   - Build performance profilers

## Plugin Discovery

Plugins are discovered via:
- Local directories (`.cmod/plugins/`)
- Git submodules
- Explicit paths in `cmod.toml`

```toml
[plugins]
fuzz = { path = "tools/fuzz" }
```

## Plugin Interface

### Manifest

```toml
[plugin]
name = "cmod-fuzz"
version = "0.1.0"
entry = "bin/fuzz"
capabilities = ["build-hook", "cli"]
```

### Execution Model
- Plugins run as external processes
- Communicate via stdin/stdout (JSON)
- No in-process ABI coupling

## Security Model
- Explicit enablement per plugin
- Capability-based permissions
- Optional sandboxing (OS-level)

## Standard Utilities

Recommended ecosystem tools:
- `cmod-viz` – DAG visualizer
- `cmod-bench` – build benchmarking
- `cmod-audit` – dependency/license scanning
- `cmod-gen` – project/module generators

## Versioning & Compatibility
- Plugin API versioned separately
- Backward compatibility guaranteed within major versions

## Future Extensions
- WASM-based plugins
- Signed plugin verification
- Remote plugin execution

## Alternatives Considered
- Monolithic core tooling (rejected)
- Centralized plugin registry (rejected)

## Open Questions
- Default sandboxing mechanisms
- Plugin-to-plugin communication

## Status
**Draft**
