use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use cmod_build::graph::{ModuleGraph, ModuleNode};
use cmod_build::runner;
use cmod_core::config::Config;
use cmod_core::error::CmodError;

/// Run `cmod tidy` — find and optionally remove unused dependencies.
pub fn run(apply: bool, verbose: bool) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    let src_dir = config.src_dir();

    // Try to build a module graph for accurate import analysis.
    // Fall back to text-based scanning if graph construction fails.
    let imports = match build_import_set(&src_dir, &config.manifest.package.name) {
        Ok(imports) => {
            if verbose {
                eprintln!("  Using module graph for import analysis");
            }
            imports
        }
        Err(_) => {
            if verbose {
                eprintln!("  Module graph unavailable, falling back to text-based import scanning");
            }
            collect_all_imports(&src_dir)?
        }
    };

    if verbose {
        eprintln!("  Found {} unique imports in source files", imports.len());
        for imp in &imports {
            eprintln!("    {}", imp);
        }
    }

    // Compare against declared dependencies
    let mut unused = Vec::new();
    for (dep_name, dep) in &config.manifest.dependencies {
        // A dep is "used" if any source file imports a module that matches
        // the dep name or a module that starts with the dep name.
        let is_used = imports.iter().any(|imp| dep_matches_import(dep_name, imp));

        if !is_used {
            let source_info = dep
                .git_url()
                .map(|u| format!(" ({})", u))
                .or_else(|| dep.path().map(|p| format!(" (path: {})", p.display())))
                .unwrap_or_default();
            unused.push((dep_name.clone(), source_info));
        }
    }

    if unused.is_empty() {
        eprintln!("  All dependencies are used.");
        return Ok(());
    }

    eprintln!("  Unused dependencies ({}):", unused.len());
    for (name, source) in &unused {
        eprintln!("    - {}{}", name, source);
    }

    if apply {
        // Remove unused deps from cmod.toml
        let manifest_path = config.root.join("cmod.toml");
        let mut manifest = config.manifest.clone();
        for (name, _) in &unused {
            manifest.dependencies.remove(name);
        }
        manifest.save(&manifest_path)?;
        eprintln!(
            "  Removed {} unused dependencies from cmod.toml",
            unused.len()
        );
    } else {
        eprintln!("  Run `cmod tidy --apply` to remove them.");
    }

    Ok(())
}

/// Build a set of all imported module names using the module graph.
///
/// This is more accurate than text scanning because it uses the same
/// classification and parsing logic as the build system.
fn build_import_set(src_dir: &Path, package_name: &str) -> Result<BTreeSet<String>, CmodError> {
    let sources = runner::discover_sources(src_dir)?;
    if sources.is_empty() {
        return Ok(BTreeSet::new());
    }

    let mut graph = ModuleGraph::new();

    for source in &sources {
        let kind = runner::classify_source(source)?;
        let module_name = runner::extract_module_name(source)?.unwrap_or_else(|| {
            source
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string()
        });

        let imports = extract_imports_from_source(source)?;
        let partition_of = runner::extract_partition_owner(source)?;
        let node_id = source.display().to_string();

        graph.add_node(ModuleNode {
            id: node_id,
            name: module_name,
            kind,
            source: source.clone(),
            package: package_name.to_string(),
            imports,
            partition_of,
        });
    }

    // Collect ALL imports (including external ones that are NOT in the graph).
    // This is the opposite of what build does — we want external imports.
    let mut all_imports = BTreeSet::new();
    for node in graph.nodes.values() {
        for import in &node.imports {
            all_imports.insert(import.clone());
        }
    }

    Ok(all_imports)
}

/// Extract import statements from a C++ source file.
fn extract_imports_from_source(path: &Path) -> Result<Vec<String>, CmodError> {
    let content = fs::read_to_string(path)?;
    let mut imports = Vec::new();
    let mut in_block_comment = false;

    for line in content.lines() {
        let trimmed = line.trim();

        // Track block comments
        if in_block_comment {
            if let Some(pos) = trimmed.find("*/") {
                let rest = trimmed[pos + 2..].trim();
                in_block_comment = false;
                if let Some(name) = parse_import_line(rest) {
                    imports.push(name);
                }
            }
            continue;
        }

        if trimmed.contains("/*") && !trimmed.contains("*/") {
            in_block_comment = true;
            // Check text before the comment
            if let Some(before) = trimmed.split("/*").next() {
                if let Some(name) = parse_import_line(before.trim()) {
                    imports.push(name);
                }
            }
            continue;
        }

        // Skip single-line comments
        if trimmed.starts_with("//") {
            continue;
        }

        // Skip preprocessor directives
        if trimmed.starts_with('#') {
            continue;
        }

        if let Some(name) = parse_import_line(trimmed) {
            imports.push(name);
        }
    }

    Ok(imports)
}

