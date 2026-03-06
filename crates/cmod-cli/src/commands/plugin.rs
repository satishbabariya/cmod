use std::collections::BTreeMap;
use std::io::{BufRead, Write};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::shell::Shell;

/// A plugin definition from `[plugins]` in cmod.toml.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDef {
    pub name: String,
    pub path: PathBuf,
    pub capabilities: Vec<String>,
}

/// A message sent to a plugin via stdin.
#[derive(Debug, Serialize)]
pub struct PluginRequest {
    pub action: String,
    pub project_root: String,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub args: BTreeMap<String, String>,
}

/// A response from a plugin via stdout.
#[derive(Debug, Deserialize)]
#[allow(dead_code)]
pub struct PluginResponse {
    pub status: String,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub data: Option<serde_json::Value>,
}

/// Run `cmod plugin list` — list discovered plugins.
pub fn list(shell: &Shell) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    let plugins = discover_plugins(&config);

    if plugins.is_empty() {
        shell.status("Plugins", "no plugins configured");
        shell.note("add plugins in cmod.toml under [plugins]");
        return Ok(());
    }

    shell.status("Plugins", format!("{} plugin(s) configured", plugins.len()));
    for plugin in &plugins {
        shell.status(
            "Plugin",
            format!("{} ({})", plugin.name, plugin.path.display()),
        );
        if !plugin.capabilities.is_empty() {
            shell.verbose("Capabilities", plugin.capabilities.join(", "));
        }
    }

    Ok(())
}

/// Run `cmod plugin run <name>` — execute a plugin.
pub fn run_plugin(name: &str, shell: &Shell) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    let plugins = discover_plugins(&config);
    let plugin = plugins
        .iter()
        .find(|p| p.name == name)
        .ok_or_else(|| CmodError::BuildFailed {
            reason: format!("plugin '{}' not found", name),
        })?;

    let entry = find_plugin_entry(&config.root, plugin)?;

    shell.verbose("Running", format!("{} ({})", plugin.name, entry.display()));

    let request = PluginRequest {
        action: "run".to_string(),
        project_root: config.root.to_string_lossy().to_string(),
        args: BTreeMap::new(),
    };

    let request_json = serde_json::to_string(&request).map_err(|e| CmodError::BuildFailed {
        reason: format!("failed to serialize plugin request: {}", e),
    })?;

    let mut child = std::process::Command::new(&entry)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::inherit())
        .current_dir(&config.root)
        .spawn()
        .map_err(|e| CmodError::BuildFailed {
            reason: format!("failed to launch plugin '{}': {}", name, e),
        })?;

    // Send request
    if let Some(mut stdin) = child.stdin.take() {
        writeln!(stdin, "{}", request_json).ok();
    }

    // Read response
    if let Some(stdout) = child.stdout.take() {
        let reader = std::io::BufReader::new(stdout);
        for line in reader.lines().map_while(Result::ok) {
            if let Ok(resp) = serde_json::from_str::<PluginResponse>(&line) {
                if let Some(msg) = resp.message {
                    shell.verbose(&plugin.name, msg);
                }
            } else {
                shell.verbose(&plugin.name, &line);
            }
        }
    }

    let status = child.wait().map_err(|e| CmodError::BuildFailed {
        reason: format!("plugin '{}' failed: {}", name, e),
    })?;

    if !status.success() {
        return Err(CmodError::BuildFailed {
            reason: format!(
                "plugin '{}' exited with code {}",
                name,
                status.code().unwrap_or(-1)
            ),
        });
    }

    Ok(())
}

/// Discover plugins from the manifest and local plugin directory.
fn discover_plugins(config: &Config) -> Vec<PluginDef> {
    let mut plugins = discover_plugins_in(&config.root);

    // Also discover plugins declared in [plugins] section of cmod.toml
    if let Some(ref manifest_plugins) = config.manifest.plugins {
        for (name, entry) in manifest_plugins {
            // Skip if already discovered from .cmod/plugins/
            if plugins.iter().any(|p| p.name == *name) {
                continue;
            }
            let path = entry
                .path
                .clone()
                .unwrap_or_else(|| config.root.join(".cmod").join("plugins").join(name));
            plugins.push(PluginDef {
                name: name.clone(),
                path,
                capabilities: entry.capabilities.clone(),
            });
        }
    }

    plugins
}

/// Discover plugins from a given root directory.
fn discover_plugins_in(root: &std::path::Path) -> Vec<PluginDef> {
    let mut plugins = Vec::new();

    let plugin_dir = root.join(".cmod").join("plugins");
    if plugin_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&plugin_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let manifest = path.join("plugin.toml");
                if manifest.exists() {
                    if let Ok(content) = std::fs::read_to_string(&manifest) {
                        if let Ok(table) = content.parse::<toml::Table>() {
                            if let Some(plugin_section) = table.get("plugin") {
                                let name = plugin_section
                                    .get("name")
                                    .and_then(|v| v.as_str())
                                    .unwrap_or("unknown")
                                    .to_string();
                                let capabilities = plugin_section
                                    .get("capabilities")
                                    .and_then(|v| v.as_array())
                                    .map(|arr| {
                                        arr.iter()
                                            .filter_map(|v| v.as_str().map(|s| s.to_string()))
                                            .collect()
                                    })
                                    .unwrap_or_default();
                                plugins.push(PluginDef {
                                    name,
                                    path: path.clone(),
                                    capabilities,
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    plugins
}

/// Find the entry point executable for a plugin.
fn find_plugin_entry(root: &std::path::Path, plugin: &PluginDef) -> Result<PathBuf, CmodError> {
    let abs_path = if plugin.path.is_relative() {
        root.join(&plugin.path)
    } else {
        plugin.path.clone()
    };

    // Check for bin/<name> or entry field in plugin.toml
    let candidates = [
        abs_path.join("bin").join(&plugin.name),
        abs_path.join(&plugin.name),
        abs_path.join("entry"),
    ];

    for candidate in &candidates {
        if candidate.exists() {
            return Ok(candidate.clone());
        }
    }

    Err(CmodError::BuildFailed {
        reason: format!(
            "no entry point found for plugin '{}' in {}",
            plugin.name,
            abs_path.display()
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_discover_plugins_empty() {
        let tmp = TempDir::new().unwrap();
        let plugins = discover_plugins_in(tmp.path());
        assert!(plugins.is_empty());
    }

    #[test]
    fn test_discover_plugins_with_dir() {
        let tmp = TempDir::new().unwrap();
        let plugin_dir = tmp.path().join(".cmod/plugins/myfuzz");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        std::fs::write(
            plugin_dir.join("plugin.toml"),
            "[plugin]\nname = \"myfuzz\"\ncapabilities = [\"cli\"]\n",
        )
        .unwrap();

        let plugins = discover_plugins_in(tmp.path());
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].name, "myfuzz");
        assert_eq!(plugins[0].capabilities, vec!["cli"]);
    }

    #[test]
    fn test_plugin_request_serialization() {
        let req = PluginRequest {
            action: "run".to_string(),
            project_root: "/tmp/test".to_string(),
            args: BTreeMap::new(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("\"action\":\"run\""));
    }
}
