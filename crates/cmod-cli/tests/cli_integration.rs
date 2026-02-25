use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

/// Run cmod in a given directory with the given arguments.
fn run_cmod(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_cmod"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to run cmod")
}

#[test]
fn test_init_creates_project() {
    let tmp = TempDir::new().unwrap();

    let output = run_cmod(tmp.path(), &["init", "--name", "hello"]);
    assert!(output.status.success(), "init failed: {:?}", output);

    // Check files were created
    assert!(tmp.path().join("cmod.toml").exists());
    assert!(tmp.path().join("src/lib.cppm").exists());
    assert!(tmp.path().join("tests/main.cpp").exists());

    // Check manifest content
    let content = fs::read_to_string(tmp.path().join("cmod.toml")).unwrap();
    assert!(content.contains("name = \"hello\""));
    assert!(content.contains("version = \"0.1.0\""));
}

#[test]
fn test_init_workspace() {
    let tmp = TempDir::new().unwrap();

    let output = run_cmod(tmp.path(), &["init", "--workspace", "--name", "engine"]);
    assert!(output.status.success(), "init --workspace failed: {:?}", output);

    let content = fs::read_to_string(tmp.path().join("cmod.toml")).unwrap();
    assert!(content.contains("name = \"engine\""));
    assert!(content.contains("[workspace]"));
}

#[test]
fn test_init_fails_if_already_exists() {
    let tmp = TempDir::new().unwrap();

    // First init should succeed
    let output = run_cmod(tmp.path(), &["init", "--name", "test"]);
    assert!(output.status.success());

    // Second init should fail
    let output = run_cmod(tmp.path(), &["init", "--name", "test"]);
    assert!(!output.status.success());
}

#[test]
fn test_add_and_remove_dependency() {
    let tmp = TempDir::new().unwrap();

    // Init project
    run_cmod(tmp.path(), &["init", "--name", "mymod"]);

    // Add a path dependency (won't need network)
    let dep_dir = tmp.path().join("libs/mylib");
    fs::create_dir_all(&dep_dir).unwrap();
    fs::write(
        dep_dir.join("cmod.toml"),
        "[package]\nname = \"mylib\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();

    let output = run_cmod(
        tmp.path(),
        &["add", "mylib", "--path", "./libs/mylib"],
    );
    assert!(output.status.success(), "add failed: {:?}", String::from_utf8_lossy(&output.stderr));

    // Verify the dependency was added to cmod.toml
    let content = fs::read_to_string(tmp.path().join("cmod.toml")).unwrap();
    assert!(content.contains("mylib"));

    // Remove the dependency
    let output = run_cmod(tmp.path(), &["remove", "mylib"]);
    assert!(output.status.success(), "remove failed: {:?}", String::from_utf8_lossy(&output.stderr));

    // Verify it was removed
    let content = fs::read_to_string(tmp.path().join("cmod.toml")).unwrap();
    assert!(!content.contains("mylib"));
}

#[test]
fn test_remove_nonexistent_dep_fails() {
    let tmp = TempDir::new().unwrap();
    run_cmod(tmp.path(), &["init", "--name", "test"]);

    let output = run_cmod(tmp.path(), &["remove", "nonexistent"]);
    assert!(!output.status.success());
}

#[test]
fn test_resolve_no_deps() {
    let tmp = TempDir::new().unwrap();
    run_cmod(tmp.path(), &["init", "--name", "test"]);

    let output = run_cmod(tmp.path(), &["resolve"]);
    assert!(output.status.success(), "resolve failed: {:?}", String::from_utf8_lossy(&output.stderr));

    // Should create a lockfile (even if empty)
    assert!(tmp.path().join("cmod.lock").exists());
}

#[test]
fn test_deps_shows_empty() {
    let tmp = TempDir::new().unwrap();
    run_cmod(tmp.path(), &["init", "--name", "test"]);

    // Create lockfile first
    run_cmod(tmp.path(), &["resolve"]);

    let output = run_cmod(tmp.path(), &["deps"]);
    assert!(output.status.success());
}

#[test]
fn test_verify_passes_for_valid_project() {
    let tmp = TempDir::new().unwrap();
    run_cmod(tmp.path(), &["init", "--name", "test"]);

    let output = run_cmod(tmp.path(), &["verify", "--verbose"]);
    assert!(output.status.success(), "verify failed: {:?}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn test_cache_status() {
    let tmp = TempDir::new().unwrap();
    run_cmod(tmp.path(), &["init", "--name", "test"]);

    let output = run_cmod(tmp.path(), &["cache", "status"]);
    assert!(output.status.success());
}

#[test]
fn test_cache_clean() {
    let tmp = TempDir::new().unwrap();
    run_cmod(tmp.path(), &["init", "--name", "test"]);

    let output = run_cmod(tmp.path(), &["cache", "clean"]);
    assert!(output.status.success());
}

#[test]
fn test_help_flag() {
    let tmp = TempDir::new().unwrap();
    let output = run_cmod(tmp.path(), &["--help"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("cmod"));
    assert!(stdout.contains("init"));
    assert!(stdout.contains("build"));
}

#[test]
fn test_init_resolve_verify_workflow() {
    let tmp = TempDir::new().unwrap();

    // Full workflow: init → resolve → verify
    let output = run_cmod(tmp.path(), &["init", "--name", "workflow_test"]);
    assert!(output.status.success());

    let output = run_cmod(tmp.path(), &["resolve"]);
    assert!(output.status.success());

    let output = run_cmod(tmp.path(), &["verify"]);
    assert!(output.status.success());
}
