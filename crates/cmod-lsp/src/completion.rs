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
    /// Root file path for go-to-definition.
    pub root_path: Option<PathBuf>,
    /// Module version.
    pub version: Option<String>,
    /// Repository URL.
    pub repository: Option<String>,
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

        let byte_offset = utf16_offset_to_byte_offset(current_line, character);
        let prefix = &current_line[..byte_offset];

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
                        root_path: Some(root.join(&module.root)),
                        version: Some(manifest.package.version.clone()),
                        repository: None,
                    });
                }

                // Add dependency modules
                for name in manifest.dependencies.keys() {
                    self.known_modules.push(ModuleInfo {
                        name: name.clone(),
                        description: None,
                        is_local: false,
                        partitions: Vec::new(),
                        root_path: None,
                        version: None,
                        repository: None,
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
                                root_path: Some(source.clone()),
                                version: None,
                                repository: None,
                            });
                        }
                    }
                }
            }
        }

        // Enrich dependency modules with lockfile info
        let lockfile_path = root.join("cmod.lock");
        if let Ok(lockfile_content) = std::fs::read_to_string(&lockfile_path) {
            if let Ok(lockfile) = cmod_core::lockfile::Lockfile::from_str(&lockfile_content) {
                for module in &mut self.known_modules {
                    if !module.is_local {
                        if let Some(pkg) = lockfile.find_package(&module.name) {
                            module.version = Some(pkg.version.clone());
                            module.repository = pkg.repo.clone();
                        }
                    }
                }
            }
        }
    }

    /// Find the root file path for a module name.
    pub fn find_module_root(&self, module_name: &str) -> Option<PathBuf> {
        self.known_modules
            .iter()
            .find(|m| m.name == module_name)
            .and_then(|m| m.root_path.clone())
    }

    /// Find module info by name.
    pub fn find_module_info(&self, module_name: &str) -> Option<&ModuleInfo> {
        self.known_modules.iter().find(|m| m.name == module_name)
    }
}

impl Default for CompletionProvider {
    fn default() -> Self {
        Self::new()
    }
}

/// Convert an LSP UTF-16 code-unit offset into a byte offset within a UTF-8 string.
///
/// Returns `s.len()` when `utf16_offset` is at or past the end of the string,
/// and always lands on a valid UTF-8 char boundary.
fn utf16_offset_to_byte_offset(s: &str, utf16_offset: usize) -> usize {
    let mut utf16_count = 0;
    for (byte_idx, ch) in s.char_indices() {
        if utf16_count >= utf16_offset {
            return byte_idx;
        }
        utf16_count += ch.len_utf16();
    }
    s.len()
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
            root_path: None,
            version: None,
            repository: None,
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
            root_path: None,
            version: None,
            repository: None,
        });
        provider.known_modules.push(ModuleInfo {
            name: "github.gabime.spdlog".into(),
            description: None,
            is_local: false,
            partitions: vec![],
            root_path: None,
            version: None,
            repository: None,
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
