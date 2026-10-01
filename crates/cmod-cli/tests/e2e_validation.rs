//! Comprehensive end-to-end validation tests for the cmod CLI.
//!
//! Tests are grouped by command/feature area. Tests requiring LLVM Clang
//! (for actual C++20 module compilation) look for `/opt/homebrew/opt/llvm/bin/clang++`
//! on macOS and `clang++` on PATH on Linux, and skip gracefully if neither
//! is available.
//!
//! Run all tests:
//!   cargo test --test e2e_validation
//!
//! Run with LLVM (ensures compilation tests execute):
//!   PATH="/opt/homebrew/opt/llvm/bin:$PATH" cargo test --test e2e_validation
//!
//! Run a specific group:
//!   cargo test --test e2e_validation -- test_e2e_build

use std::fs;
use std::path::Path;
use std::process::Command;
use tempfile::TempDir;

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Run cmod in a given directory with the given arguments.
fn run_cmod(dir: &Path, args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_cmod"))
        .args(args)
        .current_dir(dir)
        .output()
        .expect("failed to run cmod")
}

/// Run cmod with LLVM Clang on PATH (prepends /opt/homebrew/opt/llvm/bin).
fn run_cmod_with_llvm(dir: &Path, args: &[&str]) -> std::process::Output {
    let llvm_path = "/opt/homebrew/opt/llvm/bin";
    let current_path = std::env::var("PATH").unwrap_or_default();
    let new_path = format!("{}:{}", llvm_path, current_path);

    Command::new(env!("CARGO_BIN_EXE_cmod"))
        .args(args)
        .current_dir(dir)
        .env("PATH", new_path)
        .output()
        .expect("failed to run cmod")
}

/// Check if a Clang that builds C++20 modules is available: Homebrew LLVM,
/// or on Linux any `clang++` on PATH (Apple's clang cannot build modules).
fn has_llvm_clang() -> bool {
    let homebrew = Path::new("/opt/homebrew/opt/llvm/bin/clang++");
    let llvm_clang = if homebrew.exists() {
        homebrew
    } else if cfg!(target_os = "linux") {
        Path::new("clang++")
    } else {
        return false;
    };
    Command::new(llvm_clang)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Check if clang-format is available on PATH.
fn has_clang_format() -> bool {
    Command::new("clang-format")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Initialize a cmod project with `cmod init --name <name>`.
fn init_project(dir: &Path, name: &str) {
    let output = run_cmod(dir, &["init", "--name", name]);
    assert!(
        output.status.success(),
        "init '{}' failed: {}",
        name,
        stderr(&output)
    );
}

/// Initialize a project and write custom C++20 module source files for compilation.
fn init_project_with_source(dir: &Path, name: &str) {
    init_project(dir, name);

    let cpp_name = name.replace('-', "_");
    let module_name = format!("local.{}", cpp_name);

    // Module interface with a testable function
    fs::write(
        dir.join("src/lib.cppm"),
        format!(
            "export module {};\n\nexport namespace {} {{\n    int add(int a, int b) {{ return a + b; }}\n}} // namespace {}\n",
            module_name, cpp_name, cpp_name
        ),
    ).unwrap();

    // Main that uses the module
    fs::write(
        dir.join("src/main.cpp"),
        format!(
            "import {};\n\nint main() {{\n    return {}::add(20, 22) == 42 ? 0 : 1;\n}}\n",
            module_name, cpp_name
        ),
    )
    .unwrap();

    // Test that uses the module
    fs::write(
        dir.join("tests/main.cpp"),
        format!(
            "import {};\n\nint main() {{\n    return {}::add(1, 2) == 3 ? 0 : 1;\n}}\n",
            module_name, cpp_name
        ),
    )
    .unwrap();
}

/// Get stderr output as a string.
fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

/// Get stdout output as a string.
fn stdout(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

// ─── Group 1: Project Initialization ─────────────────────────────────────────

#[test]
fn test_e2e_init_creates_all_files() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "mylib");

    assert!(tmp.path().join("cmod.toml").exists());
    assert!(tmp.path().join("src/lib.cppm").exists());
    assert!(tmp.path().join("src/main.cpp").exists());
    assert!(tmp.path().join("tests/main.cpp").exists());

    let manifest = fs::read_to_string(tmp.path().join("cmod.toml")).unwrap();
    assert!(manifest.contains("name = \"mylib\""));
    assert!(manifest.contains("version = \"0.1.0\""));
    assert!(manifest.contains("[module]"));
    assert!(manifest.contains("local.mylib"));
}

#[test]
fn test_e2e_init_workspace_creates_manifest() {
    let tmp = TempDir::new().unwrap();
    let output = run_cmod(tmp.path(), &["init", "--workspace", "--name", "engine"]);
    assert!(
        output.status.success(),
        "init workspace failed: {}",
        stderr(&output)
    );

    let manifest = fs::read_to_string(tmp.path().join("cmod.toml")).unwrap();
    assert!(manifest.contains("[workspace]"));
    assert!(manifest.contains("name = \"engine\""));
    // Workspace should not have src/ files
    assert!(!tmp.path().join("src/lib.cppm").exists());
}

#[test]
fn test_e2e_init_duplicate_fails() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "test");

    let output = run_cmod(tmp.path(), &["init", "--name", "test"]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("already exists"));
}

#[test]
fn test_e2e_init_hyphen_sanitization() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "my-lib");

    let cppm = fs::read_to_string(tmp.path().join("src/lib.cppm")).unwrap();
    // Hyphens should be converted to underscores in C++ identifiers
    assert!(cppm.contains("local.my_lib"));
    assert!(cppm.contains("namespace my_lib"));
}

#[test]
fn test_e2e_init_uses_dir_name_without_name_flag() {
    let tmp = TempDir::new().unwrap();
    let subdir = tmp.path().join("cool_project");
    fs::create_dir_all(&subdir).unwrap();

    let output = run_cmod(&subdir, &["init"]);
    assert!(output.status.success(), "init failed: {}", stderr(&output));

    let manifest = fs::read_to_string(subdir.join("cmod.toml")).unwrap();
    assert!(manifest.contains("name = \"cool_project\""));
}

#[test]
fn test_e2e_init_module_source_content() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "hello");

    let lib = fs::read_to_string(tmp.path().join("src/lib.cppm")).unwrap();
    assert!(lib.contains("export module local.hello;"));
    assert!(lib.contains("export namespace hello"));

    let main = fs::read_to_string(tmp.path().join("src/main.cpp")).unwrap();
    assert!(main.contains("import local.hello;"));
    assert!(main.contains("int main()"));
}

#[test]
fn test_e2e_init_test_file_content() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "hello");

    let test = fs::read_to_string(tmp.path().join("tests/main.cpp")).unwrap();
    assert!(test.contains("import local.hello;"));
    assert!(test.contains("int main()"));
}

// ─── Group 2: Dependency Management ─────────────────────────────────────────

#[test]
fn test_e2e_add_path_dependency() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "app");

    // Create a local library
    let lib_dir = tmp.path().join("libs/mylib");
    fs::create_dir_all(&lib_dir).unwrap();
    fs::write(
        lib_dir.join("cmod.toml"),
        "[package]\nname = \"mylib\"\nversion = \"1.0.0\"\n",
    )
    .unwrap();

    let output = run_cmod(tmp.path(), &["add", "mylib", "--path", "./libs/mylib"]);
    assert!(output.status.success(), "add failed: {}", stderr(&output));

    let manifest = fs::read_to_string(tmp.path().join("cmod.toml")).unwrap();
    assert!(manifest.contains("mylib"));
}

#[test]
fn test_e2e_remove_dependency() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "app");

    // Add then remove
    let lib_dir = tmp.path().join("libs/mylib");
    fs::create_dir_all(&lib_dir).unwrap();
    fs::write(
        lib_dir.join("cmod.toml"),
        "[package]\nname = \"mylib\"\nversion = \"1.0.0\"\n",
    )
    .unwrap();

    run_cmod(tmp.path(), &["add", "mylib", "--path", "./libs/mylib"]);
    let output = run_cmod(tmp.path(), &["remove", "mylib"]);
    assert!(
        output.status.success(),
        "remove failed: {}",
        stderr(&output)
    );

    let manifest = fs::read_to_string(tmp.path().join("cmod.toml")).unwrap();
    assert!(!manifest.contains("mylib"));
}

#[test]
fn test_e2e_remove_nonexistent_fails() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "app");

    let output = run_cmod(tmp.path(), &["remove", "nonexistent"]);
    assert!(!output.status.success());
}

#[test]
fn test_e2e_deps_shows_empty() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "app");
    run_cmod(tmp.path(), &["resolve"]);

    let output = run_cmod(tmp.path(), &["deps"]);
    assert!(output.status.success());
    let err = stderr(&output);
    assert!(
        err.contains("No dependencies") || err.contains("no dependencies"),
        "expected 'no dependencies' in stderr, got: {}",
        err
    );
}

#[test]
fn test_e2e_deps_tree_with_path_dep() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "app");

    let lib_dir = tmp.path().join("libs/mylib");
    fs::create_dir_all(&lib_dir).unwrap();
    fs::write(
        lib_dir.join("cmod.toml"),
        "[package]\nname = \"mylib\"\nversion = \"2.0.0\"\n",
    )
    .unwrap();

    run_cmod(tmp.path(), &["add", "mylib", "--path", "./libs/mylib"]);

    let output = run_cmod(tmp.path(), &["deps", "--tree"]);
    assert!(
        output.status.success(),
        "deps --tree failed: {}",
        stderr(&output)
    );
    let out = stdout(&output);
    assert!(out.contains("app"));
    assert!(out.contains("mylib"));
}

#[test]
fn test_e2e_deps_conflicts_no_issues() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "app");
    run_cmod(tmp.path(), &["resolve"]);

    let output = run_cmod(tmp.path(), &["deps", "--conflicts"]);
    assert!(output.status.success());
    let err = stderr(&output);
    assert!(
        err.contains("No version conflicts") || err.contains("no version conflicts"),
        "expected 'no version conflicts' in stderr, got: {}",
        err
    );
}

// ─── Group 3: Build System (real compilation) ────────────────────────────────

#[test]
fn test_e2e_build_debug_module() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found at /opt/homebrew/opt/llvm/bin/clang++");
        return;
    }

    let tmp = TempDir::new().unwrap();
    init_project_with_source(tmp.path(), "hello");

    let output = run_cmod_with_llvm(tmp.path(), &["build"]);
    assert!(output.status.success(), "build failed: {}", stderr(&output));

    // Build artifacts should exist
    let build_dir = tmp.path().join("build/debug");
    assert!(build_dir.exists(), "build/debug directory not created");
}

