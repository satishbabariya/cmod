//! End-to-end tests that compile the example projects using real clang++.
//!
//! These tests copy each example project to a temp directory, run `cmod build`,
//! and verify the output. They are automatically skipped when Homebrew LLVM
//! Clang is not available.
//!
//! Run all tests:
//!   cargo test --test example_projects
//!
//! Run with LLVM (ensures compilation tests execute):
//!   PATH="/opt/homebrew/opt/llvm/bin:$PATH" cargo test --test example_projects
//!
//! Run a specific test:
//!   cargo test --test example_projects -- test_example_hello

use std::path::{Path, PathBuf};
use std::process::Command;

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Path to the examples directory relative to the workspace root.
fn examples_dir() -> PathBuf {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    // cmod-cli is at crates/cmod-cli, so workspace root is ../../
    manifest_dir.join("../../examples").canonicalize().unwrap()
}

/// Copy an example project to a temp directory and return (TempDir, project_path).
fn copy_example(name: &str) -> (tempfile::TempDir, PathBuf) {
    let tmp = tempfile::TempDir::new().unwrap();
    let project_path = tmp.path().to_path_buf();
    let src = examples_dir().join(name);
    assert!(
        src.exists(),
        "example '{}' not found at {}",
        name,
        src.display()
    );
    copy_dir_recursive(&src, &project_path).unwrap();
    (tmp, project_path)
}

/// Recursively copy a directory tree.
fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());

        if src_path.is_dir() {
            std::fs::create_dir_all(&dst_path)?;
            copy_dir_recursive(&src_path, &dst_path)?;
        } else {
            std::fs::copy(&src_path, &dst_path)?;
        }
    }
    Ok(())
}

/// Run cmod with LLVM Clang on PATH.
fn run_cmod_with_llvm(dir: &Path, args: &[&str]) -> std::process::Output {
    let llvm_path = "/opt/homebrew/opt/llvm/bin";
    let current_path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", llvm_path, current_path);

    let mut full_args: Vec<&str> = args.to_vec();
    full_args.push("--no-cache");

    Command::new(env!("CARGO_BIN_EXE_cmod"))
        .args(&full_args)
        .current_dir(dir)
        .env("PATH", new_path)
        .output()
        .expect("failed to run cmod")
}

/// Check if Homebrew LLVM Clang is available.
fn has_llvm_clang() -> bool {
    let llvm_clang = Path::new("/opt/homebrew/opt/llvm/bin/clang++");
    if !llvm_clang.exists() {
        return false;
    }
    Command::new(llvm_clang)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

fn stdout(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

// ─── Tests ───────────────────────────────────────────────────────────────────

/// Build examples/hello — minimal binary with module interface + main.
#[test]
fn test_example_hello_build() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let (_tmp, dir) = copy_example("hello");

    let output = run_cmod_with_llvm(&dir, &["build"]);
    let err = stderr(&output);
    eprintln!("--- hello build stderr ---\n{}", err);

    assert!(
        output.status.success(),
        "cmod build failed for examples/hello:\nstdout: {}\nstderr: {}",
        stdout(&output),
        err,
    );

    // Should produce a binary in build/
    let build_dir = dir.join("build");
    assert!(build_dir.exists(), "build directory should exist");
}

/// Run examples/hello — verify it produces expected output.
#[test]
fn test_example_hello_run() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let (_tmp, dir) = copy_example("hello");

    let output = run_cmod_with_llvm(&dir, &["run"]);
    let err = stderr(&output);
    let out = stdout(&output);
    eprintln!("--- hello run stderr ---\n{}", err);
    eprintln!("--- hello run stdout ---\n{}", out);

    assert!(
        output.status.success(),
        "cmod run failed for examples/hello:\nstdout: {}\nstderr: {}",
        out,
        err,
    );

    assert!(
        out.contains("Hello, world!"),
        "expected 'Hello, world!' in output, got: {}",
        out,
    );
}

