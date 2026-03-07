use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::shell::Shell;
use cmod_core::types::Profile;
use cmod_workspace::WorkspaceManager;

// ---------------------------------------------------------------------------
// Test result model
// ---------------------------------------------------------------------------

struct CompiledTest {
    name: String,
    source: PathBuf,
    binary_path: PathBuf,
}

struct TestResult {
    name: String,
    status: TestStatus,
    duration: Duration,
    stdout: String,
    stderr: String,
    source: PathBuf,
}

#[allow(dead_code)]
enum TestStatus {
    Passed,
    Failed { exit_code: Option<i32> },
    TimedOut,
    CompileFailed { reason: String },
    Skipped,
}

struct TestSummary {
    passed: usize,
    failed: usize,
    skipped: usize,
    timed_out: usize,
    total_duration: Duration,
    results: Vec<TestResult>,
}

impl TestSummary {
    fn new() -> Self {
        Self {
            passed: 0,
            failed: 0,
            skipped: 0,
            timed_out: 0,
            total_duration: Duration::ZERO,
            results: Vec::new(),
        }
    }

    fn merge(&mut self, other: TestSummary) {
        self.passed += other.passed;
        self.failed += other.failed;
        self.skipped += other.skipped;
        self.timed_out += other.timed_out;
        self.total_duration += other.total_duration;
        self.results.extend(other.results);
    }

    fn is_success(&self) -> bool {
        self.failed == 0 && self.timed_out == 0
    }

    fn total(&self) -> usize {
        self.passed + self.failed + self.skipped + self.timed_out
    }
}

// ---------------------------------------------------------------------------
// CLI options struct (to avoid too many parameters)
// ---------------------------------------------------------------------------

struct TestOptions {
    release: bool,
    locked: bool,
    offline: bool,
    target: Option<String>,
    no_cache: bool,
    name: Option<String>,
    filter: Option<String>,
    jobs: usize,
    no_fail_fast: bool,
    timeout: u64,
    package: Option<String>,
    coverage: bool,
    sanitize: Vec<String>,
    format: Option<String>,
}

// ---------------------------------------------------------------------------
// Entry point
// ---------------------------------------------------------------------------

/// Run `cmod test` — build and run tests.
#[allow(clippy::too_many_arguments)]
pub fn run(
    release: bool,
    locked: bool,
    offline: bool,
    shell: &Shell,
    target: Option<String>,
    no_cache: bool,
    name: Option<String>,
    filter: Option<String>,
    jobs: usize,
    no_fail_fast: bool,
    timeout: u64,
    package: Option<String>,
    coverage: bool,
    sanitize: Vec<String>,
    format: Option<String>,
) -> Result<(), CmodError> {
    let opts = TestOptions {
        release,
        locked,
        offline,
        target,
        no_cache,
        name,
        filter,
        jobs,
        no_fail_fast,
        timeout,
        package,
        coverage,
        sanitize,
        format,
    };

    // Build the project first
    super::build::run(
        opts.release,
        opts.locked,
        opts.offline,
        shell,
        opts.target.clone(),
        0,
        false,
        None,
        false,
        false,
        false,
        &[],
        false,
        opts.no_cache,
        false,
        vec![],
    )?;

    let cwd = std::env::current_dir()?;
    let mut config = Config::load(&cwd)?;
    config.profile = if opts.release {
        Profile::Release
    } else {
        Profile::Debug
    };
    if let Some(ref t) = opts.target {
        config.target = Some(t.clone());
    }

    // Workspace support
    if config.manifest.is_workspace() {
        return test_workspace(&config, shell, &opts);
    }

    let summary = test_single_project(&config, shell, &opts)?;

    format_results(&summary, shell, &opts);

    if summary.is_success() {
        Ok(())
    } else {
        Err(CmodError::TestsFailed {
            count: summary.failed + summary.timed_out,
        })
    }
}

// ---------------------------------------------------------------------------
// Single-project testing
// ---------------------------------------------------------------------------

fn test_single_project(
    config: &Config,
    shell: &Shell,
    opts: &TestOptions,
) -> Result<TestSummary, CmodError> {
    shell.status("Testing", "running tests...");

    // Run pre-test hook
    super::build::run_hook(
        config,
        "pre-test",
        config
            .manifest
            .hooks
            .as_ref()
            .and_then(|h| h.pre_test.as_deref()),
        shell,
    )?;

    // Discover and filter test files
    let test_sources = discover_and_filter_tests(config, opts)?;

    if test_sources.is_empty() {
        shell.warn("No tests found, skipping");
        return Ok(TestSummary::new());
    }

    // Compile all tests
    let compiled = compile_tests(config, &test_sources, shell, opts)?;

    if compiled.is_empty() {
        shell.warn("no tests compiled successfully");
        return Ok(TestSummary::new());
    }

    // Execute tests (parallel or sequential)
    let summary = execute_tests(&compiled, opts, config, shell);

    // Run post-test hook
    super::build::run_hook(
        config,
        "post-test",
        config
            .manifest
            .hooks
            .as_ref()
            .and_then(|h| h.post_test.as_deref()),
        shell,
    )?;

    // Coverage report (after all tests)
    if opts.coverage {
        generate_coverage_report(config, &compiled, shell);
    }

    Ok(summary)
}

