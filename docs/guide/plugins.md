# Plugin Development Guide

cmod supports plugins for extending build workflows, running custom analysis, and integrating external tools. Plugins communicate with cmod via a JSON IPC protocol over stdin/stdout.

## Plugin Structure

A plugin lives under `.cmod/plugins/<name>/` and consists of:

```
.cmod/plugins/my-plugin/
├── plugin.toml          # Plugin manifest (required)
├── bin/
│   └── my-plugin        # Executable entry point
└── plugin.sig           # Optional: signature file
```

## Plugin Manifest (`plugin.toml`)

```toml
capabilities = ["cli", "read_project"]

[plugin]
name = "my-plugin"
version = "1.0.0"
description = "What this plugin does"
authors = ["Your Name"]
license = "MIT"
min_cmod_version = "0.1.0"
entry_point = "bin/my-plugin"
plugin_type = "script"     # "native", "script", or "wasm" (future)

[limits]
timeout_secs = 60
max_memory_mb = 256
max_files = 100
max_output_mb = 10
```

## Capabilities

Plugins declare required capabilities in `plugin.toml`. Unknown capabilities trigger a warning.

| Capability | Aliases | Description |
|---|---|---|
| `read_project` | — | Read files within the project directory |
| `write_project` | — | Write files within the project directory |
| `read_manifest` | — | Read the cmod.toml manifest |
| `write_manifest` | — | Modify the cmod.toml manifest |
| `execute_commands` | `cli` | Execute shell commands |
| `network_access` | `network` | Access the network |
| `cache_access` | `cache` | Access the build cache |
| `environment_access` | `env` | Access environment variables |
| `dependency_graph_access` | `deps` | Access the dependency graph |
| `build_plan_access` | `build` | Access the build plan |

## JSON IPC Protocol

### Request (stdin)

cmod writes a single JSON line to the plugin's stdin:

```json
{
  "action": "run",
  "project_root": "/path/to/project",
  "args": {
    "greeting": "world",
    "arg0": "positional-value"
  }
}
```

- `action` — always `"run"` for `cmod plugin run`
- `project_root` — absolute path to the project root
- `args` — key-value pairs from CLI arguments (`key=value` become entries; positional args become `arg0`, `arg1`, etc.)

### Response (stdout)

The plugin writes one or more JSON lines to stdout:

```json
{
  "status": "ok",
  "message": "Analysis complete: 0 issues found",
  "data": { "issues": [] }
}
```

- `status` — `"ok"` or `"error"`
- `message` — human-readable message (displayed to the user)
- `data` — optional structured data

## Declaring Plugins in `cmod.toml`

```toml
[plugins.my-plugin]
path = ".cmod/plugins/my-plugin"
capabilities = ["cli", "read_project"]
```

## Running Plugins

```bash
# List discovered plugins
cmod plugin list

# Run a plugin
cmod plugin run my-plugin

# Pass arguments
cmod plugin run my-plugin -- greeting=hello target=world
```

## Build Hook Integration

Plugins can be invoked as build hooks using the `plugin:` prefix:

```toml
[hooks]
pre-build = "plugin:my-analyzer"
post-build = "plugin:my-reporter"
```

This is equivalent to running `cmod plugin run my-analyzer` before the build.

## Security

### Signature Verification

If `[security].signature_policy` is configured in `cmod.toml`:

- `"require"` — Unsigned plugins are rejected
- `"warn"` — Unsigned plugins produce a warning
- `"none"` — No signature checking (default)

Sign a plugin by placing a `plugin.sig` file alongside `plugin.toml`.

### Version Compatibility

Set `min_cmod_version` in your plugin manifest to enforce a minimum cmod version. If the user's cmod is older, the plugin will refuse to run.

## Example

See [`examples/plugin/`](../../examples/plugin/) for a complete working example.
