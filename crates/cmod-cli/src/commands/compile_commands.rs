use std::path::PathBuf;

use cmod_build::compiler::ClangBackend;
use cmod_build::graph::{ModuleGraph, ModuleNode};
use cmod_build::plan::BuildPlan;
use cmod_build::runner;
use cmod_core::config::Config;
use cmod_core::error::CmodError;

/// Run `cmod compile-commands` — generate a compile_commands.json without building.
pub fn run(verbose: bool, target_override: Option<String>) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let mut config = Config::load(&cwd)?;

    if let Some(t) = target_override {
        config.target = Some(t);
    }

    let src_dir = config.src_dir();
    let sources = runner::discover_sources(&src_dir)?;

    if sources.is_empty() {
        eprintln!("  No source files found in {}", src_dir.display());
        return Ok(());
    }

    let graph = build_module_graph(&sources, &config.manifest.package.name)?;
    graph.validate()?;

    let build_dir = config.build_dir();
    let build_type = config
        .manifest
        .build
        .as_ref()
        .and_then(|b| b.build_type)
        .unwrap_or_default();

    let (backend, target) = setup_compiler(&config);

    let plan = BuildPlan::from_graph(&graph, &build_dir, &target, config.profile, build_type)?;

    let commands = plan.compile_commands(&backend, &config.root);
    let json = serde_json::to_string_pretty(&commands).map_err(|e| CmodError::BuildFailed {
        reason: format!("failed to serialize compile_commands.json: {}", e),
    })?;

    let output_path = config.root.join("compile_commands.json");
    std::fs::write(&output_path, &json)?;

    eprintln!(
        "  Generated {} with {} entries",
        output_path.display(),
        commands.len()
    );

    if verbose {
        for cmd in &commands {
            eprintln!("    {}", cmd.file);
        }
    }

    Ok(())
}

/// Build a ModuleGraph from discovered source files (same logic as build.rs).
fn build_module_graph(sources: &[PathBuf], package_name: &str) -> Result<ModuleGraph, CmodError> {
    let mut graph = ModuleGraph::new();

    for source in sources {
        let kind = runner::classify_source(source)?;
        let module_name = runner::extract_module_name(source)?.unwrap_or_else(|| {
            source
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string()
        });

        let imports = extract_imports_from_source(source)?;

        graph.add_node(ModuleNode {
            name: module_name,
            kind,
            source: source.clone(),
            package: package_name.to_string(),
            imports,
        });
    }

    // Filter imports to only include modules that exist in the graph
    let known_modules: std::collections::BTreeSet<String> = graph.nodes.keys().cloned().collect();
    for node in graph.nodes.values_mut() {
        node.imports.retain(|imp| known_modules.contains(imp));
    }

    Ok(graph)
}

/// Simple import extraction by scanning source content for `import` statements.
fn extract_imports_from_source(path: &std::path::Path) -> Result<Vec<String>, CmodError> {
    let content = std::fs::read_to_string(path)?;
    let mut imports = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("import ") && trimmed.ends_with(';') {
            let module_name = trimmed
                .trim_start_matches("import ")
                .trim_end_matches(';')
                .trim();
            if !module_name.starts_with('<') && !module_name.starts_with('"') {
                imports.push(module_name.to_string());
            }
        }
    }

    Ok(imports)
}

/// Set up the Clang compiler backend from config (same logic as build.rs).
fn setup_compiler(config: &Config) -> (ClangBackend, String) {
    let cxx_standard = config
        .manifest
        .toolchain
        .as_ref()
        .and_then(|tc| tc.cxx_standard.clone())
        .unwrap_or_else(|| "20".to_string());

    let mut backend = ClangBackend::new(&cxx_standard, config.profile);

    if let Some(ref tc) = config.manifest.toolchain {
        if let Some(ref stdlib) = tc.stdlib {
            backend.stdlib = Some(stdlib.clone());
        }
        if let Some(ref sysroot) = tc.sysroot {
            backend.sysroot = Some(sysroot.clone());
        }
    }

    let target = config
        .target
        .clone()
        .or_else(|| {
            config
                .manifest
                .toolchain
                .as_ref()
                .and_then(|tc| tc.target.clone())
        })
        .unwrap_or_else(default_target);

    backend.target = Some(target.clone());

    (backend, target)
}

/// Detect the default target triple for the current platform.
fn default_target() -> String {
    let arch = std::env::consts::ARCH;
    let os = std::env::consts::OS;

    match (arch, os) {
        ("x86_64", "linux") => "x86_64-unknown-linux-gnu".to_string(),
        ("x86_64", "macos") => "x86_64-apple-darwin".to_string(),
        ("aarch64", "linux") => "aarch64-unknown-linux-gnu".to_string(),
        ("aarch64", "macos") => "arm64-apple-darwin".to_string(),
        ("x86_64", "windows") => "x86_64-pc-windows-msvc".to_string(),
        _ => format!("{}-unknown-{}", arch, os),
    }
}
