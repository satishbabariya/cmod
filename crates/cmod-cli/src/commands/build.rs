use std::sync::Arc;

use cmod_build::compiler::BackendConfig;
use cmod_build::graph::{ModuleGraph, ModuleNode};
use cmod_build::runner::{self, BuildRunner, BuildStats};
use cmod_cache::{ArtifactCache, RemoteCacheMode};
use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::lockfile::Lockfile;
use cmod_core::shell::{Shell, Verbosity};
use cmod_core::types::{Compiler, Profile};
use cmod_resolver::Resolver;
use cmod_workspace::WorkspaceManager;

/// Run `cmod build` — build the current module or workspace.
#[allow(clippy::too_many_arguments)]
pub fn run(
    release: bool,
    locked: bool,
    offline: bool,
    shell: &Shell,
    target_override: Option<String>,
    jobs: usize,
    force: bool,
    remote_cache_url: Option<String>,
    no_hooks: bool,
    verify: bool,
    timings: bool,
    features: &[String],
    no_default_features: bool,
    no_cache: bool,
    distributed: bool,
    workers: Vec<String>,
) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let mut config = Config::load(&cwd)?;

    config.profile = if release {
        Profile::Release
    } else {
        Profile::Debug
    };
    config.locked = locked;
    config.offline = offline;
    if let Some(t) = target_override {
        config.target = Some(t);
    }

    // Resolve remote cache URL: CLI flag > manifest [cache].shared_url
    let effective_remote_url = remote_cache_url.or_else(|| {
        config
            .manifest
            .cache
            .as_ref()
            .and_then(|c| c.shared_url.clone())
    });

    let profile_name = match config.profile {
        Profile::Debug => "debug",
        Profile::Release => "release",
    };

    // Check if this is a workspace build
    if config.manifest.is_workspace() {
        return build_workspace(
            &config,
            shell,
            jobs,
            force,
            &effective_remote_url,
            timings,
            no_cache,
        );
    }

    shell.status(
        "Building",
        format!("{} ({})", config.manifest.package.name, profile_name),
    );

    // Step 1: Ensure dependencies are resolved (with target-specific filtering)
    let lockfile = ensure_resolved(&config, shell)?;

    // Step 1.5: Verify lockfile integrity if --verify is set
    if verify {
        shell.status("Verifying", "lockfile integrity...");
        lockfile.verify_integrity()?;

        // Verify all package hashes are present
        for pkg in &lockfile.packages {
            if pkg.source.as_deref() == Some("git") && pkg.hash.is_none() {
                return Err(CmodError::SecurityViolation {
                    reason: format!(
                        "package '{}' has no content hash in lockfile; re-run `cmod resolve` to compute hashes",
                        pkg.name
                    ),
                });
            }
        }

        shell.verbose(
            "Verified",
            format!("lockfile integrity ({} packages)", lockfile.packages.len()),
        );
    }

    // Step 1.7: Enforce signature policy from [security]
    enforce_signature_policy(&config, &lockfile, shell)?;

    // Step 2: Run pre-build hook
    if !no_hooks {
        run_hook(
            &config,
            "pre-build",
            config
                .manifest
                .hooks
                .as_ref()
                .and_then(|h| h.pre_build.as_deref()),
            shell,
        )?;
    }

    // Resolve activated features for compiler defines
    let activated_features =
        resolve_build_features(&config.manifest, features, no_default_features);

    // Step 3: Build the single module
    let result = build_module(
        &config,
        shell,
        jobs,
        force,
        &effective_remote_url,
        timings,
        &activated_features,
        no_cache,
        distributed,
        &workers,
        Some(&lockfile),
    );

    // Step 4: Run post-build hook (only on success)
    if result.is_ok() && !no_hooks {
        run_hook(
            &config,
            "post-build",
            config
                .manifest
                .hooks
                .as_ref()
                .and_then(|h| h.post_build.as_deref()),
            shell,
        )?;
    }

    result
}

/// Create a remote cache instance from a URL, if provided, honoring the
/// manifest's `[cache]` auth/timeout/retry settings.
fn make_remote_cache(
    config: &Config,
    url: &Option<String>,
    shell: &Shell,
) -> Option<Box<dyn cmod_cache::RemoteCache>> {
    let url = url.as_ref()?;
    shell.verbose("Remote cache", url);
    Some(Box::new(super::common::remote_cache_client(
        config,
        url,
        RemoteCacheMode::ReadWrite,
    )))
}

/// Build a single module project.
#[allow(clippy::too_many_arguments)]
fn build_module(
    config: &Config,
    shell: &Shell,
    jobs: usize,
    force: bool,
    remote_url: &Option<String>,
    timings: bool,
    activated_features: &[String],
    no_cache: bool,
    distributed: bool,
    workers: &[String],
    lockfile: Option<&Lockfile>,
) -> Result<(), CmodError> {
    // Build path dependencies first and collect their artifacts
    let mut dep_artifacts =
        build_path_dependencies(config, shell, jobs, force, remote_url, no_cache)?;

    // Build vendored/resolved git dependencies and collect their artifacts
    if let Some(lockfile) = lockfile {
        let ven_artifacts = build_vendored_dependencies(
            config, lockfile, shell, jobs, force, remote_url, no_cache,
        )?;
        dep_artifacts.merge(&ven_artifacts);
    }

    // Discover source files
    let src_dirs = config.src_dirs();
    let exclude = config.exclude_patterns();
    let sources = runner::discover_sources_multi(&src_dirs, &exclude)?;
    let sources = runner::filter_included_sources(&sources);

    if sources.is_empty() {
        let dirs: Vec<_> = src_dirs.iter().map(|d| d.display().to_string()).collect();
        return Err(CmodError::BuildFailed {
            reason: format!("no source files found in {}", dirs.join(", ")),
        });
    }

    shell.verbose("Found", format!("{} source files", sources.len()));
    for s in &sources {
        shell.verbose("Source", format!("{}", s.display()));
    }

    // Build the module graph
    let graph = build_module_graph(&sources, &config.manifest.package.name)?;

    // Validate the module graph (imports, cycles, duplicates)
    graph.validate()?;

    if shell.verbosity() == Verbosity::Verbose {
        let order = graph.topological_order()?;
        shell.verbose("Build order", order.join(" -> "));
    }

    // Set up the compiler backend (with feature flags as -D defines)
    let (mut backend_cfg, compiler_kind, target) = setup_compiler(config, activated_features);

    // Add dependency include directories as -I flags
    for inc_dir in &dep_artifacts.include_dirs {
        backend_cfg
            .extra_flags
            .push(format!("-I{}", inc_dir.display()));
    }
    let backend = cmod_build::compiler::make_backend(compiler_kind, &backend_cfg)?;

    // Set up cache
    let cache = ArtifactCache::new(config.cache_dir());

    // Execute the build
    let build_dir = config.build_dir();

    let build_type = config
        .manifest
        .build
        .as_ref()
        .and_then(|b| b.build_type)
        .unwrap_or_default();

    let mut runner = BuildRunner::new(backend, Some(cache))
        .with_jobs(jobs)
        .with_force(force)
        .with_no_cache(no_cache)
        .with_extra_pcm_paths(dep_artifacts.pcms)
        .with_extra_obj_paths(dep_artifacts.objs)
        .with_shell(Arc::new(Shell::new(shell.verbosity())));

    if let Some(remote) = make_remote_cache(config, remote_url, shell) {
        runner = runner.with_remote_cache(remote);
    }

    // Set up distributed build if requested
    if distributed || !workers.is_empty() {
        let worker_endpoints = if workers.is_empty() {
            // Try to read workers from manifest [build.distributed] if available
            Vec::new()
        } else {
            workers.to_vec()
        };

        if !worker_endpoints.is_empty() {
            // Resolve auth token from environment variable if configured in manifest.
            let auth_token = config
                .manifest
                .build
                .as_ref()
                .and_then(|b| b.distributed.as_ref())
                .and_then(|d| d.auth_token_env.as_deref())
                .and_then(|env_name| std::env::var(env_name).ok());

            let dist_config = cmod_build::distributed::DistributedConfig {
                enabled: true,
                workers: worker_endpoints,
                auth_token,
                ..Default::default()
            };
            let pool = cmod_build::distributed::WorkerPool::new(&dist_config);
            match pool.discover_workers() {
                Ok(count) => {
                    shell.status("Workers", format!("{} remote worker(s) available", count));
                    runner = runner.with_worker_pool(pool);
                }
                Err(e) => {
                    shell.warn(format!("distributed build setup failed: {}", e));
                    shell.note("falling back to local build");
                }
            }
        } else {
            shell.warn("--distributed specified but no worker endpoints provided");
            shell.note("use --workers=http://host:port to specify workers");
        }
    }

    if jobs != 1 {
        shell.verbose("Parallelism", format!("{} jobs", runner.effective_jobs()));
    }

    let (output, stats) = runner.build_with_stats(
        &graph,
        &build_dir,
        &target,
        config.profile,
        build_type,
        Some(&config.manifest.package.name),
    )?;

    print_build_stats(&stats, shell, timings);
    shell.status("Finished", format!("{}", output.display()));
    Ok(())
}

