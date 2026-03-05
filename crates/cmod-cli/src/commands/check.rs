use std::fs;

use cmod_build::graph::{ModuleGraph, ModuleNode};
use cmod_build::runner;
use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::types::ModuleId;

/// Run `cmod check` — validate module naming, identity, structure, and semantics.
pub fn run(verbose: bool) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;
    let mut warnings = Vec::new();
    let mut errors = Vec::new();

    eprintln!(
        "{:>12} {} v{}",
        "Checking", config.manifest.package.name, config.manifest.package.version
    );

    // 1. Check module name matches export declaration
    if let Some(ref module) = config.manifest.module {
        let root_file = config.root.join(&module.root);
        if root_file.exists() {
            let content = fs::read_to_string(&root_file)?;
            let declared_name = extract_export_module_name(&content);
            if let Some(ref declared) = declared_name {
                if declared != &module.name {
                    errors.push(format!(
                        "module name mismatch: cmod.toml declares '{}' but source declares 'export module {};'",
                        module.name, declared
                    ));
                }
            } else if verbose {
                warnings.push(format!(
                    "no 'export module' declaration found in {}",
                    root_file.display()
                ));
            }
        } else if verbose {
            warnings.push(format!(
                "module root file not found: {}",
                root_file.display()
            ));
        }

        // 2. Check module name follows reverse-domain convention
        if let Some(ref repo) = config.manifest.package.repository {
            if let Some(module_id) = ModuleId::from_git_url(repo) {
                let expected_prefix = module_id.to_string();
                if !module.name.starts_with(&expected_prefix) && verbose {
                    warnings.push(format!(
                        "module name '{}' does not match reverse-domain from repository '{}' (expected prefix: '{}')",
                        module.name, repo, expected_prefix
                    ));
                }
            }
        }

        // 3. Check for reserved prefixes
        if module.name.starts_with("std.")
            || module.name.starts_with("std:")
            || module.name == "std"
        {
            errors.push(format!(
                "module name '{}' uses reserved 'std' prefix",
                module.name
            ));
        }
        if module.name.starts_with("stdx.")
            || module.name.starts_with("stdx:")
            || module.name == "stdx"
        {
            errors.push(format!(
                "module name '{}' uses reserved 'stdx' prefix",
                module.name
            ));
        }
    }

    // 4. Validate compat constraints against toolchain
    if let (Some(ref compat), Some(ref toolchain)) =
        (&config.manifest.compat, &config.manifest.toolchain)
    {
        if let (Some(ref req_cpp), Some(ref tc_std)) = (&compat.cpp, &toolchain.cxx_standard) {
            // Simple numeric comparison for C++ standards
            let req_num = extract_std_version(req_cpp);
            let tc_num = extract_std_version(tc_std);
            if let (Some(req), Some(tc)) = (req_num, tc_num) {
                if tc < req {
                    errors.push(format!(
                        "toolchain C++ standard '{}' does not satisfy compat requirement '{}'",
                        tc_std, req_cpp
                    ));
                }
            }
        }
    }

    // 5. Check that dependencies don't use reserved prefixes
    for dep_name in config.manifest.dependencies.keys() {
        let short = dep_name.rsplit('/').next().unwrap_or(dep_name);
        if short == "std" || short.starts_with("std.") {
            errors.push(format!(
                "dependency '{}' uses reserved 'std' prefix",
                dep_name
            ));
        }
    }

    // 6. Semantic validation: build module graph and validate
    validate_module_graph(&config, &mut errors, &mut warnings, verbose);

    // Report
    for w in &warnings {
        eprintln!("{:>12} {}", "warning", w);
    }
    for e in &errors {
        eprintln!("{:>12} {}", "error", e);
    }

    if errors.is_empty() {
        eprintln!(
            "{:>12} all checks passed ({} warnings)",
            "Finished",
            warnings.len()
        );
        Ok(())
    } else {
        Err(CmodError::BuildFailed {
            reason: format!("{} check(s) failed", errors.len()),
        })
    }
}

