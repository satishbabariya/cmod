//! Real-time diagnostics engine for the LSP server.
//!
//! Provides diagnostics for:
//! - Module import errors (missing modules, circular imports)
//! - cmod.toml validation errors
//! - Build errors from the last compilation
//! - Dependency issues

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Diagnostics engine for cmod LSP.
pub struct DiagnosticsEngine {
    /// Project root directory.
    project_root: Option<PathBuf>,
}

/// LSP diagnostic severity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    Error = 1,
    Warning = 2,
    Information = 3,
    Hint = 4,
}

/// A single diagnostic message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    /// Range in the document.
    pub range: DiagnosticRange,
    /// Severity (1=error, 2=warning, 3=info, 4=hint).
    pub severity: u8,
    /// Source of the diagnostic.
    pub source: String,
    /// Diagnostic message.
    pub message: String,
    /// Diagnostic code.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<String>,
}

/// A range in a document (0-based line/character).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiagnosticRange {
    pub start: Position,
    pub end: Position,
}

/// A position in a document.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Position {
    pub line: u32,
    pub character: u32,
}

impl DiagnosticsEngine {
    pub fn new() -> Self {
        DiagnosticsEngine { project_root: None }
    }

    /// Set the project root.
    pub fn set_project_root(&mut self, root: PathBuf) {
        self.project_root = Some(root);
    }

    /// Generate diagnostics for a file.
    pub fn diagnose_file(&self, file_path: &Path) -> Vec<Value> {
        let mut diagnostics = Vec::new();

        let file_name = file_path
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();

        if file_name == "cmod.toml" {
            diagnostics.extend(self.diagnose_manifest(file_path));
        } else if is_cpp_source(&file_name) {
            diagnostics.extend(self.diagnose_source_from_file(file_path));
        }

        diagnostics
    }

    /// Diagnose a cmod.toml manifest file.
    fn diagnose_manifest(&self, path: &Path) -> Vec<Value> {
        let mut diagnostics = Vec::new();

        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return diagnostics,
        };

        // Try to parse the manifest
        match cmod_core::manifest::Manifest::from_str(&content) {
            Ok(manifest) => {
                // Validate the manifest
                if let Err(e) = manifest.validate() {
                    diagnostics.push(make_diagnostic(
                        0,
                        0,
                        DiagnosticSeverity::Error,
                        &format!("{}", e),
                        "cmod-validate",
                    ));
                }

                // Check for common issues
                if manifest.package.version == "0.0.0" {
                    diagnostics.push(make_diagnostic(
                        0,
                        0,
                        DiagnosticSeverity::Warning,
                        "package version is 0.0.0; consider setting a meaningful version",
                        "cmod-version",
                    ));
                }

                if manifest.package.license.is_none() {
                    diagnostics.push(make_diagnostic(
                        0,
                        0,
                        DiagnosticSeverity::Information,
                        "no license specified; consider adding a license field",
                        "cmod-license",
                    ));
                }
            }
            Err(e) => {
                // Find the line number from the error if possible
                let line = find_error_line(&format!("{}", e), &content).unwrap_or(0);
                diagnostics.push(make_diagnostic(
                    line,
                    0,
                    DiagnosticSeverity::Error,
                    &format!("manifest parse error: {}", e),
                    "cmod-parse",
                ));
            }
        }