// ---------------------------------------------------------------------------
// Test discovery and filtering
// ---------------------------------------------------------------------------

fn discover_and_filter_tests(
    config: &Config,
    opts: &TestOptions,
) -> Result<Vec<PathBuf>, CmodError> {
    let (test_patterns, exclude_patterns) = match config.manifest.test.as_ref() {
        Some(test_cfg) => (
            test_cfg.test_patterns.clone(),
            test_cfg.exclude_patterns.clone(),
        ),
        None => (vec![], vec![]),
    };

    let test_dir = config.root.join("tests");
    if !test_dir.exists() {
        return Ok(vec![]);
    }

    let test_sources = cmod_build::runner::discover_sources(&test_dir)?;
    if test_sources.is_empty() {
        return Ok(vec![]);
    }

    let filtered: Vec<_> = test_sources
        .into_iter()
        .filter(|src| matches_test_patterns(src, &test_patterns, &exclude_patterns))
        .filter(|src| matches_cli_filter(src, &opts.name, &opts.filter))
        .collect();

    Ok(filtered)
}

/// Check if a test source matches the configured patterns using glob.
/// Patterns containing glob metacharacters (`*`, `?`, `[`) are treated as globs.
/// Plain strings without metacharacters are treated as substring matches.
fn matches_test_patterns(
    source: &Path,
    test_patterns: &[String],
    exclude_patterns: &[String],
) -> bool {
    let source_str = source.to_string_lossy();
    let filename = source.file_name().and_then(|s| s.to_str()).unwrap_or("");

    // Check exclude patterns first
    for pattern in exclude_patterns {
        if pattern_matches(pattern, filename, &source_str) {
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
        .any(|pattern| pattern_matches(pattern, filename, &source_str))
}

/// Match a pattern against a filename/path. Uses glob if the pattern contains
/// metacharacters (`*`, `?`, `[`), otherwise uses substring matching.
fn pattern_matches(pattern: &str, filename: &str, source_str: &str) -> bool {
    if is_glob_pattern(pattern) {
        if let Ok(glob_pat) = glob::Pattern::new(pattern) {
            return glob_pat.matches(filename) || glob_pat.matches(source_str);
        }
    }
    // Substring fallback
    filename.contains(pattern) || source_str.contains(pattern)
}

fn is_glob_pattern(s: &str) -> bool {
    s.contains('*') || s.contains('?') || s.contains('[')
}

/// Apply CLI name/filter arguments as an additional filter layer.
fn matches_cli_filter(source: &Path, name: &Option<String>, filter: &Option<String>) -> bool {
    let filename = source.file_name().and_then(|s| s.to_str()).unwrap_or("");
    let stem = source.file_stem().and_then(|s| s.to_str()).unwrap_or("");

    // Positional name: substring match on stem
    if let Some(ref n) = name {
        if !stem.contains(n.as_str()) {
            return false;
        }
    }

    // --filter: glob if metacharacters present, else substring
    if let Some(ref f) = filter {
        if is_glob_pattern(f) {
            if let Ok(glob_pat) = glob::Pattern::new(f) {
                if !glob_pat.matches(filename) && !glob_pat.matches(stem) {
                    return false;
                }
            } else if !filename.contains(f.as_str()) && !stem.contains(f.as_str()) {
                return false;
            }
        } else if !filename.contains(f.as_str()) && !stem.contains(f.as_str()) {
            return false;
        }
    }

    true
}

// ---------------------------------------------------------------------------
// Test compilation
// ---------------------------------------------------------------------------

fn compile_tests(
    config: &Config,
    test_sources: &[PathBuf],
    shell: &Shell,
    opts: &TestOptions,
) -> Result<Vec<CompiledTest>, CmodError> {
    let build_dir = config.build_dir();
    let pcm_dir = build_dir.join("pcm");
    let obj_dir = build_dir.join("obj");

    let cxx_standard = config
        .manifest
        .toolchain
        .as_ref()
        .and_then(|tc| tc.cxx_standard.clone())
        .unwrap_or_else(|| "20".to_string());

    let target_triple = resolve_target_triple(config);

    // Collect PCM and object files from the build
    let pcm_flags = collect_pcm_flags(config, &pcm_dir);
    let obj_files = collect_obj_files(config, &obj_dir);

    // Framework flags
    let framework = config
        .manifest
        .test
        .as_ref()
        .and_then(|t| t.framework.clone());
    let framework_flags = build_framework_flags(&framework, config);

    // Extra test flags from manifest
    let extra_flags: Vec<String> = config
        .manifest
        .test
        .as_ref()
        .map(|t| t.extra_flags.clone())
        .unwrap_or_default();

    // Sanitizer flags
    let sanitizer_flags = build_sanitizer_flags(&opts.sanitize);

    // Coverage flags
    let coverage_flags: Vec<String> = if opts.coverage {
        vec![
            "-fprofile-instr-generate".to_string(),
            "-fcoverage-mapping".to_string(),
        ]
    } else {
        vec![]
    };

    let mut compiled = Vec::new();
    let mut compile_failures = Vec::new();

    let clang_path = std::env::var_os("CXX").unwrap_or_else(|| std::ffi::OsString::from("clang++"));

    for test_source in test_sources {
        let test_name = test_source
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("test")
            .to_string();

        let test_binary = build_dir.join(format!("test_{}", test_name));

        let mut cmd = std::process::Command::new(&clang_path);
        cmd.arg(format!("-std=c++{}", cxx_standard));
        cmd.arg(format!("--target={}", target_triple));

        for flag in &pcm_flags {
            cmd.arg(flag);
        }

        // Framework flags
        for flag in &framework_flags {
            cmd.arg(flag);
        }

        // Extra flags from manifest
        for flag in &extra_flags {
            cmd.arg(flag);
        }

        // Sanitizer flags
        for flag in &sanitizer_flags {
            cmd.arg(flag);
        }

        // Coverage flags
        for flag in &coverage_flags {
            cmd.arg(flag);
        }

        cmd.arg("-o").arg(&test_binary).arg(test_source);

        for obj in &obj_files {
            cmd.arg(obj);
        }

        shell.verbose("Compiling", format!("test: {}", test_name));

        let output = cmd.output().map_err(|e| CmodError::TestFailed {
            reason: format!("failed to compile test '{}': {}", test_name, e),
        })?;

        if output.status.success() {
            compiled.push(CompiledTest {
                name: test_name,
                source: test_source.clone(),
                binary_path: test_binary,
            });
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            shell.error(format!("compilation failed: {}", test_name));
            if !stderr.is_empty() {
                shell.error(&stderr);
            }
            compile_failures.push(test_name);
        }
    }

    if !compile_failures.is_empty() {
        shell.warn(format!(
            "{} test(s) failed to compile: {}",
            compile_failures.len(),
            compile_failures.join(", ")
        ));
    }

    Ok(compiled)
}

fn resolve_target_triple(config: &Config) -> String {
    config
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
        })
}