/// Build path dependencies and collect their PCMs, object files, and include dirs.
///
/// For each dependency with `path = "..."`, load its config, build it,
/// and return the aggregated artifacts for the parent project.
fn build_path_dependencies(
    config: &Config,
    shell: &Shell,
    jobs: usize,
    force: bool,
    remote_url: &Option<String>,
    no_cache: bool,
) -> Result<super::common::DepArtifacts, CmodError> {
    let mut artifacts = super::common::DepArtifacts::default();

    for (dep_name, dep) in &config.manifest.dependencies {
        let dep_path = match dep.path() {
            Some(p) => config.root.join(p),
            None => continue,
        };

        if !dep_path.join("cmod.toml").exists() {
            continue;
        }

        shell.verbose(
            "Building",
            format!("path dependency: {} ({})", dep_name, dep_path.display()),
        );

        // Load the dependency's config and propagate the active profile so
        // release builds of the parent produce release path-dep artifacts.
        let mut dep_config = Config::load(&dep_path)?;
        dep_config.profile = config.profile;
        dep_config.locked = config.locked;
        dep_config.offline = config.offline;
        if let Some(t) = &config.target {
            dep_config.target = Some(t.clone());
        }

        // Collect include directories from the dependency
        let inc_dirs = super::common::detect_include_dirs(&dep_path, &dep_config);
        artifacts.include_dirs.extend(inc_dirs);

        // Load the path dep's own lockfile so its git dependencies get built
        let dep_lockfile = if dep_config.lockfile_path.exists() {
            Lockfile::load(&dep_config.lockfile_path).ok()
        } else {
            None
        };

        // Check if the dependency has compilable sources; header-only deps
        // provide only include dirs and should not be passed to build_module().
        let dep_sources =
            runner::discover_sources_multi(&dep_config.src_dirs(), &dep_config.exclude_patterns())
                .unwrap_or_default();

        if !dep_sources.is_empty() {
            // Recursively build the dependency (handles nested path deps)
            build_module(
                &dep_config,
                shell,
                jobs,
                force,
                remote_url,
                false,
                &[],
                no_cache,
                false,
                &[],
                dep_lockfile.as_ref(),
            )?;
        } else {
            shell.verbose(
                "Skipping",
                format!("header-only path dep: {} (no sources)", dep_name),
            );
        }

        // Collect PCM files
        let dep_build_dir = dep_config.build_dir();
        let pcm_dir = dep_build_dir.join("pcm");
        if pcm_dir.exists() {
            let dep_sources = runner::discover_sources_multi(
                &dep_config.src_dirs(),
                &dep_config.exclude_patterns(),
            )?;
            for source in &dep_sources {
                if let Ok(Some(mod_name)) = runner::extract_module_name(source) {
                    let sanitized = mod_name.replace(['.', ':', '/'], "_");
                    let pcm_path = pcm_dir.join(format!("{}.pcm", sanitized));
                    if pcm_path.exists() {
                        artifacts.pcms.insert(mod_name, pcm_path);
                    }
                }
            }
        }

        // Collect linkable artifacts: prefer .a archives over individual .o files
        // to avoid duplicate symbols from stale path-encoded objects.
        let mut has_archive = false;
        if dep_build_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&dep_build_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("a") {
                        artifacts.objs.push(path);
                        has_archive = true;
                    }
                }
            }
        }

        // Only collect individual .o files if no archive was produced
        if !has_archive {
            let obj_dir = dep_build_dir.join("obj");
            if obj_dir.exists() {
                if let Ok(entries) = std::fs::read_dir(&obj_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().and_then(|e| e.to_str()) == Some("o") {
                            artifacts.objs.push(path);
                        }
                    }
                }
            }
        }
    }

    if !artifacts.pcms.is_empty() || !artifacts.objs.is_empty() {
        shell.verbose(
            "Path deps",
            format!(
                "{} PCMs, {} objects/libs, {} include dirs",
                artifacts.pcms.len(),
                artifacts.objs.len(),
                artifacts.include_dirs.len()
            ),
        );
    }

    Ok(artifacts)
}