        diagnostics
    }

    /// Diagnose a C++ source file for module-related issues.
    fn diagnose_source_from_file(&self, path: &Path) -> Vec<Value> {
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };
        self.diagnose_source_content(&content)
    }

    /// Diagnose C++ source content for module-related issues.
    ///
    /// This is the in-memory variant used by `didChange` where the file
    /// may not yet be saved to disk.
    pub fn diagnose_source(&self, content: &str, _path: &Path) -> Vec<Value> {
        self.diagnose_source_content(content)
    }

    fn diagnose_source_content(&self, content: &str) -> Vec<Value> {
        let mut diagnostics = Vec::new();

        let lines: Vec<&str> = content.lines().collect();
        let mut module_decls: Vec<(u32, String)> = Vec::new();

        for (line_num, line) in lines.iter().enumerate() {
            let trimmed = line.trim();

            // Check import statements
            if let Some(import_name) = extract_import_name(trimmed) {
                // Validate module name format
                if import_name.is_empty() {
                    diagnostics.push(make_diagnostic(
                        line_num as u32,
                        0,
                        DiagnosticSeverity::Error,
                        "empty module name in import statement",
                        "cmod-import",
                    ));
                }

                // Check for known module existence (if project root set)
                if let Some(ref root) = self.project_root {
                    if !import_name.starts_with("std") && !self.is_known_module(&import_name, root)
                    {
                        diagnostics.push(make_diagnostic(
                            line_num as u32,
                            0,
                            DiagnosticSeverity::Warning,
                            &format!(
                                "module '{}' not found in dependencies; add with `cmod add`",
                                import_name
                            ),
                            "cmod-unknown-import",
                        ));
                    }
                }
            }

            // Check for common module declaration issues
            if trimmed.starts_with("export module") && !trimmed.ends_with(';') {
                diagnostics.push(make_diagnostic(
                    line_num as u32,
                    0,
                    DiagnosticSeverity::Error,
                    "module declaration should end with semicolon",
                    "cmod-syntax",
                ));
            }

            // Track duplicate module declarations
            if trimmed.starts_with("export module ") && trimmed.ends_with(';') {
                let decl_name = trimmed
                    .trim_start_matches("export module ")
                    .trim_end_matches(';')
                    .trim();
                if !decl_name.is_empty() {
                    module_decls.push((line_num as u32, decl_name.to_string()));
                }
            }
        }

        // Report duplicate module declarations
        if module_decls.len() > 1 {
            for (line, name) in &module_decls[1..] {
                diagnostics.push(make_diagnostic(
                    *line,
                    0,
                    DiagnosticSeverity::Error,
                    &format!(
                        "duplicate module declaration '{}'; only one export module declaration is allowed per file",
                        name
                    ),
                    "cmod-duplicate-module",
                ));
            }
        }

        diagnostics
    }

    fn is_known_module(&self, name: &str, root: &Path) -> bool {
        let manifest_path = root.join("cmod.toml");
        if let Ok(content) = std::fs::read_to_string(&manifest_path) {
            if let Ok(manifest) = cmod_core::manifest::Manifest::from_str(&content) {
                // Check dependencies
                if manifest.dependencies.contains_key(name) {
                    return true;
                }
                // Check local module
                if let Some(ref module) = manifest.module {
                    if module.name == name {
                        return true;
                    }
                }
            }
        }
        false
    }
}

impl Default for DiagnosticsEngine {
    fn default() -> Self {
        Self::new()
    }
}

/// A parsed Clang diagnostic from build output.
#[derive(Debug, Clone)]
pub struct ClangDiagnostic {
    /// Source file path.
    pub file: String,
    /// 1-based line number.
    pub line: u32,
    /// 1-based column number.
    pub column: u32,
    /// Severity string ("error", "warning", "note").
    pub severity: String,
    /// Diagnostic message.
    pub message: String,
}

/// Parse Clang-format diagnostics from build output.
///
/// Recognizes lines in the format: `file:line:col: severity: message`
pub fn parse_clang_diagnostics(build_output: &str) -> Vec<ClangDiagnostic> {
    let mut diagnostics = Vec::new();

    for line in build_output.lines() {
        if let Some(diag) = parse_clang_diagnostic_line(line) {
            diagnostics.push(diag);
        }
    }

    diagnostics
}

/// Parse a single Clang diagnostic line.
fn parse_clang_diagnostic_line(line: &str) -> Option<ClangDiagnostic> {
    // Format: file:line:col: severity: message
    // Example: src/main.cpp:10:5: error: undeclared identifier 'foo'
    let trimmed = line.trim();

    // Skip non-diagnostic lines
    if trimmed.is_empty() || trimmed.starts_with("In file included") {
        return None;
    }

    // Find the pattern: ":line:col: severity:"
    let parts: Vec<&str> = trimmed.splitn(4, ':').collect();
    if parts.len() < 4 {
        return None;
    }

    let file = parts[0].to_string();
    let line_num: u32 = parts[1].trim().parse().ok()?;
    let col_num: u32 = parts[2].trim().parse().ok()?;

    let rest = parts[3].trim();
    // rest should be "severity: message"
    let (severity, message) = if let Some(idx) = rest.find(':') {
        let sev = rest[..idx].trim().to_string();
        let msg = rest[idx + 1..].trim().to_string();
        // Validate severity is a known keyword
        if matches!(sev.as_str(), "error" | "warning" | "note" | "fatal error") {
            (sev, msg)
        } else {
            return None;
        }
    } else {
        return None;
    };

    Some(ClangDiagnostic {
        file,
        line: line_num,
        column: col_num,
        severity,
        message,
    })
}