fn collect_pcm_flags(config: &Config, pcm_dir: &Path) -> Vec<String> {
    let mut pcm_flags = Vec::new();
    if !pcm_dir.exists() {
        return pcm_flags;
    }

    let src_dirs = config.src_dirs();
    let exclude = config.exclude_patterns();
    let mut name_map: std::collections::HashMap<String, String> = std::collections::HashMap::new();
    if let Ok(sources) = cmod_build::runner::discover_sources_multi(&src_dirs, &exclude) {
        for source in &sources {
            if let Ok(Some(mod_name)) = cmod_build::runner::extract_module_name(source) {
                let sanitized = mod_name.replace(['.', ':', '/'], "_");
                name_map.insert(sanitized, mod_name);
            }
        }
    }

    if let Ok(entries) = std::fs::read_dir(pcm_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("pcm") {
                if let Some(stem) = path.file_stem().and_then(|s| s.to_str()) {
                    let module_name = name_map
                        .get(stem)
                        .cloned()
                        .unwrap_or_else(|| stem.to_string());
                    pcm_flags.push(format!("-fmodule-file={}={}", module_name, path.display()));
                }
            }
        }
    }

    pcm_flags
}

fn collect_obj_files(config: &Config, obj_dir: &Path) -> Vec<String> {
    let mut obj_files = Vec::new();
    if !obj_dir.exists() {
        return obj_files;
    }

    // Identify main object stems to skip
    let mut main_obj_stems: std::collections::HashSet<String> = std::collections::HashSet::new();
    main_obj_stems.insert("main".to_string());

    let src_dirs = config.src_dirs();
    let exclude = config.exclude_patterns();
    if let Ok(sources) = cmod_build::runner::discover_sources_multi(&src_dirs, &exclude) {
        for source in &sources {
            let stem = source.file_stem().and_then(|s| s.to_str()).unwrap_or("");
            if stem == "main" {
                let sanitized = source.display().to_string().replace(['.', ':', '/'], "_");
                main_obj_stems.insert(sanitized);
            }
        }
    }

    if let Ok(entries) = std::fs::read_dir(obj_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().and_then(|e| e.to_str()) == Some("o") {
                let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                if main_obj_stems.contains(stem) {
                    continue;
                }
                obj_files.push(path.display().to_string());
            }
        }
    }

    obj_files
}

// ---------------------------------------------------------------------------
// Framework support
// ---------------------------------------------------------------------------

fn build_framework_flags(framework: &Option<String>, config: &Config) -> Vec<String> {
    let mut flags = Vec::new();
    let fw = match framework {
        Some(f) => f.to_lowercase(),
        None => return flags,
    };

    let deps_dir = config.deps_dir();

    match fw.as_str() {
        "catch2" => {
            // Look for Catch2 in dev-dependencies checkout
            if let Some(catch2_path) = find_framework_dir(&deps_dir, "catch2") {
                // Catch2 v3
                let v3_include = catch2_path.join("src");
                if v3_include.exists() {
                    flags.push(format!("-isystem{}", v3_include.display()));
                }
                // Catch2 v2 fallback
                let v2_include = catch2_path.join("single_include");
                if v2_include.exists() {
                    flags.push(format!("-isystem{}", v2_include.display()));
                }
            }
        }
        "gtest" | "googletest" => {
            if let Some(gtest_path) = find_framework_dir(&deps_dir, "googletest") {
                let include = gtest_path.join("googletest").join("include");
                if include.exists() {
                    flags.push(format!("-isystem{}", include.display()));
                }
                flags.extend([
                    "-lgtest".to_string(),
                    "-lgtest_main".to_string(),
                    "-lpthread".to_string(),
                ]);
            }
        }
        _ => {} // "custom" or unknown — only extra_flags from manifest
    }

    flags
}