#[test]
fn test_e2e_build_release_module() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let tmp = TempDir::new().unwrap();
    init_project_with_source(tmp.path(), "hello");

    let output = run_cmod_with_llvm(tmp.path(), &["build", "--release"]);
    assert!(
        output.status.success(),
        "build --release failed: {}",
        stderr(&output)
    );

    let build_dir = tmp.path().join("build/release");
    assert!(build_dir.exists(), "build/release directory not created");
}

#[test]
fn test_e2e_build_force_rebuild() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let tmp = TempDir::new().unwrap();
    init_project_with_source(tmp.path(), "hello");

    // First build
    let output = run_cmod_with_llvm(tmp.path(), &["build"]);
    assert!(
        output.status.success(),
        "first build failed: {}",
        stderr(&output)
    );

    // Force rebuild
    let output = run_cmod_with_llvm(tmp.path(), &["build", "--force"]);
    assert!(
        output.status.success(),
        "force rebuild failed: {}",
        stderr(&output)
    );
}

#[test]
fn test_e2e_build_with_timings() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let tmp = TempDir::new().unwrap();
    init_project_with_source(tmp.path(), "hello");

    let output = run_cmod_with_llvm(tmp.path(), &["build", "--timings"]);
    assert!(
        output.status.success(),
        "build --timings failed: {}",
        stderr(&output)
    );
}

#[test]
fn test_e2e_build_verbose_output() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let tmp = TempDir::new().unwrap();
    init_project_with_source(tmp.path(), "hello");

    let output = run_cmod_with_llvm(tmp.path(), &["-v", "build"]);
    assert!(
        output.status.success(),
        "verbose build failed: {}",
        stderr(&output)
    );
    let err = stderr(&output);
    assert!(err.contains("Found") || err.contains("source files") || err.contains("Build"));
}

#[test]
fn test_e2e_build_compile_error() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "broken");

    // Write invalid C++ that will fail to compile
    fs::write(
        tmp.path().join("src/lib.cppm"),
        "export module local.broken;\n\nexport int broken() { this is not valid c++; }\n",
    )
    .unwrap();

    let output = run_cmod_with_llvm(tmp.path(), &["build"]);
    assert!(
        !output.status.success(),
        "build should have failed for broken source"
    );
}

#[test]
fn test_e2e_build_no_source_files() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "empty");

    // Remove all source files
    fs::remove_file(tmp.path().join("src/lib.cppm")).unwrap();
    fs::remove_file(tmp.path().join("src/main.cpp")).unwrap();

    let output = run_cmod(tmp.path(), &["build"]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("no source files"));
}

#[test]
fn test_e2e_build_incremental_skip() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let tmp = TempDir::new().unwrap();
    init_project_with_source(tmp.path(), "hello");

    // First build
    let output = run_cmod_with_llvm(tmp.path(), &["build"]);
    assert!(
        output.status.success(),
        "first build failed: {}",
        stderr(&output)
    );

    // Second build should detect up-to-date modules
    let output = run_cmod_with_llvm(tmp.path(), &["build"]);
    assert!(
        output.status.success(),
        "second build failed: {}",
        stderr(&output)
    );
    let err = stderr(&output);
    assert!(
        err.contains("up-to-date") || err.contains("cached") || err.contains("Build complete"),
        "expected incremental skip, got: {}",
        err
    );
}

#[test]
fn test_e2e_build_static_lib() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let tmp = TempDir::new().unwrap();
    init_project_with_source(tmp.path(), "mystaticlib");

    // Change build type from binary to static-lib
    let manifest = fs::read_to_string(tmp.path().join("cmod.toml")).unwrap();
    let manifest = manifest.replace("type = \"binary\"", "type = \"static-lib\"");
    fs::write(tmp.path().join("cmod.toml"), manifest).unwrap();

    // Remove main.cpp (static libs don't have a main)
    fs::remove_file(tmp.path().join("src/main.cpp")).unwrap();

    let output = run_cmod_with_llvm(tmp.path(), &["build"]);
    assert!(
        output.status.success(),
        "static lib build failed: {}",
        stderr(&output)
    );
}

#[test]
fn test_e2e_build_with_partitions() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "mathlib");

    // Write a module with partitions
    fs::write(
        tmp.path().join("src/lib.cppm"),
        "export module local.mathlib;\nexport import :ops;\n",
    )
    .unwrap();

    fs::write(
        tmp.path().join("src/ops.cppm"),
        "export module local.mathlib:ops;\n\nexport namespace mathlib {\n    int multiply(int a, int b) { return a * b; }\n}\n",
    ).unwrap();

    fs::write(
        tmp.path().join("src/main.cpp"),
        "import local.mathlib;\n\nint main() {\n    return mathlib::multiply(6, 7) == 42 ? 0 : 1;\n}\n",
    ).unwrap();

    let output = run_cmod_with_llvm(tmp.path(), &["build"]);
    assert!(
        output.status.success(),
        "partition build failed: {}",
        stderr(&output)
    );
}

#[test]
fn test_e2e_build_no_manifest() {
    let tmp = TempDir::new().unwrap();
    // No cmod.toml
    let output = run_cmod(tmp.path(), &["build"]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("cmod.toml") || stderr(&output).contains("init"));
}

// ─── Group 4: Run Command ───────────────────────────────────────────────────

#[test]
fn test_e2e_run_binary() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let tmp = TempDir::new().unwrap();
    init_project_with_source(tmp.path(), "runner");

    let output = run_cmod_with_llvm(tmp.path(), &["run"]);
    // Binary returns 0 when add(20,22)==42
    assert!(output.status.success(), "run failed: {}", stderr(&output));
}

#[test]
fn test_e2e_run_release() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let tmp = TempDir::new().unwrap();
    init_project_with_source(tmp.path(), "runner");

    // Build in release mode first so both debug and release dirs exist
    let output = run_cmod_with_llvm(tmp.path(), &["build", "--release"]);
    assert!(
        output.status.success(),
        "build --release failed: {}",
        stderr(&output)
    );

    // Also do a debug build so `cmod run --release` can find a binary
    // (the run command currently looks in the debug build dir)
    let output = run_cmod_with_llvm(tmp.path(), &["build"]);
    assert!(
        output.status.success(),
        "build debug failed: {}",
        stderr(&output)
    );

    let output = run_cmod_with_llvm(tmp.path(), &["run"]);
    assert!(output.status.success(), "run failed: {}", stderr(&output));
}

#[test]
fn test_e2e_run_no_manifest() {
    let tmp = TempDir::new().unwrap();
    let output = run_cmod(tmp.path(), &["run"]);
    assert!(!output.status.success());
}

// ─── Group 5: Test Command ──────────────────────────────────────────────────

#[test]
fn test_e2e_test_passes() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let tmp = TempDir::new().unwrap();
    init_project_with_source(tmp.path(), "tested");

    let output = run_cmod_with_llvm(tmp.path(), &["test"]);
    assert!(output.status.success(), "test failed: {}", stderr(&output));
    assert!(stderr(&output).contains("passed") || stderr(&output).contains("test"));
}

#[test]
fn test_e2e_test_failure_detected() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let tmp = TempDir::new().unwrap();
    init_project_with_source(tmp.path(), "failtest");

    // Write a test that fails (returns non-zero)
    fs::write(
        tmp.path().join("tests/main.cpp"),
        "int main() { return 1; }\n",
    )
    .unwrap();

    let output = run_cmod_with_llvm(tmp.path(), &["test"]);
    assert!(!output.status.success(), "test should have failed");
}

#[test]
fn test_e2e_test_compile_failure_fails() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let tmp = TempDir::new().unwrap();
    init_project_with_source(tmp.path(), "nocompile");

    // The only test does not compile, so nothing runs.
    fs::write(
        tmp.path().join("tests/main.cpp"),
        "int main() { return not_declared; }\n",
    )
    .unwrap();

    let output = run_cmod_with_llvm(tmp.path(), &["test"]);
    assert!(
        !output.status.success(),
        "test should fail when no test compiles: {}",
        stderr(&output)
    );
    assert!(
        stderr(&output).contains("test result: FAILED. 0 passed, 1 failed"),
        "compile failure should be counted: {}",
        stderr(&output)
    );
}

#[test]
fn test_e2e_test_partial_compile_failure_fails() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let tmp = TempDir::new().unwrap();
    init_project_with_source(tmp.path(), "partial");

    // tests/main.cpp passes; tests/broken.cpp does not compile.
    fs::write(
        tmp.path().join("tests/broken.cpp"),
        "int main() { return not_declared; }\n",
    )
    .unwrap();

    let output = run_cmod_with_llvm(tmp.path(), &["test"]);
    assert!(
        !output.status.success(),
        "test should fail when any test does not compile: {}",
        stderr(&output)
    );
    assert!(
        stderr(&output).contains("test result: FAILED. 1 passed, 1 failed"),
        "compile failure should be counted: {}",
        stderr(&output)
    );
}

