use std::path::Path;

use cmod_core::error::CmodError;

/// Run `cmod test` — build and run tests.
pub fn run(
    release: bool,
    locked: bool,
    offline: bool,
    verbose: bool,
    target: Option<String>,
    no_cache: bool,
) -> Result<(), CmodError> {
    // First, build the project
    super::build::run(
        release,
        locked,
        offline,
        verbose,
        target,
        0,
        false,
        None,
        false,
        false,
        false,
        &[],
        false,
        no_cache,
    )?;

    eprintln!("  Running tests...");

    // Look for test files
    let cwd = std::env::current_dir()?;
    let config = cmod_core::config::Config::load(&cwd)?;

    // Run pre-test hook
    super::build::run_hook(
        &config,
        "pre-test",
        config
            .manifest
            .hooks
            .as_ref()
            .and_then(|h| h.pre_test.as_deref()),
    )?;

    // Get test patterns from manifest
    let (test_patterns, exclude_patterns) = match config.manifest.test.as_ref() {
        Some(test_cfg) => (
            test_cfg.test_patterns.clone(),
            test_cfg.exclude_patterns.clone(),
        ),
        None => (vec![], vec![]),
    };

    let test_dir = cwd.join("tests");

    if !test_dir.exists() {
        eprintln!("  No tests directory found, skipping.");
        return Ok(());
    }

    let test_sources = cmod_build::runner::discover_sources(&test_dir)?;
    if test_sources.is_empty() {
        eprintln!("  No test sources found.");
        return Ok(());
    }

    // Filter test sources by patterns if configured
    let filtered_sources: Vec<_> = test_sources
        .into_iter()
        .filter(|src| matches_test_patterns(src, &test_patterns, &exclude_patterns))
        .collect();

    if filtered_sources.is_empty() {
        eprintln!("  No test sources match the configured patterns.");
        return Ok(());
    }

    // Build and run each test file
    // config.build_dir() already includes the profile subdirectory (e.g., build/debug)
    let build_dir = config.build_dir();
    let pcm_dir = build_dir.join("pcm");
    let obj_dir = build_dir.join("obj");

    let cxx_standard = config
        .manifest
        .toolchain
        .as_ref()
        .and_then(|tc| tc.cxx_standard.clone())
        .unwrap_or_else(|| "20".to_string());

    // Use the same target triple as the main build to avoid PCM mismatch
    let target_triple = config
        .target
        .clone()
        .or_else(|| {
            config
                .manifest
                .toolchain
                .as_ref()
                .and_then(|tc| tc.target.clone())
        })
        .unwrap_or_else(|| {
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
        });

    // Collect PCM and object files from the build for linking with tests.
    //
    // Build a module-name → PCM-path map by discovering source files and
    // extracting their declared module names (same as the build does).
    let mut pcm_flags: Vec<String> = Vec::new();
    let mut obj_files: Vec<String> = Vec::new();

    if pcm_dir.exists() {
        // Build a sanitized-stem → module-name map from source files
        let src_dir = config.src_dir();
        let mut name_map: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        if let Ok(sources) = cmod_build::runner::discover_sources(&src_dir) {
            for source in &sources {
                if let Ok(Some(mod_name)) = cmod_build::runner::extract_module_name(source) {
                    let sanitized = mod_name.replace(['.', ':', '/'], "_");
                    name_map.insert(sanitized, mod_name);
                }
            }
        }

        if let Ok(entries) = std::fs::read_dir(&pcm_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("pcm") {
                    if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                        // Look up the real module name from the sanitized filename
                        let module_name = name_map
                            .get(stem)
                            .cloned()
                            .unwrap_or_else(|| stem.to_string());
                        pcm_flags.push(format!("-fmodule-file={}={}", module_name, path.display()));
                    }
                }
            }
        }
    }

    if obj_dir.exists() {
        if let Ok(entries) = std::fs::read_dir(&obj_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().and_then(|e| e.to_str()) == Some("o") {
                    // Skip the main entry point object file to avoid
                    // "multiple definition of main" when linking tests
                    if path.file_stem().and_then(|s| s.to_str()) == Some("main") {
                        continue;
                    }
                    obj_files.push(path.display().to_string());
                }
            }
        }
    }

    for test_source in &filtered_sources {
        let test_name = test_source
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("test");

        eprintln!("  Running test: {}", test_name);

        // Compile the test with module precompiled paths
        let test_binary = build_dir.join(format!("test_{}", test_name));

        let clang_path =
            std::env::var_os("CXX").unwrap_or_else(|| std::ffi::OsString::from("clang++"));
        let mut cmd = std::process::Command::new(&clang_path);
        cmd.arg(format!("-std=c++{}", cxx_standard));
        cmd.arg(format!("--target={}", target_triple));

        // Add precompiled module references so import statements resolve
        for flag in &pcm_flags {
            cmd.arg(flag);
        }

        cmd.arg("-o").arg(&test_binary).arg(test_source);

        // Link against object files from the main build
        for obj in &obj_files {
            cmd.arg(obj);
        }

        let status = cmd.status().map_err(|e| CmodError::BuildFailed {
            reason: format!("failed to compile test: {}", e),
        })?;

        if !status.success() {
            return Err(CmodError::BuildFailed {
                reason: format!("test compilation failed: {}", test_name),
            });
        }

        // Run the test binary
        let test_status = std::process::Command::new(&test_binary)
            .status()
            .map_err(|e| CmodError::BuildFailed {
                reason: format!("failed to run test: {}", e),
            })?;

        if !test_status.success() {
            return Err(CmodError::BuildFailed {
                reason: format!(
                    "test '{}' failed with exit code {:?}",
                    test_name,
                    test_status.code()
                ),
            });
        }

        eprintln!("  Test '{}' passed.", test_name);
    }

    // Run post-test hook
    super::build::run_hook(
        &config,
        "post-test",
        config
            .manifest
            .hooks
            .as_ref()
            .and_then(|h| h.post_test.as_deref()),
    )?;

    eprintln!("  All tests passed.");
    Ok(())
}

/// Check if a test source matches the configured patterns.
///
/// If `test_patterns` is empty, all sources match (no filtering).
/// If `test_patterns` is non-empty, the filename must contain at least one pattern.
/// If `exclude_patterns` is non-empty, the filename must not contain any exclude pattern.
fn matches_test_patterns(
    source: &Path,
    test_patterns: &[String],
    exclude_patterns: &[String],
) -> bool {
    let filename = source.file_name().and_then(|s| s.to_str()).unwrap_or("");

    // Check exclude patterns first
    for pattern in exclude_patterns {
        if filename.contains(pattern.as_str()) {
            return false;
        }
    }

    // If no include patterns, match everything
    if test_patterns.is_empty() {
        return true;
    }

    // Must match at least one include pattern
    test_patterns
        .iter()
        .any(|pattern| filename.contains(pattern.as_str()))
}