/// Build vendored/resolved git dependencies and collect their artifacts.
///
/// Iterates over the lockfile packages in topological order (so transitive deps
/// are built first), locates each git dependency on disk (in `vendor/` or
/// `build/deps/`), builds it, and accumulates the resulting PCM, object files,
/// and include directories.
#[allow(clippy::too_many_arguments)]
fn build_vendored_dependencies(
    config: &Config,
    lockfile: &Lockfile,
    shell: &Shell,
    jobs: usize,
    force: bool,
    remote_url: &Option<String>,
    no_cache: bool,
) -> Result<super::common::DepArtifacts, CmodError> {
    let mut artifacts = super::common::DepArtifacts::default();

    let vendor_dir = config.root.join("vendor");
    let deps_dir = config.deps_dir();

    // Build packages in topological order so deps are ready before dependents
    let ordered = super::common::topo_sort_packages(&lockfile.packages);

    for pkg in &ordered {
        // Only handle git-sourced dependencies
        if pkg.source.as_deref() != Some("git") {
            continue;
        }

        // Find (or fetch) the dependency on disk
        let dep_dir = match super::common::ensure_dep_on_disk(pkg, &vendor_dir, &deps_dir, shell) {
            Ok(Some(d)) => d,
            Ok(None) => continue,
            Err(e) => return Err(e),
        };

        shell.verbose(
            "Building",
            format!("dependency: {} ({})", pkg.name, dep_dir.display()),
        );

        let mut dep_config = Config::load(&dep_dir)?;
        dep_config.profile = config.profile;
        dep_config.target = config.target.clone();

        // Auto-detect include directories for this dependency
        let inc_dirs = super::common::detect_include_dirs(&dep_dir, &dep_config);
        artifacts.include_dirs.extend(inc_dirs.clone());

        // Discover source files in the dependency
        let src_dirs = dep_config.src_dirs();
        let exclude = dep_config.exclude_patterns();
        let sources = runner::discover_sources_multi(&src_dirs, &exclude)?;

        // Filter out source files that are #include'd by module interface files
        // to avoid duplicate compilation and link-time symbol conflicts.
        let sources = runner::filter_included_sources(&sources);

        if sources.is_empty() {
            shell.warn(format!(
                "no source files for dependency {}, skipping",
                pkg.name
            ));
            continue;
        }

        shell.verbose(
            "Found",
            format!("{} sources in {}", sources.len(), pkg.name),
        );

        // Build the module graph for this dependency
        let graph = build_module_graph(&sources, &dep_config.manifest.package.name)?;
        graph.validate()?;

        // Set up compiler from the dependency's own toolchain config
        let (mut backend_cfg, compiler_kind, target) = setup_compiler(&dep_config, &[]);

        // Add auto-detected include dirs to the dep's own compiler flags
        for inc_dir in &inc_dirs {
            backend_cfg
                .extra_flags
                .push(format!("-I{}", inc_dir.display()));
        }

        // Also add include dirs from already-processed deps (transitive)
        for inc_dir in &artifacts.include_dirs {
            if !inc_dirs.contains(inc_dir) {
                backend_cfg
                    .extra_flags
                    .push(format!("-I{}", inc_dir.display()));
            }
        }
        let backend = cmod_build::compiler::make_backend(compiler_kind, &backend_cfg)?;

        // Set up cache
        let cache = ArtifactCache::new(dep_config.cache_dir());

        let build_dir = dep_config.build_dir();
        let build_type = dep_config
            .manifest
            .build
            .as_ref()
            .and_then(|b| b.build_type)
            .unwrap_or_default();

        // Clean stale build artifacts from the dep's output dirs.
        // Object file names encode the full source path, so builds from a
        // different project location leave behind stale .o files that cause
        // duplicate-symbol errors at link time.
        let _ = std::fs::remove_dir_all(build_dir.join("obj"));
        let _ = std::fs::remove_dir_all(build_dir.join("pcm"));

        // Build with accumulated PCMs from already-built dependencies.
        // Only pass .o files (not .a archives) as extra objects for intermediate
        // dep builds — static lib archives should not be nested inside each other.
        let extra_objs: Vec<_> = artifacts
            .objs
            .iter()
            .filter(|p| p.extension().and_then(|e| e.to_str()) != Some("a"))
            .cloned()
            .collect();
        let mut runner = BuildRunner::new(backend, Some(cache))
            .with_jobs(jobs)
            .with_force(force)
            .with_no_cache(no_cache)
            .with_extra_pcm_paths(artifacts.pcms.clone())
            .with_extra_obj_paths(extra_objs)
            .with_shell(Arc::new(Shell::new(shell.verbosity())));

        if let Some(remote) = make_remote_cache(config, remote_url, shell) {
            runner = runner.with_remote_cache(remote);
        }

        runner.build_with_stats(
            &graph,
            &build_dir,
            &target,
            dep_config.profile,
            build_type,
            Some(&dep_config.manifest.package.name),
        )?;

        // Collect PCM files from the built dependency
        let pcm_dir = build_dir.join("pcm");
        if pcm_dir.exists() {
            for source in &sources {
                if let Ok(Some(mod_name)) = runner::extract_module_name(source) {
                    let sanitized = mod_name.replace(['.', ':', '/'], "_");
                    let pcm_path = pcm_dir.join(format!("{}.pcm", sanitized));
                    if pcm_path.exists() {
                        artifacts.pcms.insert(mod_name, pcm_path);
                    }
                }
            }
        }

        // Collect linkable artifacts: prefer .a archives over individual .o files
        let mut has_archive = false;
        if build_dir.exists() {
            if let Ok(entries) = std::fs::read_dir(&build_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.extension().and_then(|e| e.to_str()) == Some("a") {
                        artifacts.objs.push(path);
                        has_archive = true;
                    }
                }
            }
        }

        if !has_archive {
            let obj_dir = build_dir.join("obj");
            if obj_dir.exists() {
                if let Ok(entries) = std::fs::read_dir(&obj_dir) {
                    for entry in entries.flatten() {
                        let path = entry.path();
                        if path.extension().and_then(|e| e.to_str()) == Some("o") {
                            artifacts.objs.push(path);
                        }
                    }
                }
            }
        }

        shell.verbose(
            "Built",
            format!(
                "{} ({} PCMs, {} objects/libs total)",
                pkg.name,
                artifacts.pcms.len(),
                artifacts.objs.len()
            ),
        );
    }

    if !artifacts.pcms.is_empty() || !artifacts.objs.is_empty() {
        shell.verbose(
            "Git deps",
            format!(
                "{} PCMs, {} objects/libs, {} include dirs",
                artifacts.pcms.len(),
                artifacts.objs.len(),
                artifacts.include_dirs.len()
            ),
        );
    }

    Ok(artifacts)
}

