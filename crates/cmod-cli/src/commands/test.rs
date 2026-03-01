use std::path::Path;

use cmod_core::error::CmodError;

/// Run `cmod test` — build and run tests.
pub fn run(
    release: bool,
    locked: bool,
    offline: bool,
    verbose: bool,
    target: Option<String>,
) -> Result<(), CmodError> {
    // First, build the project
    super::build::run(release, locked, offline, verbose, target, 0, false, None, false, false, false, &[], false)?;

    eprintln!("  Running tests...");

    // Look for test files
    let cwd = std::env::current_dir()?;
    let config = cmod_core::config::Config::load(&cwd)?;

    // Run pre-test hook
    super::build::run_hook(
        &config,
        "pre-test",
        config.manifest.hooks.as_ref().and_then(|h| h.pre_test.as_deref()),
    )?;

    // Get test patterns from manifest
    let (test_patterns, exclude_patterns) = match config.manifest.test.as_ref() {
        Some(test_cfg) => (test_cfg.test_patterns.clone(), test_cfg.exclude_patterns.clone()),
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
    let build_dir = config.build_dir();
    let cxx_standard = config
        .manifest
        .toolchain
        .as_ref()
        .and_then(|tc| tc.cxx_standard.clone())
        .unwrap_or_else(|| "20".to_string());

    for test_source in &filtered_sources {
        let test_name = test_source
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("test");

        eprintln!("  Running test: {}", test_name);

        // Compile the test
        let test_binary = build_dir.join(format!("test_{}", test_name));

        let status = std::process::Command::new("clang++")
            .arg(format!("-std=c++{}", cxx_standard))
            .arg("-o")
            .arg(&test_binary)
            .arg(test_source)
            .status()
            .map_err(|e| CmodError::BuildFailed {
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
                reason: format!("test '{}' failed with exit code {:?}", test_name, test_status.code()),
            });
        }

        eprintln!("  Test '{}' passed.", test_name);
    }

    // Run post-test hook
    super::build::run_hook(
        &config,
        "post-test",
        config.manifest.hooks.as_ref().and_then(|h| h.post_test.as_deref()),
    )?;

    eprintln!("  All tests passed.");
    Ok(())
}

/// Check if a test source matches the configured patterns.
///
/// If `test_patterns` is empty, all sources match (no filtering).
/// If `test_patterns` is non-empty, the filename must contain at least one pattern.
/// If `exclude_patterns` is non-empty, the filename must not contain any exclude pattern.
fn matches_test_patterns(source: &Path, test_patterns: &[String], exclude_patterns: &[String]) -> bool {
    let filename = source
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");

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
    test_patterns.iter().any(|pattern| filename.contains(pattern.as_str()))
}