fn find_framework_dir(deps_dir: &Path, name: &str) -> Option<PathBuf> {
    if !deps_dir.exists() {
        return None;
    }
    let name_lower = name.to_lowercase();
    if let Ok(entries) = std::fs::read_dir(deps_dir) {
        for entry in entries.flatten() {
            let entry_name = entry.file_name().to_string_lossy().to_lowercase();
            if entry_name.contains(&name_lower) && entry.path().is_dir() {
                return Some(entry.path());
            }
        }
    }
    None
}

// ---------------------------------------------------------------------------
// Sanitizer support
// ---------------------------------------------------------------------------

fn build_sanitizer_flags(sanitizers: &[String]) -> Vec<String> {
    let mut flags = Vec::new();
    for san in sanitizers {
        match san.as_str() {
            "address" | "undefined" | "thread" | "memory" => {
                flags.push(format!("-fsanitize={}", san));
            }
            other => {
                // Pass through unknown sanitizer names
                flags.push(format!("-fsanitize={}", other));
            }
        }
    }
    flags
}

fn sanitizer_env_vars(sanitizers: &[String]) -> Vec<(String, String)> {
    let mut env_vars = Vec::new();
    for san in sanitizers {
        match san.as_str() {
            "address" => {
                env_vars.push((
                    "ASAN_OPTIONS".to_string(),
                    "detect_leaks=1:print_stacktrace=1".to_string(),
                ));
            }
            "undefined" => {
                env_vars.push((
                    "UBSAN_OPTIONS".to_string(),
                    "print_stacktrace=1:halt_on_error=1".to_string(),
                ));
            }
            "thread" => {
                env_vars.push((
                    "TSAN_OPTIONS".to_string(),
                    "second_deadlock_stack=1".to_string(),
                ));
            }
            "memory" => {
                env_vars.push(("MSAN_OPTIONS".to_string(), "print_stacktrace=1".to_string()));
            }
            _ => {}
        }
    }
    env_vars
}

// ---------------------------------------------------------------------------
// Test execution (parallel)
// ---------------------------------------------------------------------------

fn execute_tests(
    tests: &[CompiledTest],
    opts: &TestOptions,
    config: &Config,
    shell: &Shell,
) -> TestSummary {
    let timeout_secs = if opts.timeout > 0 {
        opts.timeout
    } else {
        config
            .manifest
            .test
            .as_ref()
            .and_then(|t| t.timeout)
            .unwrap_or(0)
    };

    let custom_runner = config.manifest.test.as_ref().and_then(|t| t.runner.clone());

    let san_env = sanitizer_env_vars(&opts.sanitize);

    let effective_jobs = if opts.jobs == 0 {
        std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(4)
    } else {
        opts.jobs
    };

    // Sequential path for jobs == 1 or single test
    if effective_jobs == 1 || tests.len() == 1 {
        return execute_tests_sequential(
            tests,
            timeout_secs,
            opts.no_fail_fast,
            &custom_runner,
            &san_env,
            shell,
        );
    }

    execute_tests_parallel(
        tests,
        effective_jobs,
        timeout_secs,
        opts.no_fail_fast,
        &custom_runner,
        &san_env,
        shell,
    )
}

fn execute_tests_sequential(
    tests: &[CompiledTest],
    timeout_secs: u64,
    no_fail_fast: bool,
    custom_runner: &Option<String>,
    san_env: &[(String, String)],
    shell: &Shell,
) -> TestSummary {
    let mut summary = TestSummary::new();
    let start = Instant::now();

    for test in tests {
        let result = execute_single_test(test, timeout_secs, custom_runner, san_env);
        print_test_result_line(&result, shell);
        update_summary(&mut summary, result);

        if !no_fail_fast && !summary.is_success() {
            break;
        }
    }

    summary.total_duration = start.elapsed();
    summary
}

fn execute_tests_parallel(
    tests: &[CompiledTest],
    jobs: usize,
    timeout_secs: u64,
    no_fail_fast: bool,
    custom_runner: &Option<String>,
    san_env: &[(String, String)],
    shell: &Shell,
) -> TestSummary {
    let num_workers = jobs.min(tests.len());
    let (sender, receiver) = crossbeam_channel::bounded::<usize>(tests.len());
    let failed_flag = AtomicBool::new(false);
    let start = Instant::now();

    // Enqueue all test indices
    for i in 0..tests.len() {
        let _ = sender.send(i);
    }
    drop(sender);

    let results: std::sync::Mutex<Vec<TestResult>> =
        std::sync::Mutex::new(Vec::with_capacity(tests.len()));

    std::thread::scope(|scope| {
        for _ in 0..num_workers {
            let receiver = &receiver;
            let failed_flag = &failed_flag;
            let results = &results;

            scope.spawn(move || {
                while let Ok(idx) = receiver.recv() {
                    if !no_fail_fast && failed_flag.load(Ordering::Relaxed) {
                        break;
                    }

                    let result =
                        execute_single_test(&tests[idx], timeout_secs, custom_runner, san_env);

                    match result.status {
                        TestStatus::Failed { .. } | TestStatus::TimedOut => {
                            failed_flag.store(true, Ordering::Relaxed);
                        }
                        _ => {}
                    }

                    results.lock().unwrap().push(result);
                }
            });
        }
    });

    let collected = results.into_inner().unwrap();
    let mut summary = TestSummary::new();
    for result in collected {
        print_test_result_line(&result, shell);
        update_summary(&mut summary, result);
    }
    summary.total_duration = start.elapsed();
    summary
}

