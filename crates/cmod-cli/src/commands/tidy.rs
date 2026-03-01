use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use cmod_core::config::Config;
use cmod_core::error::CmodError;

/// Run `cmod tidy` — find and optionally remove unused dependencies.
pub fn run(apply: bool, verbose: bool) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    // Collect all imports from source files
    let src_dir = config.src_dir();
    let imports = collect_all_imports(&src_dir)?;

    if verbose {
        eprintln!("  Found {} unique imports in source files", imports.len());
        for imp in &imports {
            eprintln!("    {}", imp);
        }
    }

    // Compare against declared dependencies
    let mut unused = Vec::new();
    for dep_name in config.manifest.dependencies.keys() {
        // A dep is "used" if any source file imports a module that matches
        // the dep name or a module that starts with the dep name.
        let is_used = imports.iter().any(|imp| {
            dep_matches_import(dep_name, imp)
        });

        if !is_used {
            unused.push(dep_name.clone());
        }
    }

    if unused.is_empty() {
        eprintln!("  All dependencies are used.");
        return Ok(());
    }

    eprintln!("  Unused dependencies ({}):", unused.len());
    for name in &unused {
        eprintln!("    - {}", name);
    }

    if apply {
        // Remove unused deps from cmod.toml
        let manifest_path = config.root.join("cmod.toml");
        let mut manifest = config.manifest.clone();
        for name in &unused {
            manifest.dependencies.remove(name);
        }
        manifest.save(&manifest_path)?;
        eprintln!("  Removed {} unused dependencies from cmod.toml", unused.len());
    } else {
        eprintln!("  Run `cmod tidy --apply` to remove them.");
    }

    Ok(())
}

/// Collect all module imports from C++ source files in a directory.
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
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            match ext {
                "cppm" | "ixx" | "mpp" | "cpp" | "cc" | "cxx" => {
                    let content = fs::read_to_string(path)?;
                    for line in content.lines() {
                        let trimmed = line.trim();
                        if trimmed.starts_with("import ") && trimmed.ends_with(';') {
                            let module_name = trimmed
                                .trim_start_matches("import ")
                                .trim_end_matches(';')
                                .trim();
                            // Skip header unit imports
                            if !module_name.starts_with('<') && !module_name.starts_with('"') {
                                imports.insert(module_name.to_string());
                            }
                        }
                    }
                }
                _ => {}
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
    let short_name = dep_name
        .rsplit('/')
        .next()
        .unwrap_or(dep_name);

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
        std::fs::write(
            &file,
            "export module app;\nimport fmt;\nimport spdlog;\n",
        ).unwrap();

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
}
