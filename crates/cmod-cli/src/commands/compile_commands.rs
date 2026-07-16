use std::path::PathBuf;

use cmod_build::compiler::{make_backend, BackendConfig};
use cmod_build::graph::{ModuleGraph, ModuleNode};
use cmod_build::plan::BuildPlan;
use cmod_build::runner;
use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::shell::Shell;

/// Run `cmod compile-commands` — generate a compile_commands.json without building.
pub fn run(shell: &Shell, target_override: Option<String>) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let mut config = Config::load(&cwd)?;

    if let Some(t) = target_override {
        config.target = Some(t);
    }

    let src_dirs = config.src_dirs();
    let exclude = config.exclude_patterns();
    let sources = runner::discover_sources_multi(&src_dirs, &exclude)?;

    if sources.is_empty() {
        let dirs: Vec<_> = src_dirs.iter().map(|d| d.display().to_string()).collect();
        shell.warn(format!("no source files found in {}", dirs.join(", ")));
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

    let (mut backend_cfg, compiler_kind, target) = setup_compiler(&config);

    // Add dependency artifacts if lockfile exists (without building)
    if let Ok(lockfile) = cmod_core::lockfile::Lockfile::load(&config.lockfile_path) {
        let dep_artifacts = super::common::collect_dep_artifacts(&config, &lockfile);

        // Add dep PCMs as -fmodule-file= flags
        for (mod_name, pcm_path) in &dep_artifacts.pcms {
            backend_cfg.extra_flags.push(format!(
                "-fmodule-file={}={}",
                mod_name,
                pcm_path.display()
            ));
        }

        // Add dep include directories
        for inc_dir in &dep_artifacts.include_dirs {
            backend_cfg
                .extra_flags
                .push(format!("-I{}", inc_dir.display()));
        }
    }
    let backend = make_backend(compiler_kind, &backend_cfg)?;

    let plan = BuildPlan::from_graph(
        &graph,
        &build_dir,
        &target,
        config.profile,
        build_type,
        Some(&config.manifest.package.name),
        backend.bmi_extension(),
    )?;

    let commands = plan.compile_commands(backend.as_ref(), &config.root);
    let json = serde_json::to_string_pretty(&commands).map_err(|e| CmodError::BuildFailed {
        reason: format!("failed to serialize compile_commands.json: {}", e),
    })?;

    let output_path = config.root.join("compile_commands.json");
    std::fs::write(&output_path, &json)?;

    shell.status(
        "Generated",
        format!("{} with {} entries", output_path.display(), commands.len()),
    );

    for cmd in &commands {
        shell.verbose("Entry", &cmd.file);
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

    // Filter imports to only include modules that exist in the graph
    let known_modules = graph.module_names();
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

/// Assemble the compiler configuration from the manifest (same logic as build.rs).
fn setup_compiler(config: &Config) -> (BackendConfig, cmod_core::types::Compiler, String) {
    let cxx_standard = config
        .manifest
        .toolchain
        .as_ref()
        .and_then(|tc| tc.cxx_standard.clone())
        .unwrap_or_else(|| "20".to_string());

    let compiler_kind = config
        .manifest
        .toolchain
        .as_ref()
        .and_then(|tc| tc.compiler.clone())
        .unwrap_or(cmod_core::types::Compiler::Clang);

    let mut backend_cfg = BackendConfig {
        cxx_standard,
        profile: config.profile,
        ..Default::default()
    };

    if let Some(ref tc) = config.manifest.toolchain {
        backend_cfg.stdlib = tc.stdlib.clone();
        backend_cfg.sysroot = tc.sysroot.clone();
    }

    // Add include directories from [build] section
    if let Some(ref build) = config.manifest.build {
        let root = &config.root;
        for dir in &build.include_dirs {
            let abs = root.join(dir);
            backend_cfg.extra_flags.push(format!("-I{}", abs.display()));
        }
        backend_cfg.extra_flags.extend(build.extra_flags.clone());
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

    backend_cfg.target = Some(target.clone());

    (backend_cfg, compiler_kind, target)
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
