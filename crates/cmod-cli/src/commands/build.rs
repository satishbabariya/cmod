use cmod_build::compiler::ClangBackend;
use cmod_build::graph::{ModuleGraph, ModuleNode};
use cmod_build::runner::{self, BuildRunner};
use cmod_cache::ArtifactCache;
use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::lockfile::Lockfile;
use cmod_core::types::Profile;
use cmod_resolver::Resolver;

/// Run `cmod build` — build the current module or workspace.
pub fn run(
    release: bool,
    locked: bool,
    offline: bool,
    verbose: bool,
    target_override: Option<String>,
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
    config.verbose = verbose;
    if let Some(t) = target_override {
        config.target = Some(t);
    }

    eprintln!(
        "  Building {} ({})",
        config.manifest.package.name,
        match config.profile {
            Profile::Debug => "debug",
            Profile::Release => "release",
        }
    );

    // Step 1: Ensure dependencies are resolved
    let _lockfile = ensure_resolved(&config)?;

    // Step 2: Discover source files
    let src_dir = config.src_dir();
    let sources = runner::discover_sources(&src_dir)?;

    if sources.is_empty() {
        return Err(CmodError::BuildFailed {
            reason: format!("no source files found in {}", src_dir.display()),
        });
    }

    if verbose {
        eprintln!("  Found {} source files", sources.len());
    }

    // Step 3: Build the module graph
    let mut graph = ModuleGraph::new();

    for source in &sources {
        let kind = runner::classify_source(source)?;
        let module_name = runner::extract_module_name(source)?
            .unwrap_or_else(|| {
                source
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string()
            });

        // For now, use simple import detection from the source content
        let imports = extract_imports_from_source(source)?;

        graph.add_node(ModuleNode {
            name: module_name,
            kind,
            source: source.clone(),
            package: config.manifest.package.name.clone(),
            imports,
        });
    }

    // Step 4: Set up the compiler backend
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

    // Step 5: Set up cache
    let cache = ArtifactCache::new(config.cache_dir());

    // Step 6: Execute the build
    let build_dir = config.build_dir();
    let build_type = config
        .manifest
        .build
        .as_ref()
        .and_then(|b| b.build_type)
        .unwrap_or_default();

    let runner = BuildRunner::new(backend, Some(cache));
    let output = runner.build(&graph, &build_dir, &target, config.profile, build_type)?;

    eprintln!("  Build complete: {}", output.display());

    Ok(())
}

/// Ensure dependencies are resolved; if lockfile exists, load it.
fn ensure_resolved(config: &Config) -> Result<Lockfile, CmodError> {
    if config.lockfile_path.exists() {
        Lockfile::load(&config.lockfile_path)
    } else if config.manifest.dependencies.is_empty() {
        Ok(Lockfile::new())
    } else if config.locked {
        Err(CmodError::LockfileNotFound)
    } else {
        // Auto-resolve
        eprintln!("  No lockfile found, resolving dependencies...");
        let resolver = Resolver::new(config.deps_dir());
        let lockfile =
            resolver.resolve(&config.manifest, None, false, config.offline)?;
        lockfile.save(&config.lockfile_path)?;
        Ok(lockfile)
    }
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
            // Skip header unit imports (e.g., import <iostream>;)
            if !module_name.starts_with('<') && !module_name.starts_with('"') {
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