/// A second `cmod test` compiles only the tests whose inputs changed:
/// the test source, a header it includes, or the package it links.
#[test]
fn test_e2e_test_recompiles_only_changed_tests() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let tmp = TempDir::new().unwrap();
    init_project_with_source(tmp.path(), "incr");
    let header = tmp.path().join("tests/value.h");
    fs::write(&header, "#define VALUE 1\n").unwrap();
    fs::write(
        tmp.path().join("tests/uses_header.cpp"),
        "#include \"value.h\"\nint main() { return VALUE == 2 ? 0 : 1; }\n",
    )
    .unwrap();

    let compiled = |output: &std::process::Output| -> Vec<String> {
        stderr(output)
            .lines()
            .filter_map(|l| l.trim().strip_prefix("Compiling test: "))
            .map(str::to_string)
            .collect()
    };

    // VALUE is 1: uses_header fails.
    let output = run_cmod_with_llvm(tmp.path(), &["-v", "test"]);
    assert!(!output.status.success(), "{}", stderr(&output));
    let mut first = compiled(&output);
    first.sort();
    assert_eq!(first, ["main", "uses_header"], "{}", stderr(&output));

    // Nothing changed: nothing compiles, and the failure is still reported.
    let output = run_cmod_with_llvm(tmp.path(), &["-v", "test"]);
    assert!(!output.status.success(), "{}", stderr(&output));
    assert!(compiled(&output).is_empty(), "{}", stderr(&output));

    // The header changes: only its includer is rebuilt, and now passes.
    rewrite(&header, "#define VALUE 2\n");
    let output = run_cmod_with_llvm(tmp.path(), &["-v", "test"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(compiled(&output), ["uses_header"], "{}", stderr(&output));

    // A test that fails to compile is compiled again once fixed.
    let broken = tmp.path().join("tests/broken.cpp");
    fs::write(&broken, "int main() { return not_declared; }\n").unwrap();
    let output = run_cmod_with_llvm(tmp.path(), &["-v", "test"]);
    assert!(!output.status.success(), "{}", stderr(&output));
    rewrite(&broken, "int main() { return 0; }\n");
    let output = run_cmod_with_llvm(tmp.path(), &["-v", "test"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(compiled(&output), ["broken"], "{}", stderr(&output));
}

/// Tests with the same file name in different directories are built to
/// different binaries. They used to share `test_<stem>`, so both ran
/// whichever was compiled last.
#[test]
fn test_e2e_tests_with_the_same_name_run_separately() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let tmp = TempDir::new().unwrap();
    init_project_with_source(tmp.path(), "samename");
    for (dir, code) in [("a", 0), ("b", 3)] {
        let dir = tmp.path().join("tests").join(dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            dir.join("check.cpp"),
            format!("int main() {{ return {code}; }}\n"),
        )
        .unwrap();
    }

    // --no-fail-fast: tests run in parallel, and without it the failing
    // test can stop the run before `main` runs.
    let output = run_cmod_with_llvm(tmp.path(), &["test", "--no-fail-fast"]);
    let err = stderr(&output);
    assert!(!output.status.success(), "{}", err);
    assert!(
        err.contains("test result: FAILED. 2 passed, 1 failed"),
        "{}",
        err
    );
    assert!(err.contains("tests/b/check"), "{}", err);
}

#[test]
fn test_e2e_test_no_tests_dir() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let tmp = TempDir::new().unwrap();
    init_project_with_source(tmp.path(), "notests");

    // Remove tests directory
    fs::remove_dir_all(tmp.path().join("tests")).unwrap();

    let output = run_cmod_with_llvm(tmp.path(), &["test"]);
    // Should succeed but skip (no tests to run)
    assert!(
        output.status.success(),
        "test without tests dir should succeed: {}",
        stderr(&output)
    );
    assert!(stderr(&output).contains("No tests") || stderr(&output).contains("skipping"));
}

// ─── Group 6: Workspace Build ───────────────────────────────────────────────

#[test]
fn test_e2e_workspace_add_and_list() {
    let tmp = TempDir::new().unwrap();

    // Create workspace
    let output = run_cmod(tmp.path(), &["init", "--workspace", "--name", "monorepo"]);
    assert!(output.status.success());

    // Add a member
    let output = run_cmod(tmp.path(), &["workspace", "add", "core"]);
    assert!(
        output.status.success(),
        "workspace add failed: {}",
        stderr(&output)
    );

    // Check member was created
    assert!(tmp.path().join("core/cmod.toml").exists());
    assert!(tmp.path().join("core/src/lib.cppm").exists());

    // List members
    let output = run_cmod(tmp.path(), &["workspace", "list"]);
    assert!(
        output.status.success(),
        "workspace list failed: {}",
        stderr(&output)
    );
    assert!(stderr(&output).contains("core"));
}

#[test]
fn test_e2e_workspace_remove_member() {
    let tmp = TempDir::new().unwrap();
    run_cmod(tmp.path(), &["init", "--workspace", "--name", "mono"]);
    run_cmod(tmp.path(), &["workspace", "add", "lib1"]);

    let output = run_cmod(tmp.path(), &["workspace", "remove", "lib1"]);
    assert!(
        output.status.success(),
        "workspace remove failed: {}",
        stderr(&output)
    );
    assert!(stderr(&output).contains("Removed"));
}

#[test]
fn test_e2e_workspace_remove_nonexistent() {
    let tmp = TempDir::new().unwrap();
    run_cmod(tmp.path(), &["init", "--workspace", "--name", "mono"]);

    let output = run_cmod(tmp.path(), &["workspace", "remove", "ghost"]);
    assert!(!output.status.success());
}

#[test]
fn test_e2e_workspace_build() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let tmp = TempDir::new().unwrap();
    run_cmod(tmp.path(), &["init", "--workspace", "--name", "ws"]);
    run_cmod(tmp.path(), &["workspace", "add", "core"]);

    // Write a simple module in the core member
    fs::write(
        tmp.path().join("core/src/lib.cppm"),
        "export module local.core;\n\nexport namespace core {\n    int value() { return 42; }\n}\n",
    )
    .unwrap();

    // Mark core as a static lib (no main.cpp)
    let core_manifest = fs::read_to_string(tmp.path().join("core/cmod.toml")).unwrap();
    let core_manifest = core_manifest.replace("type = \"binary\"", "type = \"static-lib\"");
    fs::write(tmp.path().join("core/cmod.toml"), core_manifest).unwrap();

    // Remove the generated main.cpp from core (it's a library)
    let _ = fs::remove_file(tmp.path().join("core/src/main.cpp"));

    let output = run_cmod_with_llvm(tmp.path(), &["build"]);
    assert!(
        output.status.success(),
        "workspace build failed: {}",
        stderr(&output)
    );
    assert!(
        stderr(&output).contains("Workspace")
            || stderr(&output).contains("workspace")
            || stderr(&output).contains("Building member")
    );
}

// ─── Group 7: Clean Command ─────────────────────────────────────────────────

#[test]
fn test_e2e_clean_no_build() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "cleanme");

    let output = run_cmod(tmp.path(), &["clean"]);
    assert!(output.status.success(), "clean failed: {}", stderr(&output));
    assert!(stderr(&output).contains("Cleaned") || stderr(&output).contains("Cleaning"));
}

#[test]
fn test_e2e_clean_after_build() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let tmp = TempDir::new().unwrap();
    init_project_with_source(tmp.path(), "cleanme");

    run_cmod_with_llvm(tmp.path(), &["build"]);
    assert!(
        tmp.path().join("build").exists(),
        "build dir should exist after build"
    );

    let output = run_cmod(tmp.path(), &["clean"]);
    assert!(output.status.success(), "clean failed: {}", stderr(&output));
    assert!(
        !tmp.path().join("build").exists(),
        "build dir should be removed after clean"
    );
}

// ─── Group 8: Plan Output ───────────────────────────────────────────────────

#[test]
fn test_e2e_plan_json_output() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "plantest");

    let output = run_cmod(tmp.path(), &["plan"]);
    assert!(output.status.success(), "plan failed: {}", stderr(&output));

    let out = stdout(&output);
    // Plan output should be valid JSON
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&out);
    assert!(
        parsed.is_ok(),
        "plan output should be valid JSON, got: {}",
        out
    );
}

#[test]
fn test_e2e_plan_contains_nodes() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "plantest2");

    let output = run_cmod(tmp.path(), &["plan"]);
    assert!(output.status.success());

    let out = stdout(&output);
    let parsed: serde_json::Value = serde_json::from_str(&out).unwrap();
    // Should be an array of nodes
    assert!(
        parsed.is_array(),
        "plan should output an array, got: {}",
        out
    );
    assert!(
        !parsed.as_array().unwrap().is_empty(),
        "plan should have at least one node"
    );
}

// ─── Group 9: CMake Export ──────────────────────────────────────────────────

#[test]
fn test_e2e_emit_cmake_creates_file() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "cmaketest");

    let output = run_cmod(tmp.path(), &["emit-cmake"]);
    assert!(
        output.status.success(),
        "emit-cmake failed: {}",
        stderr(&output)
    );
    assert!(tmp.path().join("CMakeLists.txt").exists());
}

#[test]
fn test_e2e_emit_cmake_content() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "cmaketest2");

    run_cmod(tmp.path(), &["emit-cmake"]);

    let cmake = fs::read_to_string(tmp.path().join("CMakeLists.txt")).unwrap();
    assert!(cmake.contains("cmake_minimum_required"));
    assert!(cmake.contains("project(cmaketest2"));
    assert!(cmake.contains("CXX_STANDARD 20"));
    assert!(cmake.contains("add_executable") || cmake.contains("add_library"));
    assert!(cmake.contains("lib.cppm") || cmake.contains("main.cpp"));
}

// ─── Group 10: Compile Commands ─────────────────────────────────────────────

#[test]
fn test_e2e_compile_commands_creates_file() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "cctest");

    let output = run_cmod(tmp.path(), &["compile-commands"]);
    assert!(
        output.status.success(),
        "compile-commands failed: {}",
        stderr(&output)
    );
    assert!(tmp.path().join("compile_commands.json").exists());
}

#[test]
fn test_e2e_compile_commands_valid_json() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "cctest2");

    run_cmod(tmp.path(), &["compile-commands"]);

    let content = fs::read_to_string(tmp.path().join("compile_commands.json")).unwrap();
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&content);
    assert!(parsed.is_ok(), "compile_commands.json should be valid JSON");

    let arr = parsed.unwrap();
    assert!(arr.is_array());
    // Each entry should have "file", "directory", "command" or "arguments"
    for entry in arr.as_array().unwrap() {
        assert!(
            entry.get("file").is_some(),
            "entry missing 'file': {:?}",
            entry
        );
        assert!(
            entry.get("directory").is_some(),
            "entry missing 'directory': {:?}",
            entry
        );
    }
}

// ─── Group 11: Graph Visualization ──────────────────────────────────────────

#[test]
fn test_e2e_graph_ascii() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "graphtest");

    let output = run_cmod(tmp.path(), &["graph"]);
    assert!(output.status.success(), "graph failed: {}", stderr(&output));

    let out = stdout(&output);
    assert!(
        out.contains("graphtest"),
        "ASCII graph should show project name"
    );
}

#[test]
fn test_e2e_graph_dot_format() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "graphtest2");

    let output = run_cmod(tmp.path(), &["graph", "--format", "dot"]);
    assert!(
        output.status.success(),
        "graph --format dot failed: {}",
        stderr(&output)
    );

    let out = stdout(&output);
    assert!(
        out.contains("digraph"),
        "DOT output should contain 'digraph'"
    );
    assert!(
        out.contains("rankdir"),
        "DOT output should contain 'rankdir'"
    );
}

#[test]
fn test_e2e_graph_json_format() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "graphtest3");

    let output = run_cmod(tmp.path(), &["graph", "--format", "json"]);
    assert!(
        output.status.success(),
        "graph --format json failed: {}",
        stderr(&output)
    );

    let out = stdout(&output);
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&out);
    assert!(
        parsed.is_ok(),
        "graph JSON output should be valid JSON, got: {}",
        out
    );
}