/// Convert parsed Clang diagnostics into LSP diagnostic values grouped by file.
pub fn clang_diagnostics_to_lsp(
    diagnostics: &[ClangDiagnostic],
) -> std::collections::BTreeMap<String, Vec<Value>> {
    let mut by_file: std::collections::BTreeMap<String, Vec<Value>> =
        std::collections::BTreeMap::new();

    for diag in diagnostics {
        let severity = match diag.severity.as_str() {
            "error" | "fatal error" => DiagnosticSeverity::Error,
            "warning" => DiagnosticSeverity::Warning,
            "note" => DiagnosticSeverity::Hint,
            _ => DiagnosticSeverity::Information,
        };

        let lsp_diag = make_diagnostic(
            diag.line.saturating_sub(1), // Convert to 0-based
            diag.column.saturating_sub(1),
            severity,
            &diag.message,
            "clang",
        );

        by_file.entry(diag.file.clone()).or_default().push(lsp_diag);
    }

    by_file
}

fn is_cpp_source(filename: &str) -> bool {
    let extensions = [".cpp", ".cppm", ".cxx", ".cc", ".c++", ".ixx", ".mxx"];
    extensions.iter().any(|ext| filename.ends_with(ext))
}

fn extract_import_name(line: &str) -> Option<String> {
    let trimmed = line.trim();
    if !trimmed.starts_with("import") {
        return None;
    }

    let rest = trimmed.strip_prefix("import")?.trim();
    let name = rest.trim_end_matches(';').trim();

    if name.starts_with('<') || name.starts_with('"') {
        return None; // Header import, not module import
    }

    Some(name.to_string())
}

fn make_diagnostic(
    line: u32,
    character: u32,
    severity: DiagnosticSeverity,
    message: &str,
    code: &str,
) -> Value {
    serde_json::json!({
        "range": {
            "start": { "line": line, "character": character },
            "end": { "line": line, "character": character + 1 },
        },
        "severity": severity as u8,
        "source": "cmod",
        "message": message,
        "code": code,
    })
}

/// Propagate diagnostics through the module DAG.
///
/// When a module has errors, all modules that depend on it should receive
/// informational diagnostics noting they may be affected.
pub fn propagate_diagnostics(
    broken_file: &Path,
    root: &Path,
    graph: &Option<cmod_build::graph::ModuleGraph>,
) -> std::collections::BTreeMap<PathBuf, Vec<Value>> {
    let mut result: std::collections::BTreeMap<PathBuf, Vec<Value>> =
        std::collections::BTreeMap::new();

    // Find the module name for the broken file
    let broken_module = match cmod_build::runner::extract_module_name(broken_file) {
        Ok(Some(name)) => name,
        _ => return result,
    };

    // Use the provided graph, or build a minimal one
    let owned_graph;
    let graph_ref = if let Some(g) = graph {
        g
    } else {
        // Try to build a graph from the project root
        let manifest_path = root.join("cmod.toml");
        let content = match std::fs::read_to_string(&manifest_path) {
            Ok(c) => c,
            Err(_) => return result,
        };
        let manifest = match cmod_core::manifest::Manifest::from_str(&content) {
            Ok(m) => m,
            Err(_) => return result,
        };
        let src_dirs: Vec<PathBuf> = manifest
            .build
            .as_ref()
            .map(|b| {
                if b.sources.is_empty() {
                    vec![root.join("src")]
                } else {
                    b.sources.iter().map(|s| root.join(s)).collect()
                }
            })
            .unwrap_or_else(|| vec![root.join("src")]);
        let exclude = manifest
            .build
            .as_ref()
            .map(|b| b.exclude.clone())
            .unwrap_or_default();

        let sources = match cmod_build::runner::discover_sources_multi(&src_dirs, &exclude) {
            Ok(s) => s,
            Err(_) => return result,
        };

        let mut g = cmod_build::graph::ModuleGraph::new();
        for source in &sources {
            if let Ok(Some(name)) = cmod_build::runner::extract_module_name(source) {
                let kind = cmod_build::runner::classify_source(source)
                    .unwrap_or(cmod_core::types::ModuleUnitKind::LegacyUnit);
                let partition_of = cmod_build::runner::extract_partition_owner(source)
                    .ok()
                    .flatten();
                let imports = cmod_build::runner::extract_imports(source)
                    .unwrap_or_default();
                let node = cmod_build::graph::ModuleNode {
                    id: source.display().to_string(),
                    name,
                    kind,
                    source: source.clone(),
                    package: String::new(),
                    imports,
                    partition_of,
                };
                g.add_node(node);
            }
        }
        owned_graph = g;
        &owned_graph
    };

    // Find all nodes that import the broken module
    for node in graph_ref.nodes.values() {
        if node.imports.contains(&broken_module) {
            let diag = make_diagnostic(
                0,
                0,
                DiagnosticSeverity::Information,
                &format!(
                    "Module '{}' has errors; this module may be affected",
                    broken_module
                ),
                "cmod-propagated",
            );
            result.entry(node.source.clone()).or_default().push(diag);
        }
    }

    result
}