/// Build module graph from sources and run semantic validation.
fn validate_module_graph(
    config: &Config,
    errors: &mut Vec<String>,
    warnings: &mut Vec<String>,
    verbose: bool,
) {
    // Skip for workspace roots without a module
    if config.manifest.is_workspace() && config.manifest.module.is_none() {
        return;
    }

    let src_dir = config.src_dir();
    if !src_dir.exists() {
        return; // No sources — already checked elsewhere
    }

    if verbose {
        eprintln!("{:>12} module graph for validation", "Building");
    }

    // Discover and classify sources
    let sources = match runner::discover_sources(&src_dir) {
        Ok(s) => s,
        Err(e) => {
            warnings.push(format!("could not discover sources: {}", e));
            return;
        }
    };

    if sources.is_empty() {
        return;
    }

    // Classify each source file
    let mut classification_errors = Vec::new();
    for source in &sources {
        if let Err(e) = runner::classify_source(source) {
            classification_errors.push(format!("{}: {}", source.display(), e));
        }
    }
    if !classification_errors.is_empty() {
        warnings.push(format!(
            "source classification issues ({}):",
            classification_errors.len()
        ));
        for ce in &classification_errors {
            warnings.push(format!("  {}", ce));
        }
    }

    // Build the module graph
    let mut graph = ModuleGraph::new();
    for source in &sources {
        let kind = match runner::classify_source(source) {
            Ok(k) => k,
            Err(_) => continue,
        };
        let module_name = match runner::extract_module_name(source) {
            Ok(Some(name)) => name,
            Ok(None) => source
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string(),
            Err(_) => continue,
        };

        let imports = extract_imports(source).unwrap_or_default();
        let partition_of = runner::extract_partition_owner(source).ok().flatten();
        let node_id = source.display().to_string();

        graph.add_node(ModuleNode {
            id: node_id,
            name: module_name,
            kind,
            source: source.clone(),
            package: config.manifest.package.name.clone(),
            imports,
            partition_of,
        });
    }

    if verbose {
        eprintln!(
            "    Graph: {} nodes, {} modules",
            graph.nodes.len(),
            graph.module_names().len()
        );
    }

    // Validate graph: cycles, duplicate interfaces, imports
    // Filter imports to known modules first (same as build does)
    let known_modules = graph.module_names();

    // Collect external imports (those NOT in the graph) for dep validation
    let mut external_imports = std::collections::BTreeSet::new();
    for node in graph.nodes.values() {
        for import in &node.imports {
            if !known_modules.contains(import) {
                external_imports.insert(import.clone());
            }
        }
    }

    // Warn about external imports that don't match any declared dependency.
    // For path dependencies, also check the module name declared in the dep's cmod.toml.
    for ext_import in &external_imports {
        let has_dep = config.manifest.dependencies.keys().any(|dep_name| {
            let short = dep_name.rsplit('/').next().unwrap_or(dep_name);
            // Direct match against dependency short name
            if ext_import == short
                || ext_import.starts_with(&format!("{}.", short))
                || ext_import.starts_with(&format!("{}:", short))
            {
                return true;
            }
            // For path dependencies, check if the import matches the module name
            // declared in the dependency's own cmod.toml (e.g., "local.colors")
            if let Some(dep_value) = config.manifest.dependencies.get(dep_name) {
                if let Some(path) = dep_value.path() {
                    let dep_dir = config.root.join(path);
                    if let Ok(dep_config) = cmod_core::config::Config::load(&dep_dir) {
                        if let Some(ref module) = dep_config.manifest.module {
                            if ext_import == &module.name
                                || ext_import.starts_with(&format!("{}.", module.name))
                                || ext_import.starts_with(&format!("{}:", module.name))
                            {
                                return true;
                            }
                        }
                    }
                }
            }
            false
        });
        if !has_dep {
            warnings.push(format!(
                "import '{}' does not match any declared dependency",
                ext_import
            ));
        }
    }

    // Filter imports for validation (only internal)
    for node in graph.nodes.values_mut() {
        node.imports.retain(|imp| known_modules.contains(imp));
    }

    // Run graph validation
    match graph.validate() {
        Ok(()) => {
            if verbose {
                eprintln!("    Module graph validation passed");
            }
        }
        Err(e) => {
            errors.push(format!("module graph validation failed: {}", e));
        }
    }

    // Check partition ownership consistency
    validate_partitions(&graph, errors, verbose);
}

