use cmod_build::compiler::ClangBackend;
use cmod_build::graph::{ModuleGraph, ModuleNode};
use cmod_build::runner::{self, BuildRunner};
use cmod_cache::ArtifactCache;
use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::lockfile::Lockfile;
use cmod_core::types::Profile;
use cmod_resolver::Resolver;
use cmod_workspace::WorkspaceManager;

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

    let profile_name = match config.profile {
        Profile::Debug => "debug",
        Profile::Release => "release",
    };

    // Check if this is a workspace build
    if config.manifest.is_workspace() {
        return build_workspace(&config, verbose);
    }

    eprintln!(
        "  Building {} ({})",
        config.manifest.package.name, profile_name
    );

    // Step 1: Ensure dependencies are resolved
    let _lockfile = ensure_resolved(&config)?;

    // Step 2: Build the single module
    build_module(&config, verbose)
}

/// Build a single module project.
fn build_module(config: &Config, verbose: bool) -> Result<(), CmodError> {
    // Discover source files
    let src_dir = config.src_dir();
    let sources = runner::discover_sources(&src_dir)?;

    if sources.is_empty() {
        return Err(CmodError::BuildFailed {
            reason: format!("no source files found in {}", src_dir.display()),
        });
    }

    if verbose {
        eprintln!("  Found {} source files", sources.len());
        for s in &sources {
            eprintln!("    {}", s.display());
        }
    }

    // Build the module graph
    let graph = build_module_graph(&sources, &config.manifest.package.name)?;

    if verbose {
        let order = graph.topological_order()?;
        eprintln!("  Build order: {}", order.join(" → "));
    }

    // Set up the compiler backend
    let (backend, target) = setup_compiler(config);

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

    let runner = BuildRunner::new(backend, Some(cache));
    let output = runner.build(&graph, &build_dir, &target, config.profile, build_type)?;

    eprintln!("  Build complete: {}", output.display());
    Ok(())
}

/// Build all members of a workspace.
fn build_workspace(config: &Config, verbose: bool) -> Result<(), CmodError> {
    let ws = WorkspaceManager::load(&config.root)?;

    eprintln!(
        "  Building workspace ({} members, {})",
        ws.members.len(),
        match config.profile {
            Profile::Debug => "debug",
            Profile::Release => "release",
        }
    );

    // Ensure dependencies are resolved
    let _lockfile = ensure_resolved(config)?;

    let mut failed = Vec::new();

    for member in &ws.members {
        eprintln!("  Building member: {}", member.name);

        let member_src = member.path.join("src");
        let sources = runner::discover_sources(&member_src)?;

        if sources.is_empty() {
            if verbose {
                eprintln!("    No source files, skipping.");
            }
            continue;
        }

        let graph = build_module_graph(&sources, &member.name)?;
        let (backend, target) = setup_compiler(config);
        let cache = ArtifactCache::new(config.cache_dir());

        let build_dir = config
            .build_dir()
            .join(&member.name);

        let build_type = member
            .manifest
            .build
            .as_ref()
            .and_then(|b| b.build_type)
            .unwrap_or_default();

        let runner = BuildRunner::new(backend, Some(cache));
        match runner.build(&graph, &build_dir, &target, config.profile, build_type) {
            Ok(output) => {
                eprintln!("    Built: {}", output.display());
            }
            Err(e) => {
                eprintln!("    Failed: {}", e);
                failed.push(member.name.clone());
            }
        }
    }

    if !failed.is_empty() {
        return Err(CmodError::BuildFailed {
            reason: format!(
                "workspace build failed for members: {}",
                failed.join(", ")
            ),
        });
    }

    eprintln!("  Workspace build complete.");
    Ok(())
}

/// Build a ModuleGraph from discovered source files.
fn build_module_graph(
    sources: &[std::path::PathBuf],
    package_name: &str,
) -> Result<ModuleGraph, CmodError> {
    let mut graph = ModuleGraph::new();

    for source in sources {
        let kind = runner::classify_source(source)?;
        let module_name = runner::extract_module_name(source)?
            .unwrap_or_else(|| {
                source
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string()
            });

        let imports = extract_imports_from_source(source)?;

        // Filter out imports for modules not in this graph (external deps)
        // They will be resolved via pre-built PCMs at compile time.
        graph.add_node(ModuleNode {
            name: module_name,
            kind,
            source: source.clone(),
            package: package_name.to_string(),
            imports,
        });
    }

    // Filter imports to only include modules that exist in the graph
    let known_modules: std::collections::BTreeSet<String> =
        graph.nodes.keys().cloned().collect();
    for node in graph.nodes.values_mut() {
        node.imports.retain(|imp| known_modules.contains(imp));
    }

    Ok(graph)
}

/// Set up the Clang compiler backend from config.
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

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_extract_imports_module() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("test.cppm");
        std::fs::write(
            &file,
            "export module mymod;\nimport base;\nimport utils;\n",
        )
        .unwrap();

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
        std::fs::write(
            &file,
            "  import   base  ;\n\timport utils;\n",
        )
        .unwrap();

        let imports = extract_imports_from_source(&file).unwrap();
        assert_eq!(imports, vec!["base", "utils"]);
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
        assert!(graph.nodes.contains_key("mymod"));
        assert_eq!(graph.nodes["mymod"].package, "test_pkg");
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
        let app_node = &graph.nodes["app"];
        assert_eq!(app_node.imports, vec!["base"]);
    }

    #[test]
    fn test_build_module_graph_legacy_source() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("main.cpp");
        std::fs::write(&file, "#include <stdio.h>\nint main() {}\n").unwrap();

        let sources = vec![file];
        let graph = build_module_graph(&sources, "test").unwrap();

        // Legacy files use filename as module name
        assert_eq!(graph.nodes.len(), 1);
        assert!(graph.nodes.contains_key("main"));
    }
}