/// Build examples/library — static lib with module partitions (:ops, :stats).
#[test]
fn test_example_library_build() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let (_tmp, dir) = copy_example("library");

    let output = run_cmod_with_llvm(&dir, &["build"]);
    let err = stderr(&output);
    eprintln!("--- library build stderr ---\n{}", err);

    assert!(
        output.status.success(),
        "cmod build failed for examples/library:\nstdout: {}\nstderr: {}",
        stdout(&output),
        err,
    );

    // Should produce a static library
    let build_dir = dir.join("build");
    assert!(build_dir.exists(), "build directory should exist");
}

/// Build examples/library tests — compile and run the test binary.
#[test]
fn test_example_library_test() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let (_tmp, dir) = copy_example("library");

    let output = run_cmod_with_llvm(&dir, &["test"]);
    let err = stderr(&output);
    eprintln!("--- library test stderr ---\n{}", err);

    assert!(
        output.status.success(),
        "cmod test failed for examples/library:\nstdout: {}\nstderr: {}",
        stdout(&output),
        err,
    );
}

/// Build examples/path-deps — binary with local path dependencies.
#[test]
fn test_example_path_deps_build() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let (_tmp, dir) = copy_example("path-deps");

    let output = run_cmod_with_llvm(&dir, &["build"]);
    let err = stderr(&output);
    eprintln!("--- path-deps build stderr ---\n{}", err);

    assert!(
        output.status.success(),
        "cmod build failed for examples/path-deps:\nstdout: {}\nstderr: {}",
        stdout(&output),
        err,
    );
}

/// Build examples/workspace — multi-member monorepo (core → utils → app).
#[test]
fn test_example_workspace_build() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let (_tmp, dir) = copy_example("workspace");

    let output = run_cmod_with_llvm(&dir, &["build"]);
    let err = stderr(&output);
    eprintln!("--- workspace build stderr ---\n{}", err);

    assert!(
        output.status.success(),
        "cmod build failed for examples/workspace:\nstdout: {}\nstderr: {}",
        stdout(&output),
        err,
    );
}

/// Verify `cmod graph` works on example projects (no clang needed).
#[test]
fn test_example_hello_graph() {
    let (_tmp, dir) = copy_example("hello");

    let output = run_cmod_with_llvm(&dir, &["graph"]);
    let err = stderr(&output);
    eprintln!("--- hello graph stderr ---\n{}", err);

    assert!(
        output.status.success(),
        "cmod graph failed:\nstdout: {}\nstderr: {}",
        stdout(&output),
        err,
    );
}

/// Verify `cmod compile-commands` generates a valid JSON for the hello example.
#[test]
fn test_example_hello_compile_commands() {
    let (_tmp, dir) = copy_example("hello");

    let output = run_cmod_with_llvm(&dir, &["compile-commands"]);
    let err = stderr(&output);
    eprintln!("--- hello compile-commands stderr ---\n{}", err);

    assert!(
        output.status.success(),
        "cmod compile-commands failed:\nstdout: {}\nstderr: {}",
        stdout(&output),
        err,
    );

    let cc_path = dir.join("compile_commands.json");
    assert!(
        cc_path.exists(),
        "compile_commands.json should be generated"
    );

    let content = std::fs::read_to_string(&cc_path).unwrap();
    let parsed: serde_json::Value = serde_json::from_str(&content).unwrap();
    assert!(
        parsed.is_array(),
        "compile_commands.json should be an array"
    );
    assert!(
        !parsed.as_array().unwrap().is_empty(),
        "compile_commands.json should not be empty"
    );
}

/// Verify `cmod graph --format json` works on the library example with partitions.
#[test]
fn test_example_library_graph_json() {
    let (_tmp, dir) = copy_example("library");

    let output = run_cmod_with_llvm(&dir, &["graph", "--format", "json"]);
    let out = stdout(&output);
    let err = stderr(&output);
    eprintln!("--- library graph json stderr ---\n{}", err);

    assert!(
        output.status.success(),
        "cmod graph --format json failed:\nstdout: {}\nstderr: {}",
        out,
        err,
    );

    // Should be valid JSON with module entries
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    assert!(parsed.is_object(), "graph JSON should be an object");
}
