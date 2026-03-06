//! Module-aware code completion for the LSP server.
//!
//! Provides completions for:
//! - `import` statements (module names from dependency graph)
//! - Module partition references
//! - cmod.toml dependency names
//! - C++ keywords in module contexts

use std::path::PathBuf;

use serde_json::Value;

/// Completion provider for cmod LSP.
pub struct CompletionProvider {
    /// Project root directory.
    project_root: Option<PathBuf>,
    /// Known module names from the dependency graph.
    known_modules: Vec<ModuleInfo>,
}

/// Information about a known module.
#[derive(Debug, Clone)]
pub struct ModuleInfo {
    /// Module name.
    pub name: String,
    /// Module description.
    pub description: Option<String>,
    /// Whether this is a local module.
    pub is_local: bool,
    /// Available partitions.
    pub partitions: Vec<String>,
}

impl CompletionProvider {
    pub fn new() -> Self {
        CompletionProvider {
            project_root: None,
            known_modules: Vec::new(),
        }
    }

    /// Set the project root and scan for modules.
    pub fn set_project_root(&mut self, root: PathBuf) {
        self.project_root = Some(root.clone());
        self.scan_modules(&root);
    }

    /// Provide completions at a given position in a document.
    pub fn complete(&self, content: &str, line: usize, character: usize) -> Vec<Value> {
        let lines: Vec<&str> = content.lines().collect();
        let current_line = match lines.get(line) {
            Some(l) => *l,
            None => return Vec::new(),
        };

        let prefix = &current_line[..character.min(current_line.len())];

        // Import statement completion
        if prefix.trim_start().starts_with("import") {
            return self.complete_import(prefix);
        }

        // export module completion
        if prefix.trim_start().starts_with("export module") {
            return self.complete_module_declaration(prefix);
        }

        // General C++ module keywords
        if self.is_module_context(content) {
            return self.complete_module_keywords(prefix);
        }

        Vec::new()
    }

    fn complete_import(&self, prefix: &str) -> Vec<Value> {
        let import_prefix = prefix
            .trim_start()
            .strip_prefix("import")
            .unwrap_or("")
            .trim();

        let mut items = Vec::new();

        for module in &self.known_modules {
            if module.name.starts_with(import_prefix) || import_prefix.is_empty() {
                items.push(serde_json::json!({
                    "label": &module.name,
                    "kind": 9, // Module
                    "detail": module.description.as_deref().unwrap_or("C++ module"),
                    "insertText": format!("{};", module.name),
                    "documentation": {
                        "kind": "markdown",
                        "value": format!(
                            "**Module:** `{}`\n\n{}{}",
                            module.name,
                            if module.is_local { "Local module\n" } else { "" },
                            module.description.as_deref().unwrap_or(""),
                        ),
                    },
                }));

                // Also suggest partitions
                for partition in &module.partitions {
                    items.push(serde_json::json!({
                        "label": format!("{}:{}", module.name, partition),
                        "kind": 9,
                        "detail": "Module partition",
                        "insertText": format!("{}:{};", module.name, partition),
                    }));
                }
            }
        }

        // Add standard library modules
        for std_mod in &["std", "std.compat"] {
            if std_mod.starts_with(import_prefix) || import_prefix.is_empty() {
                items.push(serde_json::json!({
                    "label": std_mod,
                    "kind": 9,
                    "detail": "C++ Standard Library module",
                    "insertText": format!("{};", std_mod),
                }));
            }
        }

        items
    }

    fn complete_module_declaration(&self, _prefix: &str) -> Vec<Value> {
        let mut items = Vec::new();

        if let Some(ref root) = self.project_root {
            let name = root
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "my_module".to_string());

            items.push(serde_json::json!({
                "label": format!("local.{}", name),
                "kind": 9,
                "detail": "Local module name",
                "insertText": format!("local.{};", name),
            }));
        }