#[test]
fn test_e2e_graph_with_filter() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "graphfilter");

    let output = run_cmod(tmp.path(), &["graph", "--filter", "local"]);
    assert!(
        output.status.success(),
        "graph --filter failed: {}",
        stderr(&output)
    );
}

// ─── Group 12: Verify Command ───────────────────────────────────────────────

#[test]
fn test_e2e_verify_valid_project() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "verified");

    let output = run_cmod(tmp.path(), &["verify"]);
    assert!(
        output.status.success(),
        "verify failed: {}",
        stderr(&output)
    );
    assert!(
        stderr(&output).contains("Verification passed")
            || stderr(&output).contains("No issues")
            || stderr(&output).contains("verification passed")
            || stderr(&output).contains("no issues")
            || stderr(&output).contains("Verified")
    );
}

#[test]
fn test_e2e_verify_name_mismatch() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "mismatch");

    // Change the module declaration in source to not match manifest
    fs::write(
        tmp.path().join("src/lib.cppm"),
        "export module wrong_name;\n\nexport namespace wrong {}\n",
    )
    .unwrap();

    let output = run_cmod(tmp.path(), &["verify"]);
    assert!(
        !output.status.success(),
        "verify should fail with name mismatch"
    );
    assert!(stderr(&output).contains("mismatch") || stderr(&output).contains("declares"));
}

#[test]
fn test_e2e_verify_missing_source() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "nosrc");

    // Remove all source files
    fs::remove_dir_all(tmp.path().join("src")).unwrap();

    let output = run_cmod(tmp.path(), &["verify"]);
    assert!(
        !output.status.success(),
        "verify should fail with missing sources"
    );
}

#[test]
fn test_e2e_verify_verbose() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "vverify");

    let output = run_cmod(tmp.path(), &["-v", "verify"]);
    assert!(output.status.success());
    let err = stderr(&output);
    assert!(
        err.contains("Checking") || err.contains("manifest") || err.contains("module"),
        "verbose verify should show detailed checks, got: {}",
        err
    );
}

// ─── Group 13: Check Command ────────────────────────────────────────────────

#[test]
fn test_e2e_check_valid_module() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "checkme");

    let output = run_cmod(tmp.path(), &["check"]);
    assert!(output.status.success(), "check failed: {}", stderr(&output));
    assert!(stderr(&output).contains("passed") || stderr(&output).contains("check"));
}

#[test]
fn test_e2e_check_reserved_prefix() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "stdmod");

    // Modify the module name to use reserved "std" prefix
    let manifest = fs::read_to_string(tmp.path().join("cmod.toml")).unwrap();
    let manifest = manifest.replace("local.stdmod", "std.mymod");
    fs::write(tmp.path().join("cmod.toml"), manifest).unwrap();

    // Update source to match
    fs::write(
        tmp.path().join("src/lib.cppm"),
        "export module std.mymod;\n\nexport namespace stdmod {}\n",
    )
    .unwrap();

    let output = run_cmod(tmp.path(), &["check"]);
    assert!(
        !output.status.success(),
        "check should fail for reserved prefix"
    );
    assert!(stderr(&output).contains("reserved") || stderr(&output).contains("std"));
}

#[test]
fn test_e2e_check_source_mismatch() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "mismatch2");

    // Make source module name differ from manifest
    fs::write(
        tmp.path().join("src/lib.cppm"),
        "export module different_name;\n\nexport namespace mismatch2 {}\n",
    )
    .unwrap();

    let output = run_cmod(tmp.path(), &["check"]);
    assert!(
        !output.status.success(),
        "check should fail for module name mismatch"
    );
    assert!(stderr(&output).contains("mismatch"));
}

// ─── Group 14: Status Command ───────────────────────────────────────────────

#[test]
fn test_e2e_status_output() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "statustest");

    let output = run_cmod(tmp.path(), &["status"]);
    assert!(
        output.status.success(),
        "status failed: {}",
        stderr(&output)
    );

    let out = stdout(&output);
    assert!(
        out.contains("statustest"),
        "status should show project name"
    );
    assert!(out.contains("Module:") || out.contains("local.statustest"));
    assert!(out.contains("Sources:"));
}

#[test]
fn test_e2e_status_shows_build_dir() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let tmp = TempDir::new().unwrap();
    init_project_with_source(tmp.path(), "statusbuild");
    run_cmod_with_llvm(tmp.path(), &["build"]);

    let output = run_cmod(tmp.path(), &["status"]);
    assert!(output.status.success());

    let out = stdout(&output);
    assert!(out.contains("Build dir:"));
}

// ─── Group 15: Explain Command ──────────────────────────────────────────────

#[test]
fn test_e2e_explain_existing_module() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "explaintest");

    let output = run_cmod(tmp.path(), &["explain", "local.explaintest"]);
    assert!(
        output.status.success(),
        "explain failed: {}",
        stderr(&output)
    );

    let out = stdout(&output);
    assert!(out.contains("Module:") || out.contains("local.explaintest"));
    assert!(out.contains("Source:") || out.contains("lib.cppm"));
}

#[test]
fn test_e2e_explain_nonexistent_module() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "explaintest2");

    let output = run_cmod(tmp.path(), &["explain", "nonexistent"]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("not found"));
}

// ─── Group 16: Lint Command ─────────────────────────────────────────────────

#[test]
fn test_e2e_lint_clean_source() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "linttest");

    let output = run_cmod(tmp.path(), &["lint"]);
    assert!(output.status.success(), "lint failed: {}", stderr(&output));
}

#[test]
fn test_e2e_lint_detects_issues() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "lintbad");

    // Write source with known lint issues
    fs::write(
        tmp.path().join("src/lib.cppm"),
        "export module local.lintbad;\n\n#pragma once\nusing namespace std;\n",
    )
    .unwrap();

    let output = run_cmod(tmp.path(), &["-v", "lint"]);
    assert!(output.status.success()); // lint returns success even with warnings
    let err = stderr(&output);
    assert!(
        err.contains("warning") || err.contains("#pragma once") || err.contains("using namespace"),
        "lint should detect issues, got: {}",
        err
    );
}

// ─── Group 17: Fmt Command ─────────────────────────────────────────────────

#[test]
fn test_e2e_fmt_check_mode() {
    if !has_clang_format() {
        eprintln!("Skipping: clang-format not available");
        return;
    }

    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "fmttest");

    let output = run_cmod(tmp.path(), &["fmt", "--check"]);
    // May pass or fail depending on default formatting
    // Just check it runs without crashing
    let _ = output.status;
}

#[test]
fn test_e2e_fmt_no_clang_format() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "fmttest2");

    // Run with a PATH that doesn't include clang-format
    let output = Command::new(env!("CARGO_BIN_EXE_cmod"))
        .args(["fmt"])
        .current_dir(tmp.path())
        .env("PATH", "/usr/bin") // minimal PATH without clang-format
        .output()
        .unwrap();

    // Should fail because clang-format is not found
    if !has_clang_format() {
        // Only assert failure if clang-format genuinely isn't at /usr/bin
        assert!(!output.status.success() || stderr(&output).contains("clang-format"));
    }
}

// ─── Group 18: Tidy Command ────────────────────────────────────────────────

#[test]
fn test_e2e_tidy_no_unused() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "tidytest");

    let output = run_cmod(tmp.path(), &["tidy"]);
    assert!(output.status.success(), "tidy failed: {}", stderr(&output));
    assert!(
        stderr(&output).contains("All dependencies are used") || stderr(&output).contains("used")
    );
}

#[test]
fn test_e2e_tidy_detects_unused() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "tidyunused");

    // Add a dependency that is never imported
    let lib_dir = tmp.path().join("libs/unused_lib");
    fs::create_dir_all(&lib_dir).unwrap();
    fs::write(
        lib_dir.join("cmod.toml"),
        "[package]\nname = \"unused_lib\"\nversion = \"1.0.0\"\n",
    )
    .unwrap();

    run_cmod(
        tmp.path(),
        &["add", "unused_lib", "--path", "./libs/unused_lib"],
    );

    let output = run_cmod(tmp.path(), &["tidy"]);
    assert!(output.status.success());
    let err = stderr(&output);
    assert!(err.contains("Unused") || err.contains("unused_lib"));
}

#[test]
fn test_e2e_tidy_apply_removes() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "tidyapply");

    let lib_dir = tmp.path().join("libs/dead_dep");
    fs::create_dir_all(&lib_dir).unwrap();
    fs::write(
        lib_dir.join("cmod.toml"),
        "[package]\nname = \"dead_dep\"\nversion = \"1.0.0\"\n",
    )
    .unwrap();

    run_cmod(
        tmp.path(),
        &["add", "dead_dep", "--path", "./libs/dead_dep"],
    );

    // Verify the dep is there
    let manifest = fs::read_to_string(tmp.path().join("cmod.toml")).unwrap();
    assert!(manifest.contains("dead_dep"));

    // Apply tidy
    let output = run_cmod(tmp.path(), &["tidy", "--apply"]);
    assert!(
        output.status.success(),
        "tidy --apply failed: {}",
        stderr(&output)
    );

    // Verify the dep was removed
    let manifest = fs::read_to_string(tmp.path().join("cmod.toml")).unwrap();
    assert!(
        !manifest.contains("dead_dep"),
        "dead_dep should have been removed by tidy --apply"
    );
}

// ─── Group 19: SBOM Command ────────────────────────────────────────────────

#[test]
fn test_e2e_sbom_stdout() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "sbomtest");

    let output = run_cmod(tmp.path(), &["sbom"]);
    assert!(output.status.success(), "sbom failed: {}", stderr(&output));

    let out = stdout(&output);
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&out);
    assert!(
        parsed.is_ok(),
        "SBOM output should be valid JSON, got: {}",
        out
    );
}

#[test]
fn test_e2e_sbom_to_file() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "sbomfile");

    let sbom_path = tmp.path().join("sbom.json");
    let output = run_cmod(tmp.path(), &["sbom", "-o", sbom_path.to_str().unwrap()]);
    assert!(
        output.status.success(),
        "sbom -o failed: {}",
        stderr(&output)
    );
    assert!(sbom_path.exists(), "SBOM file should exist");

    let content = fs::read_to_string(&sbom_path).unwrap();
    let parsed: Result<serde_json::Value, _> = serde_json::from_str(&content);
    assert!(parsed.is_ok(), "SBOM file should contain valid JSON");
}