/// Build all members of a workspace.
fn build_workspace(
    config: &Config,
    shell: &Shell,
    jobs: usize,
    force: bool,
    remote_url: &Option<String>,
    timings: bool,
    no_cache: bool,
) -> Result<(), CmodError> {
    let ws = WorkspaceManager::load(&config.root)?;

    shell.status(
        "Building",
        format!(
            "workspace ({} members, {})",
            ws.members.len(),
            match config.profile {
                Profile::Debug => "debug",
                Profile::Release => "release",
            }
        ),
    );

    // Ensure dependencies are resolved
    let lockfile = ensure_resolved(config, shell)?;

    // Build external git dependencies first (shared across all workspace members)
    let git_dep_artifacts = if !lockfile.packages.is_empty() {
        build_vendored_dependencies(config, &lockfile, shell, jobs, force, remote_url, no_cache)?
    } else {
        super::common::DepArtifacts::default()
    };

    // Build members in topological order so dependencies are built first
    let ordered_members = ws.build_order()?;

    // Per-member PCM and object paths, keyed by member name.
    // This allows each member to receive only the artifacts from its
    // transitive dependency chain, not from unrelated members.
    let mut member_pcm_paths: std::collections::HashMap<
        String,
        std::collections::HashMap<String, std::path::PathBuf>,
    > = std::collections::HashMap::new();
    let mut member_obj_paths: std::collections::HashMap<String, Vec<std::path::PathBuf>> =
        std::collections::HashMap::new();
    let mut member_include_dirs: std::collections::HashMap<String, Vec<std::path::PathBuf>> =
        std::collections::HashMap::new();
    let mut failed = Vec::new();

    for member in &ordered_members {
        shell.status("Compiling", &member.name);

        let member_src_dirs: Vec<std::path::PathBuf> = {
            let srcs = member
                .manifest
                .build
                .as_ref()
                .map(|b| &b.sources)
                .filter(|s| !s.is_empty());
            match srcs {
                Some(dirs) => dirs.iter().map(|s| member.path.join(s)).collect(),
                None => vec![member.path.join("src")],
            }
        };
        let member_exclude: Vec<String> = member
            .manifest
            .build
            .as_ref()
            .map(|b| b.exclude.clone())
            .unwrap_or_default();
        let sources = runner::discover_sources_multi(&member_src_dirs, &member_exclude)?;
        let sources = runner::filter_included_sources(&sources);

        if sources.is_empty() {
            shell.verbose("Skipping", format!("{} (no source files)", member.name));
            continue;
        }

        let graph = build_module_graph(&sources, &member.name)?;
        graph.validate()?;
        let (mut backend_cfg, compiler_kind, target) = setup_compiler(config, &[]);

        // Add member-specific include dirs and extra flags from [build] section
        if let Some(ref build) = member.manifest.build {
            for dir in &build.include_dirs {
                let abs = member.path.join(dir);
                backend_cfg.extra_flags.push(format!("-I{}", abs.display()));
            }
            backend_cfg.extra_flags.extend(build.extra_flags.clone());
        }

        // Auto-detect include/ directory for this member
        let member_include = member.path.join("include");
        if member_include.is_dir() {
            let flag = format!("-I{}", member_include.display());
            if !backend_cfg.extra_flags.contains(&flag) {
                backend_cfg.extra_flags.push(flag);
            }
        }

        // Add git dependency include dirs to the compiler
        for inc_dir in &git_dep_artifacts.include_dirs {
            backend_cfg
                .extra_flags
                .push(format!("-I{}", inc_dir.display()));
        }

        let cache = ArtifactCache::new(config.cache_dir());

        let build_dir = config.build_dir().join(&member.name);

        let build_type = member
            .manifest
            .build
            .as_ref()
            .and_then(|b| b.build_type)
            .unwrap_or_default();

        // Start with git dep artifacts, then layer workspace member deps on top
        let transitive_deps = ws.transitive_member_deps(&member.name);
        let mut extra_pcms: std::collections::HashMap<String, std::path::PathBuf> =
            git_dep_artifacts.pcms.clone();
        let mut extra_objs: Vec<std::path::PathBuf> = git_dep_artifacts.objs.clone();

        for dep_name in &transitive_deps {
            if let Some(dep_pcms) = member_pcm_paths.get(dep_name) {
                extra_pcms.extend(dep_pcms.clone());
            }
            if let Some(dep_objs) = member_obj_paths.get(dep_name) {
                extra_objs.extend(dep_objs.clone());
            }
            // Add include dirs from upstream members
            if let Some(dep_incs) = member_include_dirs.get(dep_name) {
                for inc_dir in dep_incs {
                    let flag = format!("-I{}", inc_dir.display());
                    if !backend_cfg.extra_flags.contains(&flag) {
                        backend_cfg.extra_flags.push(flag);
                    }
                }
            }
        }

        if !transitive_deps.is_empty() {
            let dep_list: Vec<&String> = transitive_deps.iter().collect();
            shell.verbose(
                "Upstream",
                format!(
                    "{} ({} PCMs, {} objects)",
                    dep_list
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>()
                        .join(", "),
                    extra_pcms.len(),
                    extra_objs.len(),
                ),
            );
        }

        let backend = cmod_build::compiler::make_backend(compiler_kind, &backend_cfg)?;
        let mut runner_instance = BuildRunner::new(backend, Some(cache))
            .with_jobs(jobs)
            .with_force(force)
            .with_no_cache(no_cache)
            .with_extra_pcm_paths(extra_pcms)
            .with_extra_obj_paths(extra_objs)
            .with_shell(Arc::new(Shell::new(shell.verbosity())));
        if let Some(remote) = make_remote_cache(config, remote_url, shell) {
            runner_instance = runner_instance.with_remote_cache(remote);
        }
        match runner_instance.build_with_stats(
            &graph,
            &build_dir,
            &target,
            config.profile,
            build_type,
            Some(&member.name),
        ) {
            Ok((output, stats)) => {
                print_build_stats(&stats, shell, timings);
                shell.verbose("Built", format!("{}", output.display()));

                // Collect PCM files from this member for downstream members
                let mut this_pcms: std::collections::HashMap<String, std::path::PathBuf> =
                    std::collections::HashMap::new();
                let pcm_dir = build_dir.join("pcm");
                if pcm_dir.exists() {
                    for source in &sources {
                        if let Ok(Some(mod_name)) = runner::extract_module_name(source) {
                            let sanitized = mod_name.replace(['.', ':', '/'], "_");
                            let pcm_path = pcm_dir.join(format!("{}.pcm", sanitized));
                            if pcm_path.exists() {
                                this_pcms.insert(mod_name, pcm_path);
                            }
                        }
                    }
                }
                member_pcm_paths.insert(member.name.clone(), this_pcms);

                // Collect object files from this member for downstream linking
                let mut this_objs: Vec<std::path::PathBuf> = Vec::new();
                let obj_dir = build_dir.join("obj");
                if obj_dir.exists() {
                    if let Ok(entries) = std::fs::read_dir(&obj_dir) {
                        for entry in entries.flatten() {
                            let path = entry.path();
                            if path.extension().and_then(|e| e.to_str()) == Some("o") {
                                this_objs.push(path);
                            }
                        }
                    }
                }
                member_obj_paths.insert(member.name.clone(), this_objs);

                // Store include dirs from this member for downstream members
                let mut this_inc_dirs = Vec::new();
                let inc = member.path.join("include");
                if inc.is_dir() {
                    this_inc_dirs.push(inc);
                }
                if let Some(ref build) = member.manifest.build {
                    for dir in &build.include_dirs {
                        let abs = member.path.join(dir);
                        if abs.is_dir() && !this_inc_dirs.contains(&abs) {
                            this_inc_dirs.push(abs);
                        }
                    }
                }
                member_include_dirs.insert(member.name.clone(), this_inc_dirs);
            }
            Err(e) => {
                shell.error(format!("{}: {}", member.name, e));
                failed.push(member.name.clone());
            }
        }
    }

    if !failed.is_empty() {
        return Err(CmodError::BuildFailed {
            reason: format!("workspace build failed for members: {}", failed.join(", ")),
        });
    }

    shell.status("Finished", "workspace build complete");
    Ok(())
}

