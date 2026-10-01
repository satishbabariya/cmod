use std::sync::Arc;

use cmod_build::graph::ModuleNode;
use cmod_build::runner::{self, DryRunEntry, DryRunReport};
use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::shell::{Shell, Verbosity};
use cmod_core::types::Profile;

/// Run `cmod explain <module>` — explain why a module would be rebuilt.
pub fn run(module_name: String, shell: &Shell) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;
    let verbose = shell.verbosity() == Verbosity::Verbose;

    let src_dirs = config.src_dirs();
    let exclude = config.exclude_patterns();
    let sources = runner::discover_sources_multi(&src_dirs, &exclude)?;

    // Find the module
    let mut found_node: Option<ModuleNode> = None;
    for source in &sources {
        if let Ok(Some(name)) = runner::extract_module_name(source) {
            if name == module_name {
                let kind = runner::classify_source(source)?;
                found_node = Some(ModuleNode {
                    id: name.clone(),
                    name: name.clone(),
                    kind,
                    source: source.clone(),
                    package: config.manifest.package.name.clone(),
                    imports: vec![],
                    partition_of: None,
                });
                break;
            }
        }
        // Also check by filename stem
        if let Some(stem) = source.file_stem().and_then(|s| s.to_str()) {
            if stem == module_name {
                let kind = runner::classify_source(source)?;
                found_node = Some(ModuleNode {
                    id: module_name.clone(),
                    name: module_name.clone(),
                    kind,
                    source: source.clone(),
                    package: config.manifest.package.name.clone(),
                    imports: vec![],
                    partition_of: None,
                });
                break;
            }
        }
    }

    let node = found_node.ok_or_else(|| {
        CmodError::Other(format!("module '{}' not found in source tree", module_name))
    })?;

    println!("Module: {}", node.name);
    println!("Source: {}", node.source.display());
    println!("Kind:   {:?}", node.kind);
    println!();

    // Ask the build itself: a dry run makes the same decisions `cmod build`
    // would (sources, headers, imported BMIs, flags, outputs), for this
    // package and its dependencies, without building anything.
    let report = Arc::new(DryRunReport::default());
    super::build::run(
        false,
        false,
        false,
        shell,
        None,
        0,
        false,
        None,
        true,
        false,
        false,
        &[],
        false,
        false,
        false,
        vec![],
        Some(report.clone()),
    )?;

    let entries: Vec<DryRunEntry> = report
        .entries()
        .into_iter()
        .filter(|e| {
            e.module.as_deref() == Some(module_name.as_str())
                || e.source.as_ref() == Some(&node.source)
        })
        .collect();

    let profile_name = match config.profile {
        Profile::Debug => "debug",
        Profile::Release => "release",
    };
    let reasons: Vec<String> = entries
        .iter()
        .filter_map(|e| {
            let reason = e.reason.as_ref()?;
            let source = e
                .source
                .as_ref()
                .map(|s| s.strip_prefix(&cwd).unwrap_or(s).display().to_string())
                .unwrap_or_default();
            Some(format!("{}: {}", source, reason))
        })
        .collect();

    if reasons.is_empty() {
        println!("  Status: UP TO DATE — no rebuild needed");
        println!("  Profile: {}", profile_name);
    } else {
        println!("  Status: NEEDS REBUILD");
        println!("  Profile: {}", profile_name);
        println!("  Reasons:");
        for (i, reason) in reasons.iter().enumerate() {
            println!("    {}. {}", i + 1, reason);
        }
    }
    if verbose {
        println!();
        super::build::print_dry_run(&report);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmod_core::shell::Shell;

    #[test]
    fn test_explain_module_not_found() {
        let tmp = tempfile::TempDir::new().unwrap();
        let toml = "[package]\nname = \"test\"\nversion = \"0.1.0\"\n";
        std::fs::write(tmp.path().join("cmod.toml"), toml).unwrap();
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();

        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        let shell = Shell::new(Verbosity::Normal);
        let result = run("nonexistent_module".to_string(), &shell);
        assert!(result.is_err());

        std::env::set_current_dir(original).unwrap();
    }
}