fn execute_single_test(
    test: &CompiledTest,
    timeout_secs: u64,
    custom_runner: &Option<String>,
    san_env: &[(String, String)],
) -> TestResult {
    let start = Instant::now();

    let mut cmd = if let Some(ref runner) = custom_runner {
        let mut c = std::process::Command::new(runner);
        c.arg(&test.binary_path);
        c
    } else {
        std::process::Command::new(&test.binary_path)
    };

    cmd.stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());

    // Set sanitizer environment variables
    for (key, value) in san_env {
        cmd.env(key, value);
    }

    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            return TestResult {
                name: test.name.clone(),
                status: TestStatus::CompileFailed {
                    reason: format!("failed to execute: {}", e),
                },
                duration: start.elapsed(),
                stdout: String::new(),
                stderr: e.to_string(),
                source: test.source.clone(),
            };
        }
    };

    if timeout_secs > 0 {
        use wait_timeout::ChildExt;
        let timeout_dur = Duration::from_secs(timeout_secs);
        match child.wait_timeout(timeout_dur) {
            Ok(Some(status)) => {
                // Completed within timeout
                let stdout = child
                    .stdout
                    .take()
                    .map(|mut s| {
                        let mut buf = String::new();
                        std::io::Read::read_to_string(&mut s, &mut buf).ok();
                        buf
                    })
                    .unwrap_or_default();
                let stderr = child
                    .stderr
                    .take()
                    .map(|mut s| {
                        let mut buf = String::new();
                        std::io::Read::read_to_string(&mut s, &mut buf).ok();
                        buf
                    })
                    .unwrap_or_default();

                let test_status = if status.success() {
                    TestStatus::Passed
                } else {
                    TestStatus::Failed {
                        exit_code: status.code(),
                    }
                };

                TestResult {
                    name: test.name.clone(),
                    status: test_status,
                    duration: start.elapsed(),
                    stdout,
                    stderr,
                    source: test.source.clone(),
                }
            }
            Ok(None) => {
                // Timed out
                let _ = child.kill();
                let _ = child.wait();
                TestResult {
                    name: test.name.clone(),
                    status: TestStatus::TimedOut,
                    duration: start.elapsed(),
                    stdout: String::new(),
                    stderr: String::new(),
                    source: test.source.clone(),
                }
            }
            Err(e) => TestResult {
                name: test.name.clone(),
                status: TestStatus::Failed { exit_code: None },
                duration: start.elapsed(),
                stdout: String::new(),
                stderr: e.to_string(),
                source: test.source.clone(),
            },
        }
    } else {
        // No timeout
        match child.wait_with_output() {
            Ok(output) => {
                let test_status = if output.status.success() {
                    TestStatus::Passed
                } else {
                    TestStatus::Failed {
                        exit_code: output.status.code(),
                    }
                };
                TestResult {
                    name: test.name.clone(),
                    status: test_status,
                    duration: start.elapsed(),
                    stdout: String::from_utf8_lossy(&output.stdout).to_string(),
                    stderr: String::from_utf8_lossy(&output.stderr).to_string(),
                    source: test.source.clone(),
                }
            }
            Err(e) => TestResult {
                name: test.name.clone(),
                status: TestStatus::Failed { exit_code: None },
                duration: start.elapsed(),
                stdout: String::new(),
                stderr: e.to_string(),
                source: test.source.clone(),
            },
        }
    }
}

fn update_summary(summary: &mut TestSummary, result: TestResult) {
    match &result.status {
        TestStatus::Passed => summary.passed += 1,
        TestStatus::Failed { .. } => summary.failed += 1,
        TestStatus::TimedOut => summary.timed_out += 1,
        TestStatus::CompileFailed { .. } => summary.failed += 1,
        TestStatus::Skipped => summary.skipped += 1,
    }
    summary.results.push(result);
}