/// Validate partition ownership: each partition must have an owning interface.
fn validate_partitions(graph: &ModuleGraph, errors: &mut Vec<String>, verbose: bool) {
    let known_modules = graph.module_names();

    for node in graph.nodes.values() {
        if let Some(ref owner) = node.partition_of {
            // Check the owning module exists in the graph
            if !known_modules.contains(owner) {
                errors.push(format!(
                    "partition '{}' claims owner '{}', but no such module exists",
                    node.name, owner
                ));
            } else if graph.interface_for(owner).is_none() {
                errors.push(format!(
                    "partition '{}' owner '{}' has no interface unit",
                    node.name, owner
                ));
            } else if verbose {
                eprintln!("    partition '{}' -> owner '{}' OK", node.name, owner);
            }
        }
    }
}

/// Extract import statements from a C++ source file.
fn extract_imports(path: &std::path::Path) -> Result<Vec<String>, CmodError> {
    let content = std::fs::read_to_string(path)?;
    let mut imports = Vec::new();
    let mut in_block_comment = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if in_block_comment {
            if trimmed.contains("*/") {
                in_block_comment = false;
            }
            continue;
        }

        if trimmed.contains("/*") && !trimmed.contains("*/") {
            in_block_comment = true;
            continue;
        }

        if trimmed.starts_with("//") || trimmed.starts_with('#') {
            continue;
        }

        if trimmed.starts_with("import ") && trimmed.ends_with(';') {
            let module_name = trimmed
                .trim_start_matches("import ")
                .trim_end_matches(';')
                .trim();
            if !module_name.starts_with('<')
                && !module_name.starts_with('"')
                && !module_name.is_empty()
            {
                imports.push(module_name.to_string());
            }
        }
    }

    Ok(imports)
}

/// Extract the module name from an `export module <name>;` declaration.
fn extract_export_module_name(source: &str) -> Option<String> {
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("export module ") && trimmed.ends_with(';') {
            let name = trimmed
                .trim_start_matches("export module ")
                .trim_end_matches(';')
                .trim();
            if !name.is_empty() {
                return Some(name.to_string());
            }
        }
    }
    None
}

/// Extract a numeric C++ standard version (e.g., "c++20" -> 20, "23" -> 23).
fn extract_std_version(s: &str) -> Option<u32> {
    let stripped = s
        .trim()
        .trim_start_matches("c++")
        .trim_start_matches("C++")
        .trim_start_matches(">=");
    stripped.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_export_module_name() {
        assert_eq!(
            extract_export_module_name("export module mylib;"),
            Some("mylib".to_string())
        );
        assert_eq!(
            extract_export_module_name("// comment\nexport module com.github.user.lib;\n"),
            Some("com.github.user.lib".to_string())
        );
        assert_eq!(extract_export_module_name("import foo;"), None);
        assert_eq!(extract_export_module_name(""), None);
    }

    #[test]
    fn test_extract_std_version() {
        assert_eq!(extract_std_version("c++20"), Some(20));
        assert_eq!(extract_std_version("C++23"), Some(23));
        assert_eq!(extract_std_version("20"), Some(20));
        assert_eq!(extract_std_version(">=23"), Some(23));
        assert_eq!(extract_std_version("latest"), None);
    }

    #[test]
    fn test_extract_imports_basic() {
        let tmp = tempfile::TempDir::new().unwrap();
        let file = tmp.path().join("test.cppm");
        std::fs::write(&file, "export module test;\nimport fmt;\nimport spdlog;\n").unwrap();

        let imports = extract_imports(&file).unwrap();
        assert_eq!(imports.len(), 2);
        assert_eq!(imports[0], "fmt");
        assert_eq!(imports[1], "spdlog");
    }

    #[test]
    fn test_extract_imports_skips_header_units() {
        let tmp = tempfile::TempDir::new().unwrap();
        let file = tmp.path().join("test.cppm");
        std::fs::write(
            &file,
            "export module test;\nimport <iostream>;\nimport \"header.h\";\nimport real;\n",
        )
        .unwrap();

        let imports = extract_imports(&file).unwrap();
        assert_eq!(imports.len(), 1);
        assert_eq!(imports[0], "real");
    }
}