/// Parse a single line for an `import <module>;` statement.
fn parse_import_line(line: &str) -> Option<String> {
    if line.starts_with("import ") && line.ends_with(';') {
        let module_name = line
            .trim_start_matches("import ")
            .trim_end_matches(';')
            .trim();
        // Skip header unit imports
        if !module_name.starts_with('<') && !module_name.starts_with('"') && !module_name.is_empty()
        {
            return Some(module_name.to_string());
        }
    }
    None
}

/// Collect all module imports from C++ source files using text scanning.
///
/// This is the fallback when module graph construction fails.
fn collect_all_imports(src_dir: &Path) -> Result<BTreeSet<String>, CmodError> {
    let mut imports = BTreeSet::new();

    if !src_dir.exists() {
        return Ok(imports);
    }

    for entry in walkdir::WalkDir::new(src_dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }
        if let Some("cppm" | "ixx" | "mpp" | "cpp" | "cc" | "cxx") =
            path.extension().and_then(|e| e.to_str())
        {
            let found = extract_imports_from_source(path)?;
            for name in found {
                imports.insert(name);
            }
        }
    }

    Ok(imports)
}

/// Check if a dependency name matches an import.
///
/// A dep like "github.com/fmtlib/fmt" matches imports like "fmt", "fmt.core", etc.
/// A dep like "mylib" matches imports like "mylib", "mylib.utils", etc.
fn dep_matches_import(dep_name: &str, import_name: &str) -> bool {
    // Extract the short name from a Git-style dep key
    let short_name = dep_name.rsplit('/').next().unwrap_or(dep_name);

    // Module name could be the short dep name or start with it (partition)
    import_name == short_name
        || import_name.starts_with(&format!("{}.", short_name))
        || import_name.starts_with(&format!("{}:", short_name))
        || import_name == dep_name
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_dep_matches_import_exact() {
        assert!(dep_matches_import("fmt", "fmt"));
        assert!(dep_matches_import("github.com/fmtlib/fmt", "fmt"));
    }

    #[test]
    fn test_dep_matches_import_partition() {
        assert!(dep_matches_import("fmt", "fmt.core"));
        assert!(dep_matches_import("fmt", "fmt:detail"));
    }

    #[test]
    fn test_dep_matches_import_no_match() {
        assert!(!dep_matches_import("fmt", "spdlog"));
        assert!(!dep_matches_import("github.com/fmtlib/fmt", "spdlog"));
    }

    #[test]
    fn test_collect_imports_empty_dir() {
        let tmp = TempDir::new().unwrap();
        let imports = collect_all_imports(tmp.path()).unwrap();
        assert!(imports.is_empty());
    }

    #[test]
    fn test_collect_imports_from_sources() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("main.cppm");
        std::fs::write(&file, "export module app;\nimport fmt;\nimport spdlog;\n").unwrap();

        let imports = collect_all_imports(tmp.path()).unwrap();
        assert_eq!(imports.len(), 2);
        assert!(imports.contains("fmt"));
        assert!(imports.contains("spdlog"));
    }

    #[test]
    fn test_collect_imports_nonexistent_dir() {
        let imports = collect_all_imports(Path::new("/nonexistent")).unwrap();
        assert!(imports.is_empty());
    }

    #[test]
    fn test_extract_imports_skips_comments() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("test.cppm");
        std::fs::write(
            &file,
            "export module test;\n// import commented;\nimport real;\n/* import blocked; */\n",
        )
        .unwrap();

        let imports = extract_imports_from_source(&file).unwrap();
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0], "real");
    }

    #[test]
    fn test_extract_imports_skips_header_units() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("test.cppm");
        std::fs::write(
            &file,
            "export module test;\nimport <iostream>;\nimport \"myheader.h\";\nimport fmt;\n",
        )
        .unwrap();

        let imports = extract_imports_from_source(&file).unwrap();
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0], "fmt");
    }

    #[test]
    fn test_parse_import_line() {
        assert_eq!(parse_import_line("import fmt;"), Some("fmt".to_string()));
        assert_eq!(parse_import_line("import <iostream>;"), None);
        assert_eq!(parse_import_line("// import hidden;"), None);
        assert_eq!(parse_import_line("export module test;"), None);
    }
}