fn print_test_result_line(result: &TestResult, shell: &Shell) {
    let duration_str = format!("{:.2}s", result.duration.as_secs_f64());
    match &result.status {
        TestStatus::Passed => {
            shell.status(
                "Passed",
                format!("test: {} ({})", result.name, duration_str),
            );
        }
        TestStatus::Failed { exit_code } => {
            let code_str = exit_code
                .map(|c| format!(" (exit code {})", c))
                .unwrap_or_default();
            shell.status_with_color(
                "FAILED",
                format!("test: {}{} ({})", result.name, code_str, duration_str),
                &cmod_core::shell::ERROR,
            );
            if !result.stderr.is_empty() {
                // Show first 20 lines of stderr
                let lines: Vec<&str> = result.stderr.lines().take(20).collect();
                for line in &lines {
                    shell.error(format!("  {}", line));
                }
                let total_lines = result.stderr.lines().count();
                if total_lines > 20 {
                    shell.error(format!("  ... ({} more lines)", total_lines - 20));
                }
            }
        }
        TestStatus::TimedOut => {
            shell.status_with_color(
                "TIMEOUT",
                format!("test: {} ({})", result.name, duration_str),
                &cmod_core::shell::WARN,
            );
        }
        TestStatus::CompileFailed { reason } => {
            shell.status_with_color(
                "COMPILE",
                format!("test: {} — {}", result.name, reason),
                &cmod_core::shell::ERROR,
            );
        }
        TestStatus::Skipped => {
            shell.status("Skipped", format!("test: {}", result.name));
        }
    }
}

// ---------------------------------------------------------------------------
// Workspace testing
// ---------------------------------------------------------------------------

