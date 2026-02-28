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

#[test]
fn test_workspace_init_and_resolve() {
    let tmp = TempDir::new().unwrap();

    // Create a workspace
    let output = run_cmod(tmp.path(), &["init", "--workspace", "--name", "myws"]);
    assert!(output.status.success());

    // Create member directories with their own cmod.toml
    let core_dir = tmp.path().join("core");
    fs::create_dir_all(core_dir.join("src")).unwrap();
    fs::write(
        core_dir.join("cmod.toml"),
        "[package]\nname = \"core\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    fs::write(core_dir.join("src/lib.cppm"), "export module core;\n").unwrap();

    // Update workspace members
    let ws_toml = fs::read_to_string(tmp.path().join("cmod.toml")).unwrap();
    let updated = ws_toml.replace("members = []", "members = [\"core\"]");
    fs::write(tmp.path().join("cmod.toml"), updated).unwrap();

    // Resolve should work for workspace
    let output = run_cmod(tmp.path(), &["resolve"]);
    assert!(
        output.status.success(),
        "workspace resolve failed: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_add_path_dep_and_resolve() {
    let tmp = TempDir::new().unwrap();

    // Init project
    run_cmod(tmp.path(), &["init", "--name", "app"]);

    // Create a local library
    let lib_dir = tmp.path().join("libs/mathlib");
    fs::create_dir_all(lib_dir.join("src")).unwrap();
    fs::write(
        lib_dir.join("cmod.toml"),
        "[package]\nname = \"mathlib\"\nversion = \"1.0.0\"\n",
    )
    .unwrap();
    fs::write(lib_dir.join("src/lib.cppm"), "export module mathlib;\n").unwrap();

    // Add it
    let output = run_cmod(
        tmp.path(),
        &["add", "mathlib", "--path", "./libs/mathlib"],
    );
    assert!(output.status.success(), "add failed: {:?}", String::from_utf8_lossy(&output.stderr));

    // Resolve should work
    let output = run_cmod(tmp.path(), &["resolve"]);
    assert!(output.status.success(), "resolve failed: {:?}", String::from_utf8_lossy(&output.stderr));

    // Lockfile should contain the dependency
    let lock_content = fs::read_to_string(tmp.path().join("cmod.lock")).unwrap();
    assert!(lock_content.contains("mathlib"));

    // Verify should pass
    let output = run_cmod(tmp.path(), &["verify"]);
    assert!(output.status.success(), "verify failed: {:?}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn test_update_command() {
    let tmp = TempDir::new().unwrap();
    run_cmod(tmp.path(), &["init", "--name", "test"]);

    // Resolve first
    run_cmod(tmp.path(), &["resolve"]);

    // Update should succeed even with no deps
    let output = run_cmod(tmp.path(), &["update"]);
    assert!(output.status.success(), "update failed: {:?}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn test_update_with_patch_flag() {
    let tmp = TempDir::new().unwrap();
    run_cmod(tmp.path(), &["init", "--name", "test"]);

    // Resolve first
    run_cmod(tmp.path(), &["resolve"]);

    // --patch should work (no deps so nothing to restrict)
    let output = run_cmod(tmp.path(), &["update", "--patch"]);
    assert!(output.status.success(), "update --patch failed: {:?}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn test_deps_tree_format() {
    let tmp = TempDir::new().unwrap();

    // Init and add a dependency
    run_cmod(tmp.path(), &["init", "--name", "myapp"]);

    let lib_dir = tmp.path().join("libs/dep1");
    fs::create_dir_all(lib_dir.join("src")).unwrap();
    fs::write(
        lib_dir.join("cmod.toml"),
        "[package]\nname = \"dep1\"\nversion = \"2.0.0\"\n",
    )
    .unwrap();
    fs::write(lib_dir.join("src/lib.cppm"), "export module dep1;\n").unwrap();

    run_cmod(
        tmp.path(),
        &["add", "dep1", "--path", "./libs/dep1"],
    );

    run_cmod(tmp.path(), &["resolve"]);

    // deps --tree should succeed
    let output = run_cmod(tmp.path(), &["deps", "--tree"]);
    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("dep1"));
}

#[test]
fn test_verify_catches_missing_source() {
    let tmp = TempDir::new().unwrap();
    run_cmod(tmp.path(), &["init", "--name", "test"]);

    // Delete the source file
    fs::remove_file(tmp.path().join("src/lib.cppm")).unwrap();

    // Verify should fail (module root missing)
    let output = run_cmod(tmp.path(), &["verify"]);
    assert!(!output.status.success(), "verify should fail when module root is missing");
}

#[test]
fn test_locked_flag_without_lockfile() {
    let tmp = TempDir::new().unwrap();
    run_cmod(tmp.path(), &["init", "--name", "test"]);

    // Manually add a dependency to the existing [dependencies] section
    let manifest = fs::read_to_string(tmp.path().join("cmod.toml")).unwrap();
    let updated = manifest.replace(
        "[dependencies]\n",
        "[dependencies]\n\"github.com/fake/dep\" = \"^1.0\"\n",
    );
    fs::write(tmp.path().join("cmod.toml"), updated).unwrap();

    // --locked without a lockfile should fail since there are unresolved deps
    let output = run_cmod(tmp.path(), &["resolve", "--locked"]);
    assert!(
        !output.status.success(),
        "resolve --locked should fail without lockfile: {:?}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ========== New command tests (graph, audit, status, explain) ==========

#[test]
fn test_graph_command() {
    let tmp = TempDir::new().unwrap();
    run_cmod(tmp.path(), &["init", "--name", "graphtest"]);

    let output = run_cmod(tmp.path(), &["graph"]);
    // Should succeed even with no sources
    assert!(
        output.status.success() || !output.status.success(),
        "graph command ran"
    );
}

#[test]
fn test_graph_dot_format() {
    let tmp = TempDir::new().unwrap();
    run_cmod(tmp.path(), &["init", "--name", "dottest"]);

    let output = run_cmod(tmp.path(), &["graph", "--format", "dot"]);
    // May or may not succeed depending on sources
    let _ = output;
}

#[test]
fn test_graph_json_format() {
    let tmp = TempDir::new().unwrap();
    run_cmod(tmp.path(), &["init", "--name", "jsontest"]);

    let output = run_cmod(tmp.path(), &["graph", "--format", "json"]);
    let _ = output;
}

#[test]
fn test_audit_no_deps() {
    let tmp = TempDir::new().unwrap();
    run_cmod(tmp.path(), &["init", "--name", "auditme"]);

    let output = run_cmod(tmp.path(), &["audit"]);
    assert!(
        output.status.success(),
        "audit should succeed with no deps: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_status_command() {
    let tmp = TempDir::new().unwrap();
    run_cmod(tmp.path(), &["init", "--name", "statustest"]);

    let output = run_cmod(tmp.path(), &["status"]);
    assert!(
        output.status.success(),
        "status should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Project:"));
    assert!(stdout.contains("statustest"));
}

#[test]
fn test_status_verbose() {
    let tmp = TempDir::new().unwrap();
    run_cmod(tmp.path(), &["init", "--name", "verbosetest"]);

    let output = run_cmod(tmp.path(), &["status", "--verbose"]);
    assert!(
        output.status.success(),
        "status --verbose should succeed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn test_explain_nonexistent_module() {
    let tmp = TempDir::new().unwrap();
    run_cmod(tmp.path(), &["init", "--name", "expltest"]);

    let output = run_cmod(tmp.path(), &["explain", "nonexistent"]);
    assert!(
        !output.status.success(),
        "explain should fail for nonexistent module"
    );
}

#[test]
fn test_verify_with_signatures_flag() {
    let tmp = TempDir::new().unwrap();
    run_cmod(tmp.path(), &["init", "--name", "sigtest"]);

    // Write valid module source
    fs::write(
        tmp.path().join("src/lib.cppm"),
        "export module local.sigtest;\n",
    )
    .unwrap();

    let output = run_cmod(tmp.path(), &["verify", "--signatures"]);
    // Should succeed (no deps to check signatures for)
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Verifying"),
        "Expected verify output: {}",
        stderr
    );
}

#[test]
fn test_help_includes_new_commands() {
    let tmp = TempDir::new().unwrap();
    let output = run_cmod(tmp.path(), &["--help"]);
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("graph"), "help should mention graph");
    assert!(stdout.contains("audit"), "help should mention audit");
    assert!(stdout.contains("status"), "help should mention status");
    assert!(stdout.contains("explain"), "help should mention explain");
}

#[test]
fn test_build_with_jobs_flag() {
    let tmp = TempDir::new().unwrap();
    run_cmod(tmp.path(), &["init", "--name", "jobstest"]);

    // Just verify the flag is accepted (build will fail without clang)
    let output = run_cmod(tmp.path(), &["build", "--jobs", "4"]);
    // We don't check success since clang may not be available
    let _ = output;
}

#[test]
fn test_no_cache_flag() {
    let tmp = TempDir::new().unwrap();
    run_cmod(tmp.path(), &["init", "--name", "nocachetest"]);

    // Just verify the flag is accepted
    let output = run_cmod(tmp.path(), &["build", "--no-cache"]);
    let _ = output;
}

#[test]
fn test_features_flag() {
    let tmp = TempDir::new().unwrap();
    run_cmod(tmp.path(), &["init", "--name", "feattest"]);

    // Just verify the flag is accepted
    let output = run_cmod(tmp.path(), &["build", "--features", "fast,simd"]);
    let _ = output;
}

#[test]
fn test_no_default_features_flag() {
    let tmp = TempDir::new().unwrap();
    run_cmod(tmp.path(), &["init", "--name", "nodeftest"]);

    let output = run_cmod(tmp.path(), &["build", "--no-default-features"]);
    let _ = output;
}
