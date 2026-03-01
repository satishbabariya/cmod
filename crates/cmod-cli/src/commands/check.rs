use std::fs;

use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::types::ModuleId;

/// Run `cmod check` — validate module naming, identity, and structure rules.
pub fn run(verbose: bool) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;
    let mut warnings = Vec::new();
    let mut errors = Vec::new();

    eprintln!("  Checking {} v{}", config.manifest.package.name, config.manifest.package.version);

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
        if module.name.starts_with("std.") || module.name.starts_with("std:") || module.name == "std" {
            errors.push(format!(
                "module name '{}' uses reserved 'std' prefix",
                module.name
            ));
        }
        if module.name.starts_with("stdx.") || module.name.starts_with("stdx:") || module.name == "stdx" {
            errors.push(format!(
                "module name '{}' uses reserved 'stdx' prefix",
                module.name
            ));
        }
    }

    // 4. Validate compat constraints against toolchain
    if let (Some(ref compat), Some(ref toolchain)) = (&config.manifest.compat, &config.manifest.toolchain) {
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

    // Report
    for w in &warnings {
        eprintln!("  warning: {}", w);
    }
    for e in &errors {
        eprintln!("  error: {}", e);
    }

    if errors.is_empty() {
        eprintln!("  All checks passed ({} warnings).", warnings.len());
        Ok(())
    } else {
        Err(CmodError::BuildFailed {
            reason: format!("{} check(s) failed", errors.len()),
        })
    }
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
}