// ─── Group 20: Publish Command ──────────────────────────────────────────────

#[test]
fn test_e2e_publish_dry_run() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "pubtest");

    // Initialize a git repo so publish can work
    Command::new("git")
        .args(["init"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    // Disable commit signing in this temp repo (may be enabled globally)
    Command::new("git")
        .args(["config", "commit.gpgsign", "false"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "test"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(tmp.path())
        .output()
        .unwrap();

    let output = run_cmod(tmp.path(), &["publish", "--dry-run", "--skip-governance"]);
    assert!(
        output.status.success(),
        "publish --dry-run failed: {}",
        stderr(&output)
    );
    let err = stderr(&output);
    assert!(err.contains("Dry run") || err.contains("dry run") || err.contains("would create"));
}

#[test]
fn test_e2e_publish_dirty_tree_fails() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "pubdirty");

    // Initialize git repo but don't commit everything
    Command::new("git")
        .args(["init"])
        .current_dir(tmp.path())
        .output()
        .unwrap();

    // Don't add files — tree is dirty
    let output = run_cmod(tmp.path(), &["publish"]);
    assert!(!output.status.success());
    assert!(
        stderr(&output).contains("uncommitted")
            || stderr(&output).contains("dirty")
            || stderr(&output).contains("change")
    );
}

#[test]
fn test_e2e_publish_creates_tag() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "pubtag");

    // Set up clean git repo with user identity (required on CI)
    Command::new("git")
        .args(["init"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    // Disable commit signing in this temp repo (may be enabled globally)
    Command::new("git")
        .args(["config", "commit.gpgsign", "false"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "tag.gpgsign", "false"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.name", "test"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["config", "user.email", "test@test.com"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "init"])
        .current_dir(tmp.path())
        .output()
        .unwrap();

    let output = run_cmod(tmp.path(), &["publish", "--skip-governance"]);
    assert!(
        output.status.success(),
        "publish failed: {}",
        stderr(&output)
    );

    // Verify tag was created
    let tag_output = Command::new("git")
        .args(["tag", "--list", "v0.1.0"])
        .current_dir(tmp.path())
        .output()
        .unwrap();
    let tags = String::from_utf8_lossy(&tag_output.stdout);
    assert!(
        tags.contains("v0.1.0"),
        "tag v0.1.0 should exist after publish"
    );
}

// ─── Group 21: Vendor Command ───────────────────────────────────────────────

#[test]
fn test_e2e_vendor_no_deps() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "vendortest");
    run_cmod(tmp.path(), &["resolve"]);

    let output = run_cmod(tmp.path(), &["vendor"]);
    assert!(
        output.status.success(),
        "vendor failed: {}",
        stderr(&output)
    );
    assert!(tmp.path().join("vendor").exists());
    assert!(tmp.path().join("vendor/config.toml").exists());
}

#[test]
fn test_e2e_vendor_sync() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "vendorsync");
    run_cmod(tmp.path(), &["resolve"]);

    // First vendor
    run_cmod(tmp.path(), &["vendor"]);

    // Vendor with --sync
    let output = run_cmod(tmp.path(), &["vendor", "--sync"]);
    assert!(
        output.status.success(),
        "vendor --sync failed: {}",
        stderr(&output)
    );
}

// ─── Group 22: Cache Operations ─────────────────────────────────────────────

#[test]
fn test_e2e_cache_status() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "cachetest");

    let output = run_cmod(tmp.path(), &["cache", "status"]);
    assert!(
        output.status.success(),
        "cache status failed: {}",
        stderr(&output)
    );
}

#[test]
fn test_e2e_cache_clean() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "cacheclean");

    let output = run_cmod(tmp.path(), &["cache", "clean"]);
    assert!(
        output.status.success(),
        "cache clean failed: {}",
        stderr(&output)
    );
}

#[test]
fn test_e2e_cache_gc() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "cachegc");

    let output = run_cmod(tmp.path(), &["cache", "gc"]);
    assert!(
        output.status.success(),
        "cache gc failed: {}",
        stderr(&output)
    );
}

#[test]
fn test_e2e_cache_after_build() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let tmp = TempDir::new().unwrap();
    init_project_with_source(tmp.path(), "cachebuild");

    run_cmod_with_llvm(tmp.path(), &["build"]);

    let output = run_cmod(tmp.path(), &["cache", "status"]);
    assert!(output.status.success());
}

// ─── Group 23: Toolchain Command ────────────────────────────────────────────

#[test]
fn test_e2e_toolchain_show() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "tcshow");

    let output = run_cmod(tmp.path(), &["toolchain", "show"]);
    assert!(
        output.status.success(),
        "toolchain show failed: {}",
        stderr(&output)
    );
    let err = stderr(&output);
    assert!(err.contains("Compiler") || err.contains("compiler") || err.contains("clang"));
    assert!(err.contains("Standard") || err.contains("C++"));
}

#[test]
fn test_e2e_toolchain_check() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "tccheck");

    let output = run_cmod(tmp.path(), &["toolchain", "check"]);
    // May succeed or fail depending on whether clang is on PATH
    // Just verify it doesn't crash
    let _ = output.status;
}

// ─── Group 24: Search Command ───────────────────────────────────────────────

#[test]
fn test_e2e_search_no_results() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "searchtest");

    let output = run_cmod(tmp.path(), &["search", "nonexistent_lib_xyz"]);
    assert!(output.status.success());
    assert!(
        stderr(&output).contains("No modules matching")
            || stderr(&output).contains("no modules matching")
    );
}

#[test]
fn test_e2e_search_finds_dependency() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "searchfind");

    // Add a dependency so we have something to search for
    let lib_dir = tmp.path().join("libs/searchable");
    fs::create_dir_all(&lib_dir).unwrap();
    fs::write(
        lib_dir.join("cmod.toml"),
        "[package]\nname = \"searchable\"\nversion = \"1.0.0\"\n",
    )
    .unwrap();
    run_cmod(
        tmp.path(),
        &["add", "searchable", "--path", "./libs/searchable"],
    );

    let output = run_cmod(tmp.path(), &["search", "search"]);
    assert!(output.status.success());
    let err = stderr(&output);
    assert!(err.contains("searchable") || err.contains("Found"));
}

// ─── Group 25: Audit Command ────────────────────────────────────────────────

#[test]
fn test_e2e_audit_no_deps() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "audittest");

    let output = run_cmod(tmp.path(), &["audit"]);
    assert!(output.status.success(), "audit failed: {}", stderr(&output));
    assert!(
        stderr(&output).contains("No dependencies")
            || stderr(&output).contains("No issues")
            || stderr(&output).contains("audit")
    );
}

// ─── Group 26: Plugin Command ───────────────────────────────────────────────

#[test]
fn test_e2e_plugin_list_empty() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "plugintest");

    let output = run_cmod(tmp.path(), &["plugin", "list"]);
    assert!(
        output.status.success(),
        "plugin list failed: {}",
        stderr(&output)
    );
    assert!(stderr(&output).contains("No plugins") || stderr(&output).contains("no plugins"));
}

#[test]
fn test_e2e_plugin_run_nonexistent() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "plugintest2");

    let output = run_cmod(tmp.path(), &["plugin", "run", "nonexistent"]);
    assert!(!output.status.success());
    assert!(stderr(&output).contains("not found"));
}

// ─── Group 27: Global Flags ─────────────────────────────────────────────────

#[test]
fn test_e2e_help_flag() {
    let output = Command::new(env!("CARGO_BIN_EXE_cmod"))
        .arg("--help")
        .output()
        .unwrap();

    assert!(output.status.success());
    let out = stdout(&output);
    assert!(out.contains("cmod"));
    assert!(out.contains("build") || out.contains("Build"));
    assert!(out.contains("init") || out.contains("Init"));
}

#[test]
fn test_e2e_version_flag() {
    let output = Command::new(env!("CARGO_BIN_EXE_cmod"))
        .arg("--version")
        .output()
        .unwrap();

    assert!(output.status.success());
    let out = stdout(&output);
    assert!(out.contains("cmod"));
}

#[test]
fn test_e2e_locked_flag_without_lockfile() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "lockedtest");

    // cmod build --locked should fail if cmod.lock does not exist AND there are dependencies
    let lib_dir = tmp.path().join("libs/dep");
    fs::create_dir_all(&lib_dir).unwrap();
    fs::write(
        lib_dir.join("cmod.toml"),
        "[package]\nname = \"dep\"\nversion = \"1.0.0\"\n",
    )
    .unwrap();
    run_cmod(tmp.path(), &["add", "dep", "--path", "./libs/dep"]);

    // Delete the lockfile that `cmod add` resolved
    std::fs::remove_file(tmp.path().join("cmod.lock")).unwrap();

    let output = run_cmod(tmp.path(), &["--locked", "build"]);
    assert!(!output.status.success());
    let err = stderr(&output);
    assert!(err.contains("lockfile") || err.contains("not found"));
}

// ─── Group 28: Full E2E Workflows ───────────────────────────────────────────

#[test]
fn test_e2e_full_lifecycle() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let tmp = TempDir::new().unwrap();

    // 1. Init
    init_project_with_source(tmp.path(), "lifecycle");

    // 2. Resolve
    let output = run_cmod(tmp.path(), &["resolve"]);
    assert!(
        output.status.success(),
        "resolve failed: {}",
        stderr(&output)
    );
    assert!(tmp.path().join("cmod.lock").exists());

    // 3. Verify
    let output = run_cmod(tmp.path(), &["verify"]);
    assert!(
        output.status.success(),
        "verify failed: {}",
        stderr(&output)
    );

    // 4. Build
    let output = run_cmod_with_llvm(tmp.path(), &["build"]);
    assert!(output.status.success(), "build failed: {}", stderr(&output));

    // 5. Test
    let output = run_cmod_with_llvm(tmp.path(), &["test"]);
    assert!(output.status.success(), "test failed: {}", stderr(&output));

    // 6. Run
    let output = run_cmod_with_llvm(tmp.path(), &["run"]);
    assert!(output.status.success(), "run failed: {}", stderr(&output));

    // 7. Clean
    let output = run_cmod(tmp.path(), &["clean"]);
    assert!(output.status.success(), "clean failed: {}", stderr(&output));
    assert!(!tmp.path().join("build").exists());
}

