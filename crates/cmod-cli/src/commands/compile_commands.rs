use cmod_build::compiler::make_backend;
use cmod_build::plan::BuildPlan;
use cmod_build::runner;
use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::shell::Shell;
use cmod_core::types::Compiler;

use super::build::{build_module_graph, setup_compiler, ClangScan};

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

    let build_dir = config.build_dir();
    let build_type = config
        .manifest
        .build
        .as_ref()
        .and_then(|b| b.build_type)
        .unwrap_or_default();

    // The build's own flags: the database must describe the commands
    // `cmod build` runs, or clangd sees different headers and macros.
    let (mut backend_cfg, compiler_kind, target) = setup_compiler(&config, &[]);
    let bmi_ext = make_backend(compiler_kind.clone(), &backend_cfg)?.bmi_extension();

    // Dependencies as the build sees them (without building them): path
    // dependencies, and git dependencies when there is a lockfile.
    let mut dep_artifacts = super::common::collect_path_dep_artifacts(&config, bmi_ext);
    if let Ok(lockfile) = cmod_core::lockfile::Lockfile::load(&config.lockfile_path) {
        dep_artifacts.merge(&super::common::collect_dep_artifacts(
            &config, &lockfile, bmi_ext,
        ));
    }
    // Dependency BMIs as -fmodule-file= flags. That flag and the `.pcm`
    // format are clang's; clangd cannot read a `.gcm` or `.ifc`.
    if compiler_kind == Compiler::Clang {
        let mut pcms: Vec<_> = dep_artifacts.pcms.iter().collect();
        pcms.sort();
        for (mod_name, pcm_path) in pcms {
            backend_cfg.extra_flags.push(format!(
                "-fmodule-file={}={}",
                mod_name,
                pcm_path.display()
            ));
        }
    }
    for inc_dir in &dep_artifacts.include_dirs {
        backend_cfg
            .extra_flags
            .push(format!("-I{}", inc_dir.display()));
    }
    let backend = make_backend(compiler_kind, &backend_cfg)?;

    let scan = ClangScan::for_backend(backend.as_ref(), &build_dir, true);
    let graph = build_module_graph(&sources, &config.manifest.package.name, scan.as_ref())?;
    graph.validate()?;

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
