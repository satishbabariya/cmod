use std::collections::HashMap;
use std::path::PathBuf;

use cmod_build::compiler::{make_backend, BackendConfig};
use cmod_build::plan::{BuildPlan, CompileCommand};
use cmod_build::runner;
use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::shell::Shell;
use cmod_core::types::Compiler;

use cmod_workspace::WorkspaceManager;

use super::build::{
    build_module_graph, member_build, member_include_dirs_of, setup_compiler, ClangScan,
};

/// Run `cmod compile-commands` — generate a compile_commands.json without building.
pub fn run(shell: &Shell, target_override: Option<String>) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let mut config = Config::load(&cwd)?;

    if let Some(t) = target_override {
        config.target = Some(t);
    }

    let commands = if config.manifest.is_workspace() {
        workspace_commands(&config)?
    } else {
        match package_commands(&config, shell)? {
            Some(commands) => commands,
            None => return Ok(()),
        }
    };

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

/// The entries of a single package, or `None` (with a warning) when it has
/// no sources.
fn package_commands(
    config: &Config,
    shell: &Shell,
) -> Result<Option<Vec<CompileCommand>>, CmodError> {
    let src_dirs = config.src_dirs();
    let exclude = config.exclude_patterns();
    let sources = runner::discover_sources_multi(&src_dirs, &exclude)?;

    if sources.is_empty() {
        let dirs: Vec<_> = src_dirs.iter().map(|d| d.display().to_string()).collect();
        shell.warn(format!("no source files found in {}", dirs.join(", ")));
        return Ok(None);
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
    let (mut backend_cfg, compiler_kind, target) = setup_compiler(config, &[]);
    let bmi_ext = make_backend(compiler_kind.clone(), &backend_cfg)?.bmi_extension();

    // Dependencies as the build sees them (without building them): path
    // dependencies, and git dependencies when there is a lockfile.
    let mut dep_artifacts = super::common::collect_path_dep_artifacts(config, bmi_ext);
    if let Ok(lockfile) = cmod_core::lockfile::Lockfile::load(&config.lockfile_path) {
        dep_artifacts.merge(&super::common::collect_dep_artifacts(
            config, &lockfile, bmi_ext,
        ));
    }
    add_module_files(&mut backend_cfg, &compiler_kind, &dep_artifacts.pcms);
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

    Ok(Some(plan.compile_commands(backend.as_ref(), &config.root)))
}

/// The entries of every workspace member, each described as `cmod build`
/// compiles it (see [`member_build`]): its flags, the git dependencies'
/// BMIs and include directories, and those of the members it depends on.
fn workspace_commands(config: &Config) -> Result<Vec<CompileCommand>, CmodError> {
    let ws = WorkspaceManager::load(&config.root)?;
    let (root_cfg, root_compiler, _) = setup_compiler(config, &[]);
    let bmi_ext = make_backend(root_compiler, &root_cfg)?.bmi_extension();
    let git_deps = match cmod_core::lockfile::Lockfile::load(&config.lockfile_path) {
        Ok(lockfile) => super::common::collect_dep_artifacts(config, &lockfile, bmi_ext),
        Err(_) => Default::default(),
    };

    let mut commands = Vec::new();
    let mut member_pcms: HashMap<String, HashMap<String, PathBuf>> = HashMap::new();
    let mut member_include_dirs: HashMap<String, Vec<PathBuf>> = HashMap::new();
    for member in ws.build_order()? {
        let Some(mb) = member_build(
            config,
            &ws,
            member,
            &git_deps.include_dirs,
            &member_include_dirs,
        )?
        else {
            continue;
        };
        let mut pcms = git_deps.pcms.clone();
        for dep in &mb.transitive_deps {
            pcms.extend(member_pcms.get(dep).cloned().unwrap_or_default());
        }
        let mut backend_cfg = mb.backend_cfg;
        add_module_files(&mut backend_cfg, &mb.compiler_kind, &pcms);
        let backend = make_backend(mb.compiler_kind, &backend_cfg)?;

        let scan = ClangScan::for_backend(backend.as_ref(), &mb.build_dir, true);
        let graph = build_module_graph(&mb.sources, &member.name, scan.as_ref())?;
        graph.validate()?;
        let plan = BuildPlan::from_graph(
            &graph,
            &mb.build_dir,
            &mb.target,
            config.profile,
            mb.build_type,
            Some(&member.name),
            backend.bmi_extension(),
        )?;
        commands.extend(plan.compile_commands(backend.as_ref(), &config.root));

        // Where the member's BMIs go, from its plan: a database written
        // before the first build must name them too.
        member_pcms.insert(member.name.clone(), plan.pcm_paths().into_iter().collect());
        member_include_dirs.insert(member.name.clone(), member_include_dirs_of(member));
    }
    Ok(commands)
}

/// Dependency BMIs as `-fmodule-file=` flags, sorted. That flag and the
/// `.pcm` format are clang's; clangd cannot read a `.gcm` or `.ifc`.
fn add_module_files(
    backend_cfg: &mut BackendConfig,
    compiler: &Compiler,
    pcms: &HashMap<String, PathBuf>,
) {
    if *compiler != Compiler::Clang {
        return;
    }
    let mut pcms: Vec<_> = pcms.iter().collect();
    pcms.sort();
    for (mod_name, pcm_path) in pcms {
        backend_cfg
            .extra_flags
            .push(format!("-fmodule-file={}={}", mod_name, pcm_path.display()));
    }
}