fn find_error_line(error_msg: &str, _content: &str) -> Option<u32> {
    // Try to extract line number from error message
    for part in error_msg.split_whitespace() {
        if let Some(line_str) = part.strip_prefix("line") {
            if let Ok(line) = line_str.trim_matches(':').parse::<u32>() {
                return Some(line.saturating_sub(1)); // Convert to 0-based
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_diagnostics_engine_new() {
        let engine = DiagnosticsEngine::new();
        assert!(engine.project_root.is_none());
    }

    #[test]
    fn test_extract_import_name() {
        assert_eq!(extract_import_name("import std;"), Some("std".to_string()));
        assert_eq!(
            extract_import_name("import github.fmtlib.fmt;"),
            Some("github.fmtlib.fmt".to_string())
        );
        assert_eq!(
            extract_import_name("import my.module:partition;"),
            Some("my.module:partition".to_string())
        );
        assert_eq!(extract_import_name("import <iostream>;"), None);
        assert_eq!(extract_import_name("import \"header.h\";"), None);
        assert_eq!(extract_import_name("int x = 0;"), None);
    }

    #[test]
    fn test_is_cpp_source() {
        assert!(is_cpp_source("main.cpp"));
        assert!(is_cpp_source("lib.cppm"));
        assert!(is_cpp_source("module.ixx"));
        assert!(!is_cpp_source("main.rs"));
        assert!(!is_cpp_source("cmod.toml"));
    }

    #[test]
    fn test_diagnose_source_empty_import() {
        let engine = DiagnosticsEngine::new();
        let diagnostics = engine.diagnose_source_content("import ;\n");
        assert!(diagnostics.iter().any(|d| {
            d.get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("")
                .contains("empty module name")
        }));
    }

    #[test]
    fn test_make_diagnostic() {
        let d = make_diagnostic(5, 0, DiagnosticSeverity::Error, "test error", "test-code");
        assert_eq!(d["severity"], 1);
        assert_eq!(d["message"], "test error");
        assert_eq!(d["code"], "test-code");
    }

    #[test]
    fn test_parse_clang_diagnostic_line() {
        let diag =
            parse_clang_diagnostic_line("src/main.cpp:10:5: error: undeclared identifier 'foo'")
                .unwrap();
        assert_eq!(diag.file, "src/main.cpp");
        assert_eq!(diag.line, 10);
        assert_eq!(diag.column, 5);
        assert_eq!(diag.severity, "error");
        assert_eq!(diag.message, "undeclared identifier 'foo'");
    }

    #[test]
    fn test_parse_clang_diagnostic_warning() {
        let diag =
            parse_clang_diagnostic_line("lib.cppm:3:1: warning: unused variable 'x'").unwrap();
        assert_eq!(diag.severity, "warning");
        assert_eq!(diag.line, 3);
    }

    #[test]
    fn test_parse_clang_diagnostic_not_diagnostic() {
        assert!(parse_clang_diagnostic_line("Building module...").is_none());
        assert!(parse_clang_diagnostic_line("").is_none());
        assert!(parse_clang_diagnostic_line("In file included from header.h").is_none());
    }

    #[test]
    fn test_parse_clang_diagnostics_multi() {
        let output =
            "src/a.cpp:1:1: error: missing semicolon\nsrc/b.cpp:5:3: warning: shadowed variable\n";
        let diags = parse_clang_diagnostics(output);
        assert_eq!(diags.len(), 2);
        assert_eq!(diags[0].file, "src/a.cpp");
        assert_eq!(diags[1].severity, "warning");
    }

    #[test]
    fn test_diagnose_duplicate_module_declaration() {
        let engine = DiagnosticsEngine::new();
        let diagnostics =
            engine.diagnose_source_content("export module mymod;\nexport module other;\n");
        assert!(diagnostics.iter().any(|d| {
            d.get("code").and_then(|c| c.as_str()).unwrap_or("") == "cmod-duplicate-module"
        }));
    }

    #[test]
    fn test_diagnose_no_duplicate_single_declaration() {
        let engine = DiagnosticsEngine::new();
        let diagnostics = engine.diagnose_source_content("export module mymod;\nimport std;\n");
        assert!(!diagnostics.iter().any(|d| {
            d.get("code").and_then(|c| c.as_str()).unwrap_or("") == "cmod-duplicate-module"
        }));
    }

    #[test]
    fn test_clang_diagnostics_to_lsp() {
        let diags = vec![
            ClangDiagnostic {
                file: "src/main.cpp".into(),
                line: 10,
                column: 5,
                severity: "error".into(),
                message: "test error".into(),
            },
            ClangDiagnostic {
                file: "src/main.cpp".into(),
                line: 20,
                column: 1,
                severity: "warning".into(),
                message: "test warning".into(),
            },
            ClangDiagnostic {
                file: "src/lib.cpp".into(),
                line: 1,
                column: 1,
                severity: "note".into(),
                message: "a note".into(),
            },
        ];
        let by_file = clang_diagnostics_to_lsp(&diags);
        assert_eq!(by_file.len(), 2);
        assert_eq!(by_file["src/main.cpp"].len(), 2);
        assert_eq!(by_file["src/lib.cpp"].len(), 1);
    }

    #[test]
    fn test_propagate_diagnostics_no_dependents() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        std::fs::write(
            tmp.path().join("cmod.toml"),
            "[package]\nname = \"test\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        let broken = tmp.path().join("src").join("lib.cppm");
        std::fs::write(&broken, "export module mylib;\n").unwrap();

        // No graph, no dependents → empty
        let result = propagate_diagnostics(&broken, tmp.path(), &None);
        assert!(result.is_empty());
    }

    #[test]
    fn test_propagate_diagnostics_with_dependents() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();
        std::fs::write(
            tmp.path().join("cmod.toml"),
            "[package]\nname = \"test\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        let broken = tmp.path().join("src").join("base.cppm");
        std::fs::write(&broken, "export module base;\n").unwrap();

        let dep1 = tmp.path().join("src").join("dep1.cppm");
        std::fs::write(&dep1, "export module dep1;\nimport base;\n").unwrap();

        let dep2 = tmp.path().join("src").join("dep2.cppm");
        std::fs::write(&dep2, "export module dep2;\nimport base;\n").unwrap();

        // Build a graph with imports
        let mut graph = cmod_build::graph::ModuleGraph::new();
        graph.add_node(cmod_build::graph::ModuleNode {
            id: broken.display().to_string(),
            name: "base".into(),
            kind: cmod_core::types::ModuleUnitKind::InterfaceUnit,
            source: broken.clone(),
            package: "test".into(),
            imports: vec![],
            partition_of: None,
        });
        graph.add_node(cmod_build::graph::ModuleNode {
            id: dep1.display().to_string(),
            name: "dep1".into(),
            kind: cmod_core::types::ModuleUnitKind::InterfaceUnit,
            source: dep1.clone(),
            package: "test".into(),
            imports: vec!["base".into()],
            partition_of: None,
        });
        graph.add_node(cmod_build::graph::ModuleNode {
            id: dep2.display().to_string(),
            name: "dep2".into(),
            kind: cmod_core::types::ModuleUnitKind::InterfaceUnit,
            source: dep2.clone(),
            package: "test".into(),
            imports: vec!["base".into()],
            partition_of: None,
        });

        let result = propagate_diagnostics(&broken, tmp.path(), &Some(graph));
        // dep1 and dep2 both import "base", so both get propagated diagnostics
        assert_eq!(result.len(), 2);
        assert!(result.contains_key(&dep1));
        assert!(result.contains_key(&dep2));
    }
}