#[test]
fn test_e2e_workspace_lifecycle() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let tmp = TempDir::new().unwrap();

    // 1. Init workspace
    run_cmod(tmp.path(), &["init", "--workspace", "--name", "monorepo"]);

    // 2. Add core member with a module (library — no main)
    run_cmod(tmp.path(), &["workspace", "add", "core"]);
    fs::write(
        tmp.path().join("core/src/lib.cppm"),
        "export module local.core;\n\nexport namespace core {\n    int value() { return 42; }\n}\n",
    )
    .unwrap();
    // Mark core as static lib
    let core_manifest = fs::read_to_string(tmp.path().join("core/cmod.toml")).unwrap();
    let core_manifest = core_manifest.replace("type = \"binary\"", "type = \"static-lib\"");
    fs::write(tmp.path().join("core/cmod.toml"), core_manifest).unwrap();
    let _ = fs::remove_file(tmp.path().join("core/src/main.cpp"));

    // 3. Add app member that imports core (binary)
    run_cmod(tmp.path(), &["workspace", "add", "app"]);
    // Declare core as a path dependency so workspace PCM propagation works
    let app_manifest = fs::read_to_string(tmp.path().join("app/cmod.toml")).unwrap();
    let app_manifest = if app_manifest.contains("[dependencies]") {
        app_manifest.replace(
            "[dependencies]",
            "[dependencies]\ncore = { path = \"../core\" }",
        )
    } else {
        format!(
            "{}\n[dependencies]\ncore = {{ path = \"../core\" }}\n",
            app_manifest
        )
    };
    fs::write(tmp.path().join("app/cmod.toml"), app_manifest).unwrap();
    fs::write(
        tmp.path().join("app/src/lib.cppm"),
        "export module local.app;\nimport local.core;\n\nexport namespace app {\n    int get() { return core::value(); }\n}\n",
    ).unwrap();
    fs::write(
        tmp.path().join("app/src/main.cpp"),
        "import local.app;\n\nint main() {\n    return app::get() == 42 ? 0 : 1;\n}\n",
    )
    .unwrap();

    // 4. List workspace
    let output = run_cmod(tmp.path(), &["workspace", "list"]);
    assert!(output.status.success());
    let err = stderr(&output);
    assert!(err.contains("core"));
    assert!(err.contains("app"));

    // 5. Build workspace
    let output = run_cmod_with_llvm(tmp.path(), &["build"]);
    assert!(
        output.status.success(),
        "workspace build failed: {}",
        stderr(&output)
    );

    // 6. Verify
    let output = run_cmod(tmp.path(), &["verify"]);
    assert!(
        output.status.success(),
        "workspace verify failed: {}",
        stderr(&output)
    );
}

#[test]
fn test_e2e_build_then_modify_rebuild() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let tmp = TempDir::new().unwrap();
    init_project_with_source(tmp.path(), "rebuild");

    // First build
    let output = run_cmod_with_llvm(tmp.path(), &["build"]);
    assert!(
        output.status.success(),
        "first build failed: {}",
        stderr(&output)
    );

    // Modify source
    let cpp_name = "rebuild";
    let module_name = format!("local.{}", cpp_name);
    fs::write(
        tmp.path().join("src/lib.cppm"),
        format!(
            "export module {};\n\nexport namespace {} {{\n    int add(int a, int b) {{ return a + b; }}\n    int sub(int a, int b) {{ return a - b; }}\n}} // namespace {}\n",
            module_name, cpp_name, cpp_name
        ),
    ).unwrap();

    // Rebuild — should detect changes
    let output = run_cmod_with_llvm(tmp.path(), &["build"]);
    assert!(
        output.status.success(),
        "rebuild failed: {}",
        stderr(&output)
    );
    let err = stderr(&output);
    assert!(err.contains("Build complete") || err.contains("compiled") || err.contains("Building"));
}

// ─── Group: Resolve Command ─────────────────────────────────────────────────

#[test]
fn test_e2e_resolve_creates_lockfile() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "resolveme");

    let output = run_cmod(tmp.path(), &["resolve"]);
    assert!(
        output.status.success(),
        "resolve failed: {}",
        stderr(&output)
    );
    assert!(tmp.path().join("cmod.lock").exists());
}

#[test]
fn test_e2e_resolve_with_path_dep() {
    let tmp = TempDir::new().unwrap();
    init_project(tmp.path(), "resolvedep");

    let lib_dir = tmp.path().join("libs/pathlib");
    fs::create_dir_all(&lib_dir).unwrap();
    fs::write(
        lib_dir.join("cmod.toml"),
        "[package]\nname = \"pathlib\"\nversion = \"0.5.0\"\n",
    )
    .unwrap();

    run_cmod(tmp.path(), &["add", "pathlib", "--path", "./libs/pathlib"]);

    let output = run_cmod(tmp.path(), &["resolve"]);
    assert!(
        output.status.success(),
        "resolve with path dep failed: {}",
        stderr(&output)
    );

    let lockfile = fs::read_to_string(tmp.path().join("cmod.lock")).unwrap();
    assert!(lockfile.contains("pathlib"));
}

// ─── Group 29: Rebuild Correctness ──────────────────────────────────────────
//
// Each of these failed on v0.1.0-alpha.6: the build succeeded and linked a
// binary built from old inputs.

/// Write a manifest for a binary package with its own cache directory, so
/// cache hits come only from this test's builds.
fn write_rebuild_manifest(dir: &Path, name: &str, extra: &str) {
    fs::create_dir_all(dir.join("src")).unwrap();
    let cache = dir.join(".test-cache");
    fs::write(
        dir.join("cmod.toml"),
        format!(
            "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2023\"\n\n\
             [toolchain]\ncompiler = \"clang\"\ncxx_standard = \"20\"\n\n\
             [build]\ntype = \"binary\"\n{extra}\n\
             [cache]\nlocal_path = {cache:?}\n"
        ),
    )
    .unwrap();
}

/// Build with `args`, then run the binary and return its stdout.
fn build_and_run(dir: &Path, name: &str, args: &[&str]) -> String {
    let output = run_cmod_with_llvm(dir, args);
    assert!(
        output.status.success(),
        "{:?} failed: {}",
        args,
        stderr(&output)
    );
    let run = Command::new(dir.join("build/debug").join(name))
        .output()
        .expect("failed to run built binary");
    assert!(run.status.success(), "binary failed: {}", stderr(&run));
    stdout(&run).trim().to_string()
}

/// A module interface and a plain TU that both include `include/value.h`.
fn write_header_project(dir: &Path) {
    write_rebuild_manifest(dir, "hdr", "include_dirs = [\"include\"]\n");
    fs::create_dir_all(dir.join("include")).unwrap();
    fs::write(dir.join("include/value.h"), "#define VALUE 1\n").unwrap();
    fs::write(
        dir.join("src/lib.cppm"),
        "module;\n#include \"value.h\"\nexport module local.hdr;\n\
         export int module_value() { return VALUE; }\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/plain.cpp"),
        "#include \"value.h\"\nint plain_value() { return VALUE * 10; }\n",
    )
    .unwrap();
    fs::write(
        dir.join("src/main.cpp"),
        "import local.hdr;\n#include <cstdio>\nint plain_value();\n\
         int main() { std::printf(\"%d %d\\n\", module_value(), plain_value()); }\n",
    )
    .unwrap();
}

/// Rewrite a file so its mtime moves even on coarse-grained filesystems.
fn rewrite(path: &Path, content: &str) {
    std::thread::sleep(std::time::Duration::from_millis(20));
    fs::write(path, content).unwrap();
}

#[test]
fn test_e2e_header_change_rebuilds_includers() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }
    let tmp = TempDir::new().unwrap();
    write_header_project(tmp.path());
    assert_eq!(build_and_run(tmp.path(), "hdr", &["build"]), "1 10");

    rewrite(&tmp.path().join("include/value.h"), "#define VALUE 2\n");
    assert_eq!(build_and_run(tmp.path(), "hdr", &["build"]), "2 20");

    // Touching a header without changing it rebuilds nothing.
    rewrite(&tmp.path().join("include/value.h"), "#define VALUE 2\n");
    let output = run_cmod_with_llvm(tmp.path(), &["build"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        !stderr(&output).contains("compiled"),
        "touch should not recompile: {}",
        stderr(&output)
    );
}

/// `--force` skips build state and goes to the cache, whose keys used to
/// cover only the source file: it handed back the object built with the old
/// header.
#[test]
fn test_e2e_cache_key_covers_included_headers() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }
    let tmp = TempDir::new().unwrap();
    write_header_project(tmp.path());
    assert_eq!(build_and_run(tmp.path(), "hdr", &["build"]), "1 10");

    rewrite(&tmp.path().join("include/value.h"), "#define VALUE 2\n");
    assert_eq!(
        build_and_run(tmp.path(), "hdr", &["build", "--force"]),
        "2 20"
    );

    // Back to the first header: every object comes from the cache.
    rewrite(&tmp.path().join("include/value.h"), "#define VALUE 1\n");
    fs::remove_dir_all(tmp.path().join("build")).unwrap();
    let output = run_cmod_with_llvm(tmp.path(), &["build"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        stderr(&output).contains("3 cached"),
        "expected all cache hits: {}",
        stderr(&output)
    );
    let run = Command::new(tmp.path().join("build/debug/hdr"))
        .output()
        .unwrap();
    assert_eq!(stdout(&run).trim(), "1 10");
}

/// A TU that imports a module rebuilt earlier in the same build was checked
/// against the previous build's state, so it stayed "up-to-date" and kept
/// the old inlined body until the next build.
#[test]
fn test_e2e_module_change_rebuilds_importer_in_same_build() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }
    let tmp = TempDir::new().unwrap();
    write_rebuild_manifest(tmp.path(), "inl", "");
    let lib = tmp.path().join("src/lib.cppm");
    fs::write(
        &lib,
        "export module local.inl;\n\nexport inline int f() { return 1; }\n",
    )
    .unwrap();
    fs::write(
        tmp.path().join("src/main.cpp"),
        "import local.inl;\n#include <cstdio>\nint main() { std::printf(\"%d\\n\", f()); }\n",
    )
    .unwrap();
    assert_eq!(build_and_run(tmp.path(), "inl", &["build"]), "1");

    rewrite(
        &lib,
        "export module local.inl;\n\nexport inline int f() { return 2; }\n",
    );
    assert_eq!(build_and_run(tmp.path(), "inl", &["build"]), "2");
}