        items
    }

    fn complete_module_keywords(&self, prefix: &str) -> Vec<Value> {
        let keywords = [
            ("export module", "Declare a module interface"),
            ("export import", "Re-export an imported module"),
            ("import", "Import a module"),
            ("module", "Begin module implementation"),
            ("export", "Export a declaration"),
            ("module :private", "Begin private module fragment"),
        ];

        let trimmed = prefix.trim_start();
        keywords
            .iter()
            .filter(|(kw, _)| kw.starts_with(trimmed) || trimmed.is_empty())
            .map(|(kw, desc)| {
                serde_json::json!({
                    "label": kw,
                    "kind": 14, // Keyword
                    "detail": desc,
                    "insertText": kw,
                })
            })
            .collect()
    }

    fn is_module_context(&self, content: &str) -> bool {
        content.contains("export module")
            || content.contains("import")
            || content.contains("module;")
    }

    fn scan_modules(&mut self, root: &std::path::Path) {
        self.known_modules.clear();

        // Read cmod.toml for dependencies
        let manifest_path = root.join("cmod.toml");
        if let Ok(content) = std::fs::read_to_string(&manifest_path) {
            if let Ok(manifest) = cmod_core::manifest::Manifest::from_str(&content) {
                // Add local module
                if let Some(ref module) = manifest.module {
                    self.known_modules.push(ModuleInfo {
                        name: module.name.clone(),
                        description: manifest.package.description.clone(),
                        is_local: true,
                        partitions: Vec::new(),
                    });
                }

                // Add dependency modules
                for name in manifest.dependencies.keys() {
                    self.known_modules.push(ModuleInfo {
                        name: name.clone(),
                        description: None,
                        is_local: false,
                        partitions: Vec::new(),
                    });
                }
            }
        }

        // Scan source directory for module partitions
        let src_dir = root.join("src");
        if src_dir.exists() {
            if let Ok(sources) = cmod_build::runner::discover_sources(&src_dir) {
                for source in &sources {
                    if let Ok(Some(name)) = cmod_build::runner::extract_module_name(source) {
                        if name.contains(':') {
                            // This is a partition
                            if let Some((parent, partition)) = name.split_once(':') {
                                if let Some(module) =
                                    self.known_modules.iter_mut().find(|m| m.name == parent)
                                {
                                    module.partitions.push(partition.to_string());
                                }
                            }
                        } else if !self.known_modules.iter().any(|m| m.name == name) {
                            self.known_modules.push(ModuleInfo {
                                name,
                                description: None,
                                is_local: true,
                                partitions: Vec::new(),
                            });
                        }
                    }
                }
            }
        }
    }
}

impl Default for CompletionProvider {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_completion_provider_new() {
        let provider = CompletionProvider::new();
        assert!(provider.known_modules.is_empty());
    }

    #[test]
    fn test_complete_import() {
        let mut provider = CompletionProvider::new();
        provider.known_modules.push(ModuleInfo {
            name: "github.fmtlib.fmt".into(),
            description: Some("Format library".into()),
            is_local: false,
            partitions: vec!["core".into()],
        });

        let items = provider.complete("import ", 0, 7);
        assert!(!items.is_empty());

        // Should include fmt module and its partition
        let labels: Vec<&str> = items
            .iter()
            .filter_map(|i| i.get("label").and_then(|l| l.as_str()))
            .collect();
        assert!(labels.contains(&"github.fmtlib.fmt"));
        assert!(labels.contains(&"github.fmtlib.fmt:core"));
    }

    #[test]
    fn test_complete_import_with_prefix() {
        let mut provider = CompletionProvider::new();
        provider.known_modules.push(ModuleInfo {
            name: "github.fmtlib.fmt".into(),
            description: None,
            is_local: false,
            partitions: vec![],
        });
        provider.known_modules.push(ModuleInfo {
            name: "github.gabime.spdlog".into(),
            description: None,
            is_local: false,
            partitions: vec![],
        });

        let items = provider.complete("import github.fmt", 0, 17);
        let labels: Vec<&str> = items
            .iter()
            .filter_map(|i| i.get("label").and_then(|l| l.as_str()))
            .collect();
        assert!(labels.contains(&"github.fmtlib.fmt"));
        assert!(!labels.contains(&"github.gabime.spdlog"));
    }

    #[test]
    fn test_complete_std_modules() {
        let provider = CompletionProvider::new();
        let items = provider.complete("import ", 0, 7);
        let labels: Vec<&str> = items
            .iter()
            .filter_map(|i| i.get("label").and_then(|l| l.as_str()))
            .collect();
        assert!(labels.contains(&"std"));
        assert!(labels.contains(&"std.compat"));
    }

    #[test]
    fn test_complete_module_keywords() {
        let provider = CompletionProvider::new();
        let content = "export module test;\nimport std;\n\n";
        let items = provider.complete(content, 2, 0);
        assert!(!items.is_empty());
    }
}
