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
    super::build::run(release, locked, offline, verbose, target, 0, false, None, false)?;

    eprintln!("  Running tests...");

    // Look for test files
    let cwd = std::env::current_dir()?;
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

    // Build and run each test file
    for test_source in &test_sources {
        let test_name = test_source
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("test");

        eprintln!("  Running test: {}", test_name);

        // Compile the test
        let config = cmod_core::config::Config::load(&cwd)?;
        let build_dir = config.build_dir();
        let test_binary = build_dir.join(format!("test_{}", test_name));

        let cxx_standard = config
            .manifest
            .toolchain
            .as_ref()
            .and_then(|tc| tc.cxx_standard.clone())
            .unwrap_or_else(|| "20".to_string());

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

    eprintln!("  All tests passed.");
    Ok(())
}