/// A path dependency's BMI was not an input of the TUs importing it: after
/// the dependency changed, they stayed "up-to-date" for good.
#[test]
fn test_e2e_path_dependency_change_rebuilds_importer() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }
    let tmp = TempDir::new().unwrap();
    write_rebuild_manifest(
        tmp.path(),
        "pd",
        "\n[dependencies]\ndep = { path = \"libs/dep\" }\n",
    );
    let dep = tmp.path().join("libs/dep");
    fs::create_dir_all(dep.join("src")).unwrap();
    fs::write(
        dep.join("cmod.toml"),
        "[package]\nname = \"dep\"\nversion = \"0.1.0\"\nedition = \"2023\"\n\n\
         [module]\nname = \"local.dep\"\nroot = \"src/lib.cppm\"\n\n\
         [toolchain]\ncompiler = \"clang\"\ncxx_standard = \"20\"\n\n\
         [build]\ntype = \"static-lib\"\n",
    )
    .unwrap();
    let lib = dep.join("src/lib.cppm");
    fs::write(
        &lib,
        "export module local.dep;\n\nexport inline int f() { return 1; }\n",
    )
    .unwrap();
    fs::write(
        tmp.path().join("src/main.cpp"),
        "import local.dep;\n#include <cstdio>\nint main() { std::printf(\"%d\\n\", f()); }\n",
    )
    .unwrap();

    let output = run_cmod_with_llvm(tmp.path(), &["resolve"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert_eq!(build_and_run(tmp.path(), "pd", &["build"]), "1");

    rewrite(
        &lib,
        "export module local.dep;\n\nexport inline int f() { return 2; }\n",
    );
    assert_eq!(build_and_run(tmp.path(), "pd", &["build"]), "2");
}

// ─── Group 30: Incremental Dependencies and Links ───────────────────────────

/// A root binary with a path dependency whose module has a separate
/// implementation unit, so an implementation change reaches the root only
/// through the dependency's archive.
fn write_impl_dep_project(dir: &Path) -> std::path::PathBuf {
    write_rebuild_manifest(
        dir,
        "app",
        "\n[dependencies]\ndep = { path = \"libs/dep\" }\n",
    );
    let dep = dir.join("libs/dep");
    fs::create_dir_all(dep.join("src")).unwrap();
    fs::write(
        dep.join("cmod.toml"),
        "[package]\nname = \"dep\"\nversion = \"0.1.0\"\nedition = \"2023\"\n\n\
         [module]\nname = \"local.dep\"\nroot = \"src/lib.cppm\"\n\n\
         [toolchain]\ncompiler = \"clang\"\ncxx_standard = \"20\"\n\n\
         [build]\ntype = \"static-lib\"\n",
    )
    .unwrap();
    fs::write(
        dep.join("src/lib.cppm"),
        "export module local.dep;\n\nexport int f();\n",
    )
    .unwrap();
    let imp = dep.join("src/impl.cpp");
    fs::write(&imp, "module local.dep;\n\nint f() { return 1; }\n").unwrap();
    fs::write(
        dir.join("src/main.cpp"),
        "import local.dep;\n#include <cstdio>\nint main() { std::printf(\"%d\\n\", f()); }\n",
    )
    .unwrap();
    let output = run_cmod_with_llvm(dir, &["resolve"]);
    assert!(output.status.success(), "{}", stderr(&output));
    imp
}

/// A build with nothing changed compiles and links nothing, including the
/// dependency, whose objects used to be deleted before every build.
#[test]
fn test_e2e_noop_build_compiles_and_links_nothing() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }
    let tmp = TempDir::new().unwrap();
    write_impl_dep_project(tmp.path());
    assert_eq!(build_and_run(tmp.path(), "app", &["build"]), "1");

    let output = run_cmod_with_llvm(tmp.path(), &["-v", "build"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let err = stderr(&output);
    for word in ["Compiled", "Cached", "Linked"] {
        assert!(!err.contains(word), "no-op build {}: {}", word, err);
    }
}

/// Only the dependency's archive changes; the root's objects do not. The
/// root binary must still be relinked.
#[test]
fn test_e2e_dependency_implementation_change_relinks_root() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let imp = write_impl_dep_project(tmp.path());
    assert_eq!(build_and_run(tmp.path(), "app", &["build"]), "1");

    rewrite(&imp, "module local.dep;\n\nint f() { return 2; }\n");
    assert_eq!(build_and_run(tmp.path(), "app", &["build"]), "2");
}

/// Object names encode the source path, so a moved checkout leaves the old
/// location's objects in the dependency's obj/ directory. They must not be
/// linked in (duplicate symbols) or survive the build.
#[test]
fn test_e2e_moved_checkout_prunes_stale_objects() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let first = tmp.path().join("first");
    fs::create_dir_all(&first).unwrap();
    write_impl_dep_project(&first);
    assert_eq!(build_and_run(&first, "app", &["build"]), "1");

    let moved = tmp.path().join("moved");
    fs::rename(&first, &moved).unwrap();
    assert_eq!(build_and_run(&moved, "app", &["build"]), "1");

    let obj_dir = moved.join("libs/dep/build/debug/obj");
    for entry in fs::read_dir(&obj_dir).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().into_owned();
        assert!(
            !name.contains("first"),
            "stale object from the old location survived: {}",
            name
        );
    }
}

// ─── Group 31: Dry Run and Explain ──────────────────────────────────────────

/// `build --dry-run` names what would rebuild and why, and builds nothing.
#[test]
fn test_e2e_dry_run_reports_without_building() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }
    let tmp = TempDir::new().unwrap();
    write_header_project(tmp.path());
    assert_eq!(build_and_run(tmp.path(), "hdr", &["build"]), "1 10");

    let output = run_cmod_with_llvm(tmp.path(), &["build", "--dry-run"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("0 of 4 build steps would run"),
        "{}",
        stdout(&output)
    );

    rewrite(&tmp.path().join("include/value.h"), "#define VALUE 2\n");
    let state = tmp.path().join("build/debug/.cmod-build-state.json");
    let state_before = fs::read(&state).unwrap();
    let output = run_cmod_with_llvm(tmp.path(), &["build", "--dry-run"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("included header changed"), "{}", out);
    assert!(out.contains("4 of 4 build steps would run"), "{}", out);

    // Nothing was built or recorded.
    assert_eq!(fs::read(&state).unwrap(), state_before);
    let run = Command::new(tmp.path().join("build/debug/hdr"))
        .output()
        .unwrap();
    assert_eq!(stdout(&run).trim(), "1 10");
}

/// A dry run of a project never built writes nothing at all.
#[test]
fn test_e2e_dry_run_writes_nothing() {
    let tmp = TempDir::new().unwrap();
    init_project_with_source(tmp.path(), "dry");
    let output = run_cmod(tmp.path(), &["build", "--dry-run"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("no previous build state"),
        "{}",
        stdout(&output)
    );
    assert!(!tmp.path().join("build").exists());
    assert!(!tmp.path().join("cmod.lock").exists());
}

/// `explain` reports the reason the build would act on, not a cache-key
/// guess.
#[test]
fn test_e2e_explain_reports_header_change() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }
    let tmp = TempDir::new().unwrap();
    write_header_project(tmp.path());
    assert_eq!(build_and_run(tmp.path(), "hdr", &["build"]), "1 10");

    let output = run_cmod_with_llvm(tmp.path(), &["explain", "local.hdr"]);
    assert!(output.status.success(), "{}", stderr(&output));
    assert!(
        stdout(&output).contains("UP TO DATE"),
        "{}",
        stdout(&output)
    );

    rewrite(&tmp.path().join("include/value.h"), "#define VALUE 2\n");
    let output = run_cmod_with_llvm(tmp.path(), &["explain", "local.hdr"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("NEEDS REBUILD"), "{}", out);
    assert!(out.contains("included header changed"), "{}", out);
}

/// A dry run never resolves: without `cmod.lock` it still examines path
/// dependencies (a locked-mode dry run used to fail here), and it does not
/// write a lockfile.
#[test]
fn test_e2e_dry_run_without_lockfile_examines_path_deps() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let imp = write_impl_dep_project(tmp.path());
    assert_eq!(build_and_run(tmp.path(), "app", &["build"]), "1");
    fs::remove_file(tmp.path().join("cmod.lock")).unwrap();

    rewrite(&imp, "module local.dep;\n\nint f() { return 2; }\n");
    let output = run_cmod_with_llvm(tmp.path(), &["build", "--dry-run"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let out = stdout(&output);
    assert!(out.contains("impl.cpp): source file changed"), "{}", out);
    assert!(
        out.contains("link build/debug/app: link inputs changed"),
        "{}",
        out
    );
    assert!(!tmp.path().join("cmod.lock").exists());

    let output = run_cmod_with_llvm(tmp.path(), &["explain", "main"]);
    assert!(output.status.success(), "{}", stderr(&output));
}

/// With a lockfile, a dry run of an up-to-date project with a path
/// dependency reports nothing to do. It used to report the dependency as
/// "not checked out at the locked commit", as if it were a git dependency.
#[test]
fn test_e2e_dry_run_with_lockfile_skips_path_deps() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }
    let tmp = TempDir::new().unwrap();
    write_impl_dep_project(tmp.path());
    assert_eq!(build_and_run(tmp.path(), "app", &["build"]), "1");

    let output = run_cmod_with_llvm(tmp.path(), &["build", "--dry-run"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let out = stdout(&output);
    assert!(!out.contains("not checked out"), "{}", out);
    assert!(out.contains("\n0 of "), "{}", out);
}

/// A workspace member that depends on several members is not relinked by a
/// no-op build. The members' objects were passed to the link in hash-set
/// order, which differs between runs, so the link key changed every time.
#[test]
fn test_e2e_workspace_noop_build_links_nothing() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }
    let tmp = TempDir::new().unwrap();
    let root = tmp.path();
    write_three_member_workspace(root);

    let output = run_cmod_with_llvm(root, &["build"]);
    assert!(output.status.success(), "{}", stderr(&output));
    // Three members give six possible orders: one rebuild can match by luck.
    for _ in 0..3 {
        let output = run_cmod_with_llvm(root, &["-v", "build"]);
        assert!(output.status.success(), "{}", stderr(&output));
        let err = stderr(&output);
        assert!(
            !err.contains("Linked"),
            "no-op workspace build relinked: {}",
            err
        );
    }
    let run = Command::new(root.join("build/debug/app/app"))
        .status()
        .expect("failed to run built binary");
    assert!(run.success());
}

/// Whether `clang-scan-deps` runs, with the PATH `run_cmod_with_llvm` uses.
fn has_clang_scan_deps() -> bool {
    let path = format!(
        "/opt/homebrew/opt/llvm/bin:{}",
        std::env::var("PATH").unwrap_or_default()
    );
    let scanner = std::env::var_os("SCAN_DEPS").unwrap_or_else(|| "clang-scan-deps".into());
    Command::new(scanner)
        .arg("--version")
        .env("PATH", path)
        .output()
        .is_ok_and(|o| o.status.success())
}

