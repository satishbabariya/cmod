use cmod_build::compiler::ClangBackend;
use cmod_build::graph::{ModuleGraph, ModuleNode};
use cmod_build::runner::{self, BuildRunner, BuildStats};
use cmod_cache::{ArtifactCache, HttpRemoteCache, RemoteCacheMode};
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
    jobs: usize,
    force: bool,
    remote_cache_url: Option<String>,
    no_hooks: bool,
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
        return build_workspace(&config, verbose, jobs, force, &effective_remote_url);
    }

    eprintln!(
        "  Building {} ({})",
        config.manifest.package.name, profile_name
    );

    // Step 1: Ensure dependencies are resolved (with target-specific filtering)
    let _lockfile = ensure_resolved(&config)?;

    // Step 2: Run pre-build hook
    if !no_hooks {
        run_hook(&config, "pre-build", config.manifest.hooks.as_ref().and_then(|h| h.pre_build.as_deref()))?;
    }

    // Step 3: Build the single module
    let result = build_module(&config, verbose, jobs, force, &effective_remote_url);

    // Step 4: Run post-build hook (only on success)
    if result.is_ok() && !no_hooks {
        run_hook(&config, "post-build", config.manifest.hooks.as_ref().and_then(|h| h.post_build.as_deref()))?;
    }

    result
}

/// Create a remote cache instance from a URL, if provided.
fn make_remote_cache(url: &Option<String>, verbose: bool) -> Option<Box<dyn cmod_cache::RemoteCache>> {
    let url = url.as_ref()?;
    if verbose {
        eprintln!("  Remote cache: {}", url);
    }
    Some(Box::new(HttpRemoteCache::new(url, RemoteCacheMode::ReadWrite)))
}

/// Build a single module project.
fn build_module(config: &Config, verbose: bool, jobs: usize, force: bool, remote_url: &Option<String>) -> Result<(), CmodError> {
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

    // Validate the module graph (imports, cycles, duplicates)
    graph.validate()?;

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

    let mut runner = BuildRunner::new(backend, Some(cache))
        .with_jobs(jobs)
        .with_force(force);

    if let Some(remote) = make_remote_cache(remote_url, verbose) {
        runner = runner.with_remote_cache(remote);
    }

    if verbose && jobs != 1 {
        eprintln!("  Parallelism: {} jobs", runner.effective_jobs());
    }

    let (output, stats) = runner.build_with_stats(&graph, &build_dir, &target, config.profile, build_type)?;

    print_build_stats(&stats, verbose);
    eprintln!("  Build complete: {}", output.display());
    Ok(())
}

/// Build all members of a workspace.
fn build_workspace(config: &Config, verbose: bool, jobs: usize, force: bool, remote_url: &Option<String>) -> Result<(), CmodError> {
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
        graph.validate()?;
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

        let mut runner = BuildRunner::new(backend, Some(cache))
            .with_jobs(jobs)
            .with_force(force);
        if let Some(remote) = make_remote_cache(remote_url, verbose) {
            runner = runner.with_remote_cache(remote);
        }
        match runner.build_with_stats(&graph, &build_dir, &target, config.profile, build_type) {
            Ok((output, stats)) => {
                print_build_stats(&stats, verbose);
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
        let module_name = runner::extract_module_name(source)?
            .unwrap_or_else(|| {
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

/// Check if `clang-scan-deps` is available on PATH.
fn is_clang_scan_deps_available() -> bool {
    std::process::Command::new("clang-scan-deps")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Use `clang-scan-deps` to discover module dependencies via P1689 format.
fn scan_deps_imports(source: &std::path::Path) -> Result<Vec<String>, CmodError> {
    let output = std::process::Command::new("clang-scan-deps")
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
    let value: serde_json::Value = serde_json::from_str(json_str).map_err(|e| {
        CmodError::ModuleScanFailed {
            reason: format!("failed to parse P1689 output: {}", e),
        }
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

/// Ensure dependencies are resolved; if lockfile exists, load it.
fn ensure_resolved(config: &Config) -> Result<Lockfile, CmodError> {
    if config.lockfile_path.exists() {
        Lockfile::load(&config.lockfile_path)
    } else if config.manifest.dependencies.is_empty() && config.manifest.target.is_empty() {
        Ok(Lockfile::new())
    } else if config.locked {
        Err(CmodError::LockfileNotFound)
    } else {
        // Auto-resolve with target-specific dependency filtering
        eprintln!("  No lockfile found, resolving dependencies...");
        let mut resolver = Resolver::new(config.deps_dir());
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

/// Display build statistics to the user.
fn print_build_stats(stats: &BuildStats, verbose: bool) {
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

    eprintln!(
        "  {} modules ({}), {:.1}s",
        total_nodes,
        parts.join(", "),
        stats.wall_time_ms as f64 / 1000.0,
    );

    if verbose && stats.total_compile_time_ms > 0 && stats.wall_time_ms > 0 {
        let speedup = stats.total_compile_time_ms as f64 / stats.wall_time_ms as f64;
        if speedup > 1.05 {
            eprintln!(
                "  Parallel speedup: {:.1}x ({:.1}s compile time in {:.1}s wall time)",
                speedup,
                stats.total_compile_time_ms as f64 / 1000.0,
                stats.wall_time_ms as f64 / 1000.0,
            );
        }
    }
}

/// Execute a build lifecycle hook if configured.
///
/// Hooks run in the project root directory. A non-zero exit code fails the build.
pub fn run_hook(config: &Config, hook_name: &str, command: Option<&str>) -> Result<(), CmodError> {
    let cmd = match command {
        Some(c) => c,
        None => return Ok(()),
    };

    eprintln!("  Running {} hook: {}", hook_name, cmd);

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
        let empty = BuildStats::default();
        print_build_stats(&empty, false);
        print_build_stats(&empty, true);

        let stats = BuildStats {
            cache_hits: 3,
            cache_misses: 2,
            skipped: 1,
            incremental_skipped: 5,
            wall_time_ms: 1500,
            total_compile_time_ms: 4000,
        };
        print_build_stats(&stats, false);
        print_build_stats(&stats, true);
    }
}