fn test_workspace(config: &Config, shell: &Shell, opts: &TestOptions) -> Result<(), CmodError> {
    let ws = WorkspaceManager::load(&config.root)?;

    // Validate --package if given
    if let Some(ref pkg) = opts.package {
        if !ws.members.iter().any(|m| m.name == *pkg) {
            return Err(CmodError::TestFailed {
                reason: format!(
                    "workspace member '{}' not found. Available members: {}",
                    pkg,
                    ws.members
                        .iter()
                        .map(|m| m.name.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            });
        }
    }

    let ordered_members = ws.build_order()?;

    let mut overall_summary = TestSummary::new();
    let mut member_summaries: Vec<(String, TestSummary)> = Vec::new();

    for member in &ordered_members {
        // Skip members not matching --package filter
        if let Some(ref pkg) = opts.package {
            if member.name != *pkg {
                continue;
            }
        }

        shell.status("Testing", format!("member: {}", member.name));

        let member_test_dir = member.path.join("tests");
        if !member_test_dir.exists() {
            shell.verbose("Skipping", format!("{} (no tests directory)", member.name));
            continue;
        }

        // Create a member-scoped config
        let member_config = super::util::create_member_config(config, member)?;

        match test_single_project(&member_config, shell, opts) {
            Ok(summary) => {
                member_summaries.push((member.name.clone(), summary));
            }
            Err(CmodError::TestsFailed { count }) => {
                let mut summary = TestSummary::new();
                summary.failed = count;
                member_summaries.push((member.name.clone(), summary));
                if !opts.no_fail_fast {
                    break;
                }
            }
            Err(e) => {
                shell.error(format!("{}: {}", member.name, e));
                if !opts.no_fail_fast {
                    return Err(e);
                }
            }
        }
    }

    // Print workspace summary
    if !member_summaries.is_empty() {
        shell.status("", "");
        shell.status("Summary", "workspace test results:");
        for (name, summary) in &member_summaries {
            let status_str = if summary.is_success() { "ok" } else { "FAILED" };
            shell.status(
                "",
                format!(
                    "  {}: {} passed, {} failed ({:.2}s) [{}]",
                    name,
                    summary.passed,
                    summary.failed + summary.timed_out,
                    summary.total_duration.as_secs_f64(),
                    status_str,
                ),
            );
        }

        for (_, summary) in member_summaries {
            overall_summary.merge(summary);
        }

        let total_status = if overall_summary.is_success() {
            "ok"
        } else {
            "FAILED"
        };
        shell.status(
            "Total",
            format!(
                "{} passed, {} failed, {} skipped; finished in {:.2}s [{}]",
                overall_summary.passed,
                overall_summary.failed + overall_summary.timed_out,
                overall_summary.skipped,
                overall_summary.total_duration.as_secs_f64(),
                total_status,
            ),
        );
    }

    if overall_summary.is_success() {
        Ok(())
    } else {
        Err(CmodError::TestsFailed {
            count: overall_summary.failed + overall_summary.timed_out,
        })
    }
}

// ---------------------------------------------------------------------------
// Coverage report
// ---------------------------------------------------------------------------

fn generate_coverage_report(config: &Config, tests: &[CompiledTest], shell: &Shell) {
    let build_dir = config.build_dir();
    let coverage_dir = build_dir.join("coverage");
    let _ = std::fs::create_dir_all(&coverage_dir);

    // Find profraw files
    let profraw_pattern = format!("{}/*.profraw", build_dir.display());
    let profraw_files: Vec<PathBuf> = glob::glob(&profraw_pattern)
        .map(|paths| paths.filter_map(Result::ok).collect())
        .unwrap_or_default();

    if profraw_files.is_empty() {
        shell.verbose("Coverage", "no profraw files found");
        return;
    }

    let merged_profdata = coverage_dir.join("merged.profdata");

    // Merge profiles
    let mut merge_cmd = std::process::Command::new("llvm-profdata");
    merge_cmd
        .arg("merge")
        .arg("-sparse")
        .arg("-o")
        .arg(&merged_profdata);
    for f in &profraw_files {
        merge_cmd.arg(f);
    }

    match merge_cmd.status() {
        Ok(s) if s.success() => {
            shell.status("Coverage", "merged profdata");
        }
        _ => {
            shell.warn("coverage: failed to merge profdata (is llvm-profdata in PATH?)");
            return;
        }
    }

    // Generate report for first test binary
    if let Some(test) = tests.first() {
        let report_status = std::process::Command::new("llvm-cov")
            .arg("report")
            .arg(&test.binary_path)
            .arg(format!("-instr-profile={}", merged_profdata.display()))
            .status();

        match report_status {
            Ok(s) if s.success() => {}
            _ => {
                shell.warn("coverage: failed to generate report (is llvm-cov in PATH?)");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Output formatting
// ---------------------------------------------------------------------------

fn format_results(summary: &TestSummary, shell: &Shell, opts: &TestOptions) {
    let format = opts.format.as_deref().unwrap_or("human");

    match format {
        "json" => print_json_results(summary),
        "junit" => print_junit_results(summary),
        "tap" => print_tap_results(summary),
        _ => print_human_results(summary, shell),
    }
}

fn print_human_results(summary: &TestSummary, shell: &Shell) {
    let status_word = if summary.is_success() { "ok" } else { "FAILED" };

    shell.status(
        "Result",
        format!(
            "test result: {}. {} passed, {} failed, {} skipped; finished in {:.2}s",
            status_word,
            summary.passed,
            summary.failed + summary.timed_out,
            summary.skipped,
            summary.total_duration.as_secs_f64(),
        ),
    );
}

fn print_json_results(summary: &TestSummary) {
    let tests_json: Vec<serde_json::Value> = summary
        .results
        .iter()
        .map(|r| {
            let (status_str, exit_code) = match &r.status {
                TestStatus::Passed => ("passed", None),
                TestStatus::Failed { exit_code } => ("failed", *exit_code),
                TestStatus::TimedOut => ("timed_out", None),
                TestStatus::CompileFailed { .. } => ("compile_failed", None),
                TestStatus::Skipped => ("skipped", None),
            };

            let mut obj = serde_json::json!({
                "name": r.name,
                "status": status_str,
                "duration_ms": r.duration.as_millis() as u64,
                "file": r.source.display().to_string(),
            });

            if let Some(code) = exit_code {
                obj["exit_code"] = serde_json::json!(code);
            }
            if !r.stderr.is_empty() {
                obj["stderr"] = serde_json::json!(r.stderr);
            }
            if !r.stdout.is_empty() {
                obj["stdout"] = serde_json::json!(r.stdout);
            }

            obj
        })
        .collect();

    let output = serde_json::json!({
        "tests": tests_json,
        "summary": {
            "passed": summary.passed,
            "failed": summary.failed + summary.timed_out,
            "skipped": summary.skipped,
            "timed_out": summary.timed_out,
            "duration_ms": summary.total_duration.as_millis() as u64,
        }
    });

    println!(
        "{}",
        serde_json::to_string_pretty(&output).unwrap_or_else(|_| "{}".to_string())
    );
}

fn print_junit_results(summary: &TestSummary) {
    let total = summary.total();
    let failures = summary.failed + summary.timed_out;
    let time = format!("{:.3}", summary.total_duration.as_secs_f64());

    println!(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    println!(
        r#"<testsuites tests="{}" failures="{}" time="{}">"#,
        total, failures, time
    );
    println!(r#"  <testsuite name="cmod" tests="{}">"#, total);

    for r in &summary.results {
        let test_time = format!("{:.3}", r.duration.as_secs_f64());
        match &r.status {
            TestStatus::Passed => {
                println!(
                    r#"    <testcase name="{}" time="{}"/>"#,
                    escape_xml(&r.name),
                    test_time
                );
            }
            TestStatus::Failed { exit_code } => {
                let msg = exit_code
                    .map(|c| format!("exit code {}", c))
                    .unwrap_or_else(|| "failed".to_string());
                println!(
                    r#"    <testcase name="{}" time="{}">"#,
                    escape_xml(&r.name),
                    test_time
                );
                println!(
                    r#"      <failure message="{}">{}</failure>"#,
                    escape_xml(&msg),
                    escape_xml(&r.stderr)
                );
                println!("    </testcase>");
            }
            TestStatus::TimedOut => {
                println!(
                    r#"    <testcase name="{}" time="{}">"#,
                    escape_xml(&r.name),
                    test_time
                );
                println!(r#"      <failure message="timed out"/>"#);
                println!("    </testcase>");
            }
            TestStatus::CompileFailed { reason } => {
                println!(
                    r#"    <testcase name="{}" time="{}">"#,
                    escape_xml(&r.name),
                    test_time
                );
                println!(
                    r#"      <failure message="compilation failed">{}</failure>"#,
                    escape_xml(reason)
                );
                println!("    </testcase>");
            }
            TestStatus::Skipped => {
                println!(
                    r#"    <testcase name="{}" time="{}">"#,
                    escape_xml(&r.name),
                    test_time
                );
                println!("      <skipped/>");
                println!("    </testcase>");
            }
        }
    }

    println!("  </testsuite>");
    println!("</testsuites>");
}

fn print_tap_results(summary: &TestSummary) {
    println!("TAP version 14");
    println!("1..{}", summary.total());

    for (i, r) in summary.results.iter().enumerate() {
        let num = i + 1;
        let duration_str = format!("{:.2}s", r.duration.as_secs_f64());
        match &r.status {
            TestStatus::Passed => {
                println!("ok {} - {} ({})", num, r.name, duration_str);
            }
            TestStatus::Failed { .. } | TestStatus::CompileFailed { .. } => {
                println!("not ok {} - {} ({})", num, r.name, duration_str);
            }
            TestStatus::TimedOut => {
                println!("not ok {} - {} (timed out)", num, r.name);
            }
            TestStatus::Skipped => {
                println!("ok {} - {} # SKIP", num, r.name);
            }
        }
    }
}

fn escape_xml(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_test_patterns_glob() {
        let source = Path::new("/project/tests/test_math.cpp");
        assert!(matches_test_patterns(
            source,
            &["test_*.cpp".to_string()],
            &[]
        ));
        assert!(!matches_test_patterns(
            source,
            &["bench_*.cpp".to_string()],
            &[]
        ));
        // Glob exclude
        assert!(!matches_test_patterns(
            source,
            &[],
            &["test_math*".to_string()]
        ));
    }

    #[test]
    fn test_matches_test_patterns_substring_fallback() {
        let source = Path::new("/project/tests/test_math.cpp");
        // Plain string without glob metacharacters uses substring matching
        assert!(matches_test_patterns(source, &["math".to_string()], &[]));
        assert!(!matches_test_patterns(
            source,
            &["physics".to_string()],
            &[]
        ));
        // Glob wildcard also works
        assert!(matches_test_patterns(source, &["*math*".to_string()], &[]));
    }

    #[test]
    fn test_matches_test_patterns_empty() {
        let source = Path::new("/project/tests/test_basic.cpp");
        // Empty patterns match everything
        assert!(matches_test_patterns(source, &[], &[]));
    }

    #[test]
    fn test_matches_test_patterns_exclude_takes_precedence() {
        let source = Path::new("/project/tests/test_math.cpp");
        // Include matches but exclude also matches — should be excluded
        assert!(!matches_test_patterns(
            source,
            &["test_*.cpp".to_string()],
            &["*math*".to_string()]
        ));
    }

    #[test]
    fn test_matches_cli_filter_glob() {
        let source = Path::new("/project/tests/test_math.cpp");
        assert!(matches_cli_filter(
            source,
            &None,
            &Some("test_m*".to_string())
        ));
        assert!(!matches_cli_filter(
            source,
            &None,
            &Some("bench_*".to_string())
        ));
    }

    #[test]
    fn test_matches_cli_filter_name_substring() {
        let source = Path::new("/project/tests/test_math.cpp");
        assert!(matches_cli_filter(source, &Some("math".to_string()), &None));
        assert!(!matches_cli_filter(
            source,
            &Some("physics".to_string()),
            &None
        ));
    }

    #[test]
    fn test_summary_merge() {
        let mut a = TestSummary::new();
        a.passed = 3;
        a.failed = 1;
        a.total_duration = Duration::from_millis(100);

        let mut b = TestSummary::new();
        b.passed = 2;
        b.timed_out = 1;
        b.total_duration = Duration::from_millis(200);

        a.merge(b);
        assert_eq!(a.passed, 5);
        assert_eq!(a.failed, 1);
        assert_eq!(a.timed_out, 1);
        assert_eq!(a.total_duration, Duration::from_millis(300));
        assert!(!a.is_success());
    }

    #[test]
    fn test_summary_success() {
        let mut s = TestSummary::new();
        s.passed = 5;
        assert!(s.is_success());
        s.failed = 1;
        assert!(!s.is_success());
    }

    #[test]
    fn test_status_display() {
        // TestStatus variants exist and have expected structure
        let passed = TestStatus::Passed;
        assert!(matches!(passed, TestStatus::Passed));

        let failed = TestStatus::Failed { exit_code: Some(1) };
        assert!(matches!(failed, TestStatus::Failed { exit_code: Some(1) }));

        let timed_out = TestStatus::TimedOut;
        assert!(matches!(timed_out, TestStatus::TimedOut));

        let compile_failed = TestStatus::CompileFailed {
            reason: "error".to_string(),
        };
        assert!(matches!(compile_failed, TestStatus::CompileFailed { .. }));

        let skipped = TestStatus::Skipped;
        assert!(matches!(skipped, TestStatus::Skipped));
    }

    #[test]
    fn test_escape_xml() {
        assert_eq!(escape_xml("a < b"), "a &lt; b");
        assert_eq!(escape_xml("a & b"), "a &amp; b");
        assert_eq!(escape_xml(r#"a "b""#), "a &quot;b&quot;");
    }

    #[test]
    fn test_build_sanitizer_flags() {
        let flags = build_sanitizer_flags(&["address".to_string(), "undefined".to_string()]);
        assert_eq!(flags, vec!["-fsanitize=address", "-fsanitize=undefined"]);
    }

    #[test]
    fn test_sanitizer_env_vars() {
        let env = sanitizer_env_vars(&["address".to_string()]);
        assert_eq!(env.len(), 1);
        assert_eq!(env[0].0, "ASAN_OPTIONS");
        assert!(env[0].1.contains("detect_leaks=1"));
    }
}