/// Build a ModuleGraph from discovered source files.
///
/// Attempts to use `clang-scan-deps` for accurate module dependency scanning.
/// Falls back to regex-based import extraction if `clang-scan-deps` is unavailable.
fn build_module_graph(
    sources: &[std::path::PathBuf],
    package_name: &str,
) -> Result<ModuleGraph, CmodError> {
    let mut graph = ModuleGraph::new();

    // Try clang-scan-deps first for more accurate results
    let use_scanner = is_clang_scan_deps_available();

    for source in sources {
        let kind = runner::classify_source(source)?;
        let module_name = runner::extract_module_name(source)?.unwrap_or_else(|| {
            source
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string()
        });

        let imports = if use_scanner {
            scan_deps_imports(source).unwrap_or_else(|_| {
                // Fall back to regex on scan failure
                extract_imports_from_source(source).unwrap_or_default()
            })
        } else {
            extract_imports_from_source(source)?
        };

        // Extract partition ownership for partition units
        let partition_of = runner::extract_partition_owner(source)?;

        // Resolve relative partition imports (`:foo`) to fully-qualified names
        // (e.g., `module_name:foo`). This is needed because `export import :vec2;`
        // inside module `local.geometry` should resolve to `local.geometry:vec2`.
        let imports = imports
            .into_iter()
            .map(|imp| {
                if let Some(part) = imp.strip_prefix(':') {
                    format!("{}:{}", module_name, part)
                } else {
                    imp
                }
            })
            .collect();

        // Use source path as unique node ID to support multi-TU modules
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

    // Filter imports to only include modules that exist in the graph.
    // Use logical module names (not node IDs) for the filter.
    let known_modules = graph.module_names();
    for node in graph.nodes.values_mut() {
        node.imports.retain(|imp| known_modules.contains(imp));
    }

    Ok(graph)
}

/// Resolve the `clang-scan-deps` binary path, respecting the `SCAN_DEPS` env var.
fn scan_deps_binary() -> std::ffi::OsString {
    std::env::var_os("SCAN_DEPS").unwrap_or_else(|| std::ffi::OsString::from("clang-scan-deps"))
}

/// Check if `clang-scan-deps` is available (respects `SCAN_DEPS` env var).
fn is_clang_scan_deps_available() -> bool {
    std::process::Command::new(scan_deps_binary())
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Use `clang-scan-deps` to discover module dependencies via P1689 format.
/// Respects the `SCAN_DEPS` env var for the binary path.
fn scan_deps_imports(source: &std::path::Path) -> Result<Vec<String>, CmodError> {
    let output = std::process::Command::new(scan_deps_binary())
        .args(["--format=p1689", "--"])
        .arg(source)
        .arg("-std=c++20")
        .output()
        .map_err(|e| CmodError::ModuleScanFailed {
            reason: format!("failed to run clang-scan-deps: {}", e),
        })?;

    if !output.status.success() {
        return Err(CmodError::ModuleScanFailed {
            reason: format!(
                "clang-scan-deps failed for {}: {}",
                source.display(),
                String::from_utf8_lossy(&output.stderr)
            ),
        });
    }

    // Parse P1689 JSON output
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_p1689_imports(&stdout)
}

/// Parse P1689 JSON format to extract required module names.
fn parse_p1689_imports(json_str: &str) -> Result<Vec<String>, CmodError> {
    let value: serde_json::Value =
        serde_json::from_str(json_str).map_err(|e| CmodError::ModuleScanFailed {
            reason: format!("failed to parse P1689 output: {}", e),
        })?;

    let mut imports = Vec::new();

    // P1689 format: { "rules": [{ "requires": [{ "logical-name": "..." }] }] }
    if let Some(rules) = value.get("rules").and_then(|r| r.as_array()) {
        for rule in rules {
            if let Some(requires) = rule.get("requires").and_then(|r| r.as_array()) {
                for req in requires {
                    if let Some(name) = req.get("logical-name").and_then(|n| n.as_str()) {
                        imports.push(name.to_string());
                    }
                }
            }
        }
    }

    Ok(imports)
}

/// Set up the Clang compiler backend from config.
fn setup_compiler(
    config: &Config,
    activated_features: &[String],
) -> (BackendConfig, Compiler, String) {
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
        .unwrap_or(Compiler::Clang);

    let mut backend_cfg = BackendConfig {
        cxx_standard,
        profile: config.profile,
        ..Default::default()
    };

    if let Some(ref tc) = config.manifest.toolchain {
        backend_cfg.stdlib = tc.stdlib.clone();
        backend_cfg.sysroot = tc.sysroot.clone();
    }

    // Apply build section settings from manifest
    if let Some(ref build) = config.manifest.build {
        if build.lto == Some(true) {
            backend_cfg.lto = true;
        }
        if let Some(opt) = build.optimization {
            backend_cfg.optimization = Some(opt);
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

    backend_cfg.target = Some(target.clone());

    // Add feature flags as compiler defines
    for feature in activated_features {
        let flag = format!(
            "-DCMOD_FEATURE_{}=1",
            feature.to_uppercase().replace('-', "_")
        );
        backend_cfg.extra_flags.push(flag);
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

    // Auto-detect include/ directory (convention)
    let include_dir = config.root.join("include");
    if include_dir.is_dir() {
        let flag = format!("-I{}", include_dir.display());
        if !backend_cfg.extra_flags.contains(&flag) {
            backend_cfg.extra_flags.push(flag);
        }
    }

    (backend_cfg, compiler_kind, target)
}

/// Resolve which features are activated for the build.
///
/// Returns a list of feature names that should be passed as `-DCMOD_FEATURE_*` flags.
fn resolve_build_features(
    manifest: &cmod_core::manifest::Manifest,
    features: &[String],
    no_default_features: bool,
) -> Vec<String> {
    let mut activated = Vec::new();

    // Add default features unless disabled
    if !no_default_features {
        if let Some(defaults) = manifest.features.get("default") {
            for f in defaults {
                // Skip dep: prefixed entries (they activate deps, not flags)
                if !f.starts_with("dep:") && !activated.contains(f) {
                    activated.push(f.clone());
                }
            }
        }
    }

    // Add explicitly requested features
    for f in features {
        if !f.starts_with("dep:") && !activated.contains(f) {
            activated.push(f.clone());
        }
    }

    activated
}

/// Ensure dependencies are resolved; if lockfile exists, load it.
///
/// If a `vendor/` directory exists and the build is in offline mode (or
/// `vendor/config.toml` is present), the resolver uses vendored sources.
fn ensure_resolved(config: &Config, shell: &Shell) -> Result<Lockfile, CmodError> {
    // Check for vendored dependencies
    let vendor_dir = config.root.join("vendor");
    let vendor_config = vendor_dir.join("config.toml");
    let is_vendored = vendor_dir.exists() && vendor_config.exists();

    if is_vendored && config.offline {
        shell.status("Using", "vendored dependencies (offline mode)");
    }

    if config.lockfile_path.exists() {
        Lockfile::load(&config.lockfile_path)
    } else if config.manifest.dependencies.is_empty() && config.manifest.target.is_empty() {
        Ok(Lockfile::new())
    } else if config.locked {
        Err(CmodError::LockfileNotFound)
    } else {
        // Auto-resolve with target-specific dependency filtering
        shell.status("Resolving", "dependencies...");
        // Use vendor dir as deps dir if vendored deps exist
        let deps_dir = if is_vendored {
            vendor_dir
        } else {
            config.deps_dir()
        };
        let mut resolver = Resolver::new(deps_dir);
        let lockfile = resolver.resolve_with_target(
            &config.manifest,
            None,
            false,
            config.offline,
            &[],
            false,
            config.target.as_deref(),
        )?;
        lockfile.save(&config.lockfile_path)?;
        Ok(lockfile)
    }
}

/// Simple import extraction by scanning source content for `import` statements.
///
/// Handles:
/// - `import module_name;`
/// - `export import module_name;` (re-exports)
/// - `import :partition;` (partition imports, qualified with parent module)
fn extract_imports_from_source(path: &std::path::Path) -> Result<Vec<String>, CmodError> {
    let content = std::fs::read_to_string(path)?;
    let mut imports = Vec::new();

    // Determine the parent module name for qualifying partition imports.
    // If this file declares `export module foo.bar;` or `export module foo.bar:part;`,
    // the parent module is `foo.bar`.
    let parent_module = content.lines().find_map(|line| {
        let trimmed = line.trim();
        if trimmed.starts_with("export module") || trimmed.starts_with("module") {
            let decl = trimmed
                .trim_start_matches("export")
                .trim()
                .trim_start_matches("module")
                .trim()
                .trim_end_matches(';')
                .trim();
            // For `foo.bar:partition`, parent is `foo.bar`
            // For `foo.bar`, parent is `foo.bar`
            Some(decl.split(':').next().unwrap_or(decl).to_string())
        } else {
            None
        }
    });

    for line in content.lines() {
        let trimmed = line.trim();

        // Match both `import X;` and `export import X;`
        let import_part = if trimmed.starts_with("export import ") && trimmed.ends_with(';') {
            Some(
                trimmed
                    .trim_start_matches("export import ")
                    .trim_end_matches(';')
                    .trim(),
            )
        } else if trimmed.starts_with("import ") && trimmed.ends_with(';') {
            Some(
                trimmed
                    .trim_start_matches("import ")
                    .trim_end_matches(';')
                    .trim(),
            )
        } else {
            None
        };

        if let Some(module_name) = import_part {
            // Skip header unit imports (e.g., import <iostream>;)
            if module_name.starts_with('<') || module_name.starts_with('"') {
                continue;
            }

            // Qualify partition imports: `:vec3` → `parent_module:vec3`
            if module_name.starts_with(':') {
                if let Some(ref parent) = parent_module {
                    imports.push(format!("{}{}", parent, module_name));
                } else {
                    imports.push(module_name.to_string());
                }
            } else {
                imports.push(module_name.to_string());
            }
        }
    }

    Ok(imports)
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

/// Display build statistics to the user.
fn print_build_stats(stats: &BuildStats, shell: &Shell, timings: bool) {
    let total_nodes = stats.cache_hits + stats.cache_misses + stats.incremental_skipped;

    if total_nodes == 0 {
        return;
    }

    // Always show a summary line
    let mut parts = Vec::new();
    if stats.cache_misses > 0 {
        parts.push(format!("{} compiled", stats.cache_misses));
    }
    if stats.cache_hits > 0 {
        parts.push(format!("{} cached", stats.cache_hits));
    }
    if stats.incremental_skipped > 0 {
        parts.push(format!("{} up-to-date", stats.incremental_skipped));
    }

    shell.status(
        "Summary",
        format!(
            "{} modules ({}), {:.1}s",
            total_nodes,
            parts.join(", "),
            stats.wall_time_ms as f64 / 1000.0,
        ),
    );

    if stats.total_compile_time_ms > 0 && stats.wall_time_ms > 0 {
        let speedup = stats.total_compile_time_ms as f64 / stats.wall_time_ms as f64;
        if speedup > 1.05 {
            shell.verbose(
                "Parallel",
                format!(
                    "{:.1}x speedup ({:.1}s compile in {:.1}s wall)",
                    speedup,
                    stats.total_compile_time_ms as f64 / 1000.0,
                    stats.wall_time_ms as f64 / 1000.0,
                ),
            );
        }
    }

    // Per-node timings
    if timings && !stats.node_timings.is_empty() {
        shell.status("Timings", "per-module breakdown:");
        let mut sorted: Vec<_> = stats.node_timings.iter().collect();
        sorted.sort_by(|a, b| b.1.cmp(a.1)); // slowest first
        for (node_id, ms) in sorted {
            shell.status("", format!("{:>6}ms  {}", ms, node_id));
        }
    }
}

/// Execute a build lifecycle hook if configured.
///
/// Hooks run in the project root directory. A non-zero exit code fails the build.
/// Hook strings beginning with `plugin:` dispatch to the named plugin instead of
/// running as a shell command (e.g., `pre-build = "plugin:my-analyzer"`).
pub fn run_hook(
    config: &Config,
    hook_name: &str,
    command: Option<&str>,
    shell: &Shell,
) -> Result<(), CmodError> {
    let cmd = match command {
        Some(c) => c,
        None => return Ok(()),
    };

    // Dispatch to plugin runner if the hook uses the `plugin:` prefix
    if let Some(plugin_name) = cmd.strip_prefix("plugin:") {
        shell.status(
            "Running",
            format!("{} hook via plugin: {}", hook_name, plugin_name),
        );
        return super::plugin::run_plugin(plugin_name.trim(), &[], shell);
    }

    shell.status("Running", format!("{} hook: {}", hook_name, cmd));

    let status = std::process::Command::new("sh")
        .arg("-c")
        .arg(cmd)
        .current_dir(&config.root)
        .status()
        .map_err(|e| CmodError::BuildFailed {
            reason: format!("{} hook failed to execute: {}", hook_name, e),
        })?;

    if !status.success() {
        return Err(CmodError::BuildFailed {
            reason: format!(
                "{} hook failed with exit code {}",
                hook_name,
                status.code().unwrap_or(-1)
            ),
        });
    }

    Ok(())
}

/// Enforce the `[security] signature_policy` from the manifest.
///
/// - `"require"`: fail if any git dependency lacks a content hash (proxy for signature)
/// - `"warn"`: emit warnings for unsigned/unhashed deps
/// - `"none"` / absent: no enforcement
fn enforce_signature_policy(
    config: &Config,
    lockfile: &Lockfile,
    shell: &Shell,
) -> Result<(), CmodError> {
    let policy = config
        .manifest
        .security
        .as_ref()
        .and_then(|s| s.signature_policy.as_deref())
        .unwrap_or("none");

    match policy {
        "require" => {
            let mut unsigned = Vec::new();
            for pkg in &lockfile.packages {
                if pkg.source.as_deref() == Some("git") && pkg.hash.is_none() {
                    unsigned.push(pkg.name.clone());
                }
            }
            if !unsigned.is_empty() {
                return Err(CmodError::SecurityViolation {
                    reason: format!(
                        "signature_policy = \"require\" but {} package(s) have no content hash: {}. \
                         Re-run `cmod resolve` to compute hashes.",
                        unsigned.len(),
                        unsigned.join(", ")
                    ),
                });
            }
            shell.verbose(
                "Security",
                format!(
                    "all {} packages have content hashes",
                    lockfile.packages.len()
                ),
            );
        }
        "warn" => {
            for pkg in &lockfile.packages {
                if pkg.source.as_deref() == Some("git") && pkg.hash.is_none() {
                    shell.warn(format!(
                        "package '{}' has no content hash (signature_policy = \"warn\")",
                        pkg.name
                    ));
                }
            }
        }
        _ => {} // "none" or unset — no enforcement
    }
    Ok(())
}

/// Output the build plan as JSON without executing a build.
pub fn plan(shell: &Shell, target_override: Option<String>) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    let src_dirs = config.src_dirs();
    let exclude = config.exclude_patterns();
    let sources = runner::discover_sources_multi(&src_dirs, &exclude)?;

    if sources.is_empty() {
        let dirs: Vec<_> = src_dirs.iter().map(|d| d.display().to_string()).collect();
        return Err(CmodError::BuildFailed {
            reason: format!("no source files found in {}", dirs.join(", ")),
        });
    }

    let target = target_override
        .or_else(|| {
            config
                .manifest
                .toolchain
                .as_ref()
                .and_then(|tc| tc.target.clone())
        })
        .unwrap_or_else(default_target);

    let build_dir = config.build_dir();
    // Resolve the configured backend so plan output reflects its BMI naming
    // (and gcc/msvc fail fast, consistent with `cmod build`).
    let (plan_backend_cfg, plan_compiler_kind, _) = setup_compiler(&config, &[]);
    let plan_backend = cmod_build::compiler::make_backend(plan_compiler_kind, &plan_backend_cfg)?;
    let build_type = config
        .manifest
        .build
        .as_ref()
        .and_then(|b| b.build_type)
        .unwrap_or_default();

    // Collect dependency build plan nodes
    let mut all_plan_nodes = Vec::new();

    let lockfile = if config.lockfile_path.exists() {
        Some(Lockfile::load(&config.lockfile_path)?)
    } else {
        None
    };

    if let Some(ref lockfile) = lockfile {
        let vendor_dir = config.root.join("vendor");
        let deps_dir = config.deps_dir();
        let ordered = super::common::topo_sort_packages(&lockfile.packages);

        for pkg in &ordered {
            if pkg.source.as_deref() != Some("git") {
                continue;
            }

            let dep_dir =
                match super::common::ensure_dep_on_disk(pkg, &vendor_dir, &deps_dir, shell)? {
                    Some(d) => d,
                    None => continue,
                };

            let mut dep_config = Config::load(&dep_dir)?;
            dep_config.profile = config.profile;

            let dep_src_dirs = dep_config.src_dirs();
            let dep_exclude = dep_config.exclude_patterns();
            let dep_sources =
                runner::discover_sources_multi(&dep_src_dirs, &dep_exclude).unwrap_or_default();

            if dep_sources.is_empty() {
                continue;
            }

            if let Ok(dep_graph) =
                build_module_graph(&dep_sources, &dep_config.manifest.package.name)
            {
                let dep_build_dir = dep_config.build_dir();
                let dep_build_type = dep_config
                    .manifest
                    .build
                    .as_ref()
                    .and_then(|b| b.build_type)
                    .unwrap_or_default();

                if let Ok(dep_plan) = cmod_build::plan::BuildPlan::from_graph(
                    &dep_graph,
                    &dep_build_dir,
                    &target,
                    config.profile,
                    dep_build_type,
                    Some(&dep_config.manifest.package.name),
                    plan_backend.bmi_extension(),
                ) {
                    all_plan_nodes.extend(dep_plan.nodes);
                }
            }
        }
    }

    // Add the main project's plan nodes
    let graph = build_module_graph(&sources, &config.manifest.package.name)?;
    let plan = cmod_build::plan::BuildPlan::from_graph(
        &graph,
        &build_dir,
        &target,
        config.profile,
        build_type,
        Some(&config.manifest.package.name),
        plan_backend.bmi_extension(),
    )?;
    all_plan_nodes.extend(plan.nodes);

    let json =
        serde_json::to_string_pretty(&all_plan_nodes).map_err(|e| CmodError::BuildFailed {
            reason: format!("failed to serialize build plan: {}", e),
        })?;

    println!("{}", json);

    shell.verbose("Plan", format!("{} nodes", all_plan_nodes.len()));

    Ok(())
}

/// Generate a CMakeLists.txt for interop with CMake-based projects.
pub fn emit_cmake(shell: &Shell) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    let src_dirs = config.src_dirs();
    let exclude = config.exclude_patterns();
    let sources = runner::discover_sources_multi(&src_dirs, &exclude)?;

    let cmake_path = config.root.join("CMakeLists.txt");

    let mut lines = vec![
        "# Generated by cmod — do not edit manually".to_string(),
        format!("cmake_minimum_required(VERSION 3.28)"),
        format!(
            "project({} VERSION {})",
            config.manifest.package.name, config.manifest.package.version
        ),
        String::new(),
        "set(CMAKE_CXX_STANDARD 20)".to_string(),
        "set(CMAKE_CXX_STANDARD_REQUIRED ON)".to_string(),
        String::new(),
    ];

    // Collect source files
    let source_files: Vec<String> = sources
        .iter()
        .map(|p| {
            p.strip_prefix(&config.root)
                .unwrap_or(p)
                .display()
                .to_string()
        })
        .collect();

    let build_type = config
        .manifest
        .build
        .as_ref()
        .and_then(|b| b.build_type)
        .unwrap_or_default();

    let target_type = match build_type {
        cmod_core::types::BuildType::StaticLib => "add_library",
        cmod_core::types::BuildType::SharedLib => "add_library",
        _ => "add_executable",
    };

    let modifier = match build_type {
        cmod_core::types::BuildType::StaticLib => " STATIC",
        cmod_core::types::BuildType::SharedLib => " SHARED",
        _ => "",
    };

    lines.push(format!(
        "{}({}{}",
        target_type, config.manifest.package.name, modifier
    ));
    for src in &source_files {
        lines.push(format!("    {}", src));
    }
    lines.push(")".to_string());

    let content = lines.join("\n") + "\n";
    std::fs::write(&cmake_path, &content)?;

    shell.status("Generated", format!("{}", cmake_path.display()));
    shell.verbose("Sources", format!("{} source file(s)", source_files.len()));

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;
    use tempfile::TempDir;

    #[test]
    fn test_extract_imports_module() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("test.cppm");
        std::fs::write(&file, "export module mymod;\nimport base;\nimport utils;\n").unwrap();

        let imports = extract_imports_from_source(&file).unwrap();
        assert_eq!(imports, vec!["base", "utils"]);
    }

    #[test]
    fn test_extract_imports_skips_header_units() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("test.cpp");
        std::fs::write(
            &file,
            "import <iostream>;\nimport \"local.h\";\nimport mymod;\n",
        )
        .unwrap();

        let imports = extract_imports_from_source(&file).unwrap();
        // Should only include mymod, not <iostream> or "local.h"
        assert_eq!(imports, vec!["mymod"]);
    }

    #[test]
    fn test_extract_imports_empty() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("test.cpp");
        std::fs::write(&file, "int main() { return 0; }\n").unwrap();

        let imports = extract_imports_from_source(&file).unwrap();
        assert!(imports.is_empty());
    }

    #[test]
    fn test_extract_imports_with_whitespace() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("test.cppm");
        std::fs::write(&file, "  import   base  ;\n\timport utils;\n").unwrap();

        let imports = extract_imports_from_source(&file).unwrap();
        assert_eq!(imports, vec!["base", "utils"]);
    }

    #[test]
    fn test_extract_imports_partition() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("mat4.cppm");
        std::fs::write(
            &file,
            "export module mylib:mat4;\nimport :vec3;\nimport :utils;\n",
        )
        .unwrap();

        let imports = extract_imports_from_source(&file).unwrap();
        // Partition imports should be qualified with the parent module
        assert_eq!(imports, vec!["mylib:vec3", "mylib:utils"]);
    }

    #[test]
    fn test_extract_imports_export_import() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("lib.cppm");
        std::fs::write(
            &file,
            "export module mylib;\nexport import :vec3;\nexport import :mat4;\nimport base;\n",
        )
        .unwrap();

        let imports = extract_imports_from_source(&file).unwrap();
        assert_eq!(imports, vec!["mylib:vec3", "mylib:mat4", "base"]);
    }

    #[test]
    fn test_default_target_is_not_empty() {
        let target = default_target();
        assert!(!target.is_empty());
        // Should contain arch and os info
        assert!(target.contains('-'));
    }

    #[test]
    fn test_build_module_graph_single_file() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("lib.cppm");
        std::fs::write(&file, "export module mymod;\n\nvoid hello() {}\n").unwrap();

        let sources = vec![file];
        let graph = build_module_graph(&sources, "test_pkg").unwrap();

        assert_eq!(graph.nodes.len(), 1);
        // Nodes are keyed by source path, find by module name
        let node = graph.nodes.values().find(|n| n.name == "mymod").unwrap();
        assert_eq!(node.package, "test_pkg");
    }

    #[test]
    fn test_build_module_graph_filters_external_imports() {
        let tmp = TempDir::new().unwrap();

        let base = tmp.path().join("base.cppm");
        std::fs::write(&base, "export module base;\n").unwrap();

        let app = tmp.path().join("app.cppm");
        std::fs::write(
            &app,
            "export module app;\nimport base;\nimport external_lib;\n",
        )
        .unwrap();

        let sources = vec![base, app];
        let graph = build_module_graph(&sources, "test").unwrap();

        // app should only import base (external_lib filtered out)
        let app_node = graph.nodes.values().find(|n| n.name == "app").unwrap();
        assert_eq!(app_node.imports, vec!["base"]);
    }

    #[test]
    fn test_build_module_graph_legacy_source() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("main.cpp");
        std::fs::write(&file, "#include <stdio.h>\nint main() {}\n").unwrap();

        let sources = vec![file];
        let graph = build_module_graph(&sources, "test").unwrap();

        // Legacy files use filename as module name, nodes keyed by source path
        assert_eq!(graph.nodes.len(), 1);
        let node = graph.nodes.values().find(|n| n.name == "main").unwrap();
        assert_eq!(node.name, "main");
    }

    #[test]
    fn test_parse_p1689_imports() {
        let json = r#"{
            "version": 1,
            "rules": [{
                "primary-output": "test.o",
                "provides": [{"logical-name": "mymod", "is-interface": true}],
                "requires": [
                    {"logical-name": "base"},
                    {"logical-name": "utils"}
                ]
            }]
        }"#;

        let imports = parse_p1689_imports(json).unwrap();
        assert_eq!(imports, vec!["base", "utils"]);
    }

    #[test]
    fn test_parse_p1689_no_requires() {
        let json = r#"{
            "version": 1,
            "rules": [{
                "primary-output": "test.o",
                "provides": [{"logical-name": "mymod"}]
            }]
        }"#;

        let imports = parse_p1689_imports(json).unwrap();
        assert!(imports.is_empty());
    }

    #[test]
    fn test_parse_p1689_empty_rules() {
        let json = r#"{"version": 1, "rules": []}"#;
        let imports = parse_p1689_imports(json).unwrap();
        assert!(imports.is_empty());
    }

    #[test]
    fn test_parse_p1689_invalid_json() {
        let result = parse_p1689_imports("not json");
        assert!(result.is_err());
    }

    #[test]
    fn test_print_build_stats_no_panic() {
        // Verify stats printing doesn't panic for various states
        let shell_normal = Shell::new(Verbosity::Normal);
        let shell_verbose = Shell::new(Verbosity::Verbose);
        let empty = BuildStats::default();
        print_build_stats(&empty, &shell_normal, false);
        print_build_stats(&empty, &shell_verbose, false);

        let stats = BuildStats {
            cache_hits: 3,
            cache_misses: 2,
            skipped: 1,
            incremental_skipped: 5,
            wall_time_ms: 1500,
            total_compile_time_ms: 4000,
            node_timings: BTreeMap::new(),
        };
        print_build_stats(&stats, &shell_normal, false);
        print_build_stats(&stats, &shell_verbose, false);
    }

    #[test]
    fn test_print_build_stats_with_timings() {
        let shell_normal = Shell::new(Verbosity::Normal);
        let shell_verbose = Shell::new(Verbosity::Verbose);
        let mut node_timings = BTreeMap::new();
        node_timings.insert("interface:base".to_string(), 120);
        node_timings.insert("impl:app".to_string(), 340);
        node_timings.insert("object:main".to_string(), 80);

        let stats = BuildStats {
            cache_hits: 0,
            cache_misses: 3,
            skipped: 0,
            incremental_skipped: 0,
            wall_time_ms: 500,
            total_compile_time_ms: 540,
            node_timings,
        };

        // Should not panic with timings enabled
        print_build_stats(&stats, &shell_normal, true);
        print_build_stats(&stats, &shell_verbose, true);
    }

    #[test]
    fn test_run_hook_none_is_noop() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("cmod.toml"),
            "[package]\nname = \"test\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        let config = Config::load(tmp.path()).unwrap();
        let shell = Shell::new(cmod_core::shell::Verbosity::Quiet);
        // None command should be a no-op
        assert!(run_hook(&config, "pre-build", None, &shell).is_ok());
    }

    #[test]
    fn test_run_hook_plugin_prefix_detection() {
        // Verify that "plugin:" prefix is detected
        let cmd = "plugin:my-analyzer";
        assert!(cmd.strip_prefix("plugin:").is_some());
        assert_eq!(cmd.strip_prefix("plugin:").unwrap(), "my-analyzer");

        // Regular commands should not have the prefix
        let regular = "echo hello";
        assert!(regular.strip_prefix("plugin:").is_none());
    }
}