/// Imports come from the preprocessed source: an `import` inside `#if 0`
/// is not one. The source-text extractor saw it, and with it a cycle
/// between `local.a` and `local.b`. Every clang-scan-deps run used to fail
/// (the compiler was missing from its command line), so it was never used.
/// Results are kept until the source changes.
#[test]
fn test_e2e_scan_follows_the_preprocessor_and_is_cached() {
    if !has_llvm_clang() || !has_clang_scan_deps() {
        eprintln!("Skipping: LLVM Clang or clang-scan-deps not found");
        return;
    }
    let tmp = TempDir::new().unwrap();
    write_rebuild_manifest(tmp.path(), "scan", "");
    let src = tmp.path().join("src");
    fs::write(
        src.join("a.cppm"),
        "export module local.a;\n#if 0\nimport local.b;\n#endif\nexport int a() { return 1; }\n",
    )
    .unwrap();
    fs::write(
        src.join("b.cppm"),
        "export module local.b;\nimport local.a;\nexport int b() { return a() + 1; }\n",
    )
    .unwrap();
    fs::write(
        src.join("main.cpp"),
        "import local.b;\n#include <cstdio>\nint main() { std::printf(\"%d\\n\", b()); }\n",
    )
    .unwrap();

    // Scan through a wrapper that fails once `scan-off` exists. The second
    // build can only succeed from cached results: falling back to the
    // source text would see the `#if 0` cycle.
    let build = |scanner: &Path| {
        let path = format!(
            "/opt/homebrew/opt/llvm/bin:{}",
            std::env::var("PATH").unwrap_or_default()
        );
        Command::new(env!("CARGO_BIN_EXE_cmod"))
            .args(["-v", "build"])
            .current_dir(tmp.path())
            .env("PATH", path)
            .env("SCAN_DEPS", scanner)
            .output()
            .expect("failed to run cmod")
    };
    let scanner = tmp.path().join("scan-wrapper.sh");
    let real = std::env::var_os("SCAN_DEPS").unwrap_or_else(|| "clang-scan-deps".into());
    fs::write(
        &scanner,
        format!(
            "#!/bin/sh\n[ -e {off:?} ] && [ \"$1\" != --version ] && exit 97\nexec {real:?} \"$@\"\n",
            off = tmp.path().join("scan-off"),
            real = real,
        ),
    )
    .unwrap();
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&scanner, fs::Permissions::from_mode(0o755)).unwrap();
    }
    #[cfg(not(unix))]
    let scanner = std::path::PathBuf::from(&real);

    let output = build(&scanner);
    assert!(output.status.success(), "{}", stderr(&output));
    if cfg!(unix) {
        fs::write(tmp.path().join("scan-off"), "").unwrap();
        let output = build(&scanner);
        assert!(output.status.success(), "{}", stderr(&output));
    }
    let run = Command::new(tmp.path().join("build/debug/scan"))
        .output()
        .expect("failed to run built binary");
    assert_eq!(stdout(&run).trim(), "2");

    let state: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(tmp.path().join("build/debug/.cmod-scan-state.json")).unwrap(),
    )
    .unwrap();
    let records = state["outputs"].as_object().unwrap();
    assert_eq!(records.len(), 3, "{:#}", state);
    let main = records
        .iter()
        .find(|(path, _)| path.ends_with("main.cpp"))
        .map(|(_, record)| record)
        .unwrap();
    assert_eq!(main["data"], serde_json::json!(["local.b"]), "{:#}", state);
}

/// An interface not named `.cppm` is compiled with `-x c++-module`, and
/// scanned the same way. Scanned as plain C++, its scan failed on every
/// build and was never cached.
#[test]
fn test_e2e_scan_handles_ixx_interfaces() {
    if !has_llvm_clang() || !has_clang_scan_deps() {
        eprintln!("Skipping: LLVM Clang or clang-scan-deps not found");
        return;
    }
    let tmp = TempDir::new().unwrap();
    write_rebuild_manifest(tmp.path(), "ixx", "");
    fs::write(
        tmp.path().join("src/lib.ixx"),
        "export module local.ixx;\nexport int v() { return 7; }\n",
    )
    .unwrap();
    fs::write(
        tmp.path().join("src/main.cpp"),
        "import local.ixx;\n#include <cstdio>\nint main() { std::printf(\"%d\\n\", v()); }\n",
    )
    .unwrap();
    assert_eq!(build_and_run(tmp.path(), "ixx", &["build"]), "7");

    let state: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(tmp.path().join("build/debug/.cmod-scan-state.json")).unwrap(),
    )
    .unwrap();
    let records = state["outputs"].as_object().unwrap();
    assert!(
        records.keys().any(|path| path.ends_with("lib.ixx")),
        "{:#}",
        state
    );
}

/// Every entry of `compile_commands.json` compiles, after a build, for a
/// package with a partition and a path dependency. Entries lacked the
/// partition's BMI (`:part` imports were not resolved) and every path
/// dependency's, so clangd reported "module not found" on code that built.
#[test]
fn test_e2e_compile_commands_entries_compile() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }
    let tmp = TempDir::new().unwrap();
    write_impl_dep_project(tmp.path());
    let src = tmp.path().join("src");
    fs::write(
        src.join("lib.cppm"),
        "export module local.app;\nexport import :part;\nexport int app() { return part() + 1; }\n",
    )
    .unwrap();
    fs::write(
        src.join("part.cppm"),
        "export module local.app:part;\nexport int part() { return 1; }\n",
    )
    .unwrap();
    fs::write(
        src.join("main.cpp"),
        "import local.app;\nimport local.dep;\n#include <cstdio>\nint main() { std::printf(\"%d\\n\", app() + f()); }\n",
    )
    .unwrap();
    assert_eq!(build_and_run(tmp.path(), "app", &["build"]), "3");

    let output = run_cmod_with_llvm(tmp.path(), &["compile-commands"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let db: Vec<serde_json::Value> = serde_json::from_str(
        &fs::read_to_string(tmp.path().join("compile_commands.json")).unwrap(),
    )
    .unwrap();
    assert_eq!(db.len(), 3, "{:#?}", db);
    assert_entries_compile(&db, &tmp.path().join("entry.o"));
}

/// A workspace of library members `a`, `b` and `c`, and a binary member
/// `app` that imports all three and returns 0 when they add up to 3.
fn write_three_member_workspace(root: &Path) {
    fs::write(
        root.join("cmod.toml"),
        "[package]\nname = \"ws\"\nversion = \"0.1.0\"\nedition = \"2023\"\n\n\
         [workspace]\nmembers = [\"a\", \"b\", \"c\", \"app\"]\n",
    )
    .unwrap();
    let member = |name: &str, build_type: &str, deps: &str| {
        let dir = root.join(name);
        fs::create_dir_all(dir.join("src")).unwrap();
        fs::write(
            dir.join("cmod.toml"),
            format!(
                "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\nedition = \"2023\"\n\n\
                 [module]\nname = \"local.{name}\"\nroot = \"src/lib.cppm\"\n\n\
                 [dependencies]\n{deps}\n\
                 [toolchain]\ncompiler = \"clang\"\ncxx_standard = \"20\"\n\n\
                 [build]\ntype = \"{build_type}\"\n"
            ),
        )
        .unwrap();
        dir
    };
    for name in ["a", "b", "c"] {
        let dir = member(name, "static-lib", "");
        fs::write(
            dir.join("src/lib.cppm"),
            format!("export module local.{name};\n\nexport int {name}() {{ return 1; }}\n"),
        )
        .unwrap();
    }
    let app = member(
        "app",
        "binary",
        "a = { path = \"../a\" }\nb = { path = \"../b\" }\nc = { path = \"../c\" }\n",
    );
    fs::write(
        app.join("src/lib.cppm"),
        "export module local.app;\nimport local.a;\nimport local.b;\nimport local.c;\n\n\
         export int total() { return a() + b() + c(); }\n",
    )
    .unwrap();
    fs::write(
        app.join("src/main.cpp"),
        "import local.app;\n\nint main() { return total() == 3 ? 0 : 1; }\n",
    )
    .unwrap();
}

/// Run every entry of a `compile_commands.json`, writing to `out`, and
/// fail with the compiler's error if one does not compile.
fn assert_entries_compile(db: &[serde_json::Value], out: &Path) {
    for entry in db {
        let mut args: Vec<String> = entry["arguments"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a.as_str().unwrap().to_string())
            .collect();
        let o = args.iter().position(|a| a == "-o").unwrap();
        args[o + 1] = out.display().to_string();
        let path = format!(
            "/opt/homebrew/opt/llvm/bin:{}",
            std::env::var("PATH").unwrap_or_default()
        );
        let run = Command::new(&args[0])
            .args(&args[1..])
            .current_dir(entry["directory"].as_str().unwrap())
            .env("PATH", path)
            .output()
            .unwrap();
        assert!(
            run.status.success(),
            "{}: {}",
            entry["file"],
            String::from_utf8_lossy(&run.stderr)
        );
    }
}

/// A workspace root gets one database with every member's entries, each
/// compiling with the upstream members' BMIs. It used to warn "no source
/// files" and write nothing.
#[test]
fn test_e2e_workspace_compile_commands() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }
    let tmp = TempDir::new().unwrap();
    write_three_member_workspace(tmp.path());
    let output = run_cmod_with_llvm(tmp.path(), &["build"]);
    assert!(output.status.success(), "{}", stderr(&output));

    let output = run_cmod_with_llvm(tmp.path(), &["compile-commands"]);
    assert!(output.status.success(), "{}", stderr(&output));
    let db: Vec<serde_json::Value> = serde_json::from_str(
        &fs::read_to_string(tmp.path().join("compile_commands.json")).unwrap(),
    )
    .unwrap();
    let mut files: Vec<String> = db
        .iter()
        .map(|e| {
            let file = Path::new(e["file"].as_str().unwrap());
            let member = file.parent().unwrap().parent().unwrap();
            format!(
                "{}/{}",
                member.file_name().unwrap().to_string_lossy(),
                file.file_name().unwrap().to_string_lossy()
            )
        })
        .collect();
    files.sort();
    assert_eq!(
        files,
        [
            "a/lib.cppm",
            "app/lib.cppm",
            "app/main.cpp",
            "b/lib.cppm",
            "c/lib.cppm"
        ]
    );
    assert_entries_compile(&db, &tmp.path().join("entry.o"));
}
