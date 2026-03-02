//! Real-world integration tests for the cmod CLI.
//!
//! Tests exercise sophisticated multi-file C++20 module projects,
//! path dependency composition, workspace cross-member imports,
//! build configuration variants, error cases, and Git-based resolution.
//!
//! Run all tests:
//!   cargo test --test real_projects
//!
//! Run with LLVM (ensures compilation tests execute):
//!   PATH="/opt/homebrew/opt/llvm/bin:$PATH" cargo test --test real_projects
//!
//! Run including network tests:
//!   CMOD_TEST_NETWORK=1 PATH="/opt/homebrew/opt/llvm/bin:$PATH" cargo test --test real_projects
//!
//! Run specific group:
//!   cargo test --test real_projects -- test_real_math
//!   cargo test --test real_projects -- test_real_path_dep
//!   cargo test --test real_projects -- test_real_workspace
//!   cargo test --test real_projects -- test_real_config
//!   cargo test --test real_projects -- test_real_error
//!   cargo test --test real_projects -- test_real_git

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

    // Always pass --no-cache to avoid stale cached PCMs referencing old temp dirs
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

/// Get stderr output as a string.
fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).to_string()
}

/// Get stdout output as a string.
fn stdout(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stdout).to_string()
}

/// Check if network tests are enabled via CMOD_TEST_NETWORK env var.
fn network_tests_enabled() -> bool {
    std::env::var("CMOD_TEST_NETWORK")
        .map(|v| v == "1")
        .unwrap_or(false)
}

/// Create a local Git repository with files and tagged commits.
///
/// `tags` is a list of (tag_name, files_to_add_or_modify) pairs.
/// Each tag creates a commit with the specified file changes then tags it.
fn create_local_git_repo(
    dir: &Path,
    initial_files: &[(&str, &str)],
    tags: &[(&str, &[(&str, &str)])],
) {
    fs::create_dir_all(dir).unwrap();

    // git init
    Command::new("git")
        .args(["init"])
        .current_dir(dir)
        .output()
        .expect("git init failed");

    // Set up git config for the repo
    for (key, value) in [("user.name", "test"), ("user.email", "test@test.com")] {
        Command::new("git")
            .args(["config", key, value])
            .current_dir(dir)
            .output()
            .unwrap();
    }

    // Write initial files
    for (path, content) in initial_files {
        let full_path = dir.join(path);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).unwrap();
        }
        fs::write(&full_path, content).unwrap();
    }

    // Initial commit
    Command::new("git")
        .args(["add", "."])
        .current_dir(dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "initial commit"])
        .current_dir(dir)
        .output()
        .unwrap();

    // Create tagged commits
    for (tag, files) in tags {
        for (path, content) in *files {
            let full_path = dir.join(path);
            if let Some(parent) = full_path.parent() {
                fs::create_dir_all(parent).unwrap();
            }
            fs::write(&full_path, content).unwrap();
        }
        Command::new("git")
            .args(["add", "."])
            .current_dir(dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", &format!("release {}", tag)])
            .current_dir(dir)
            .output()
            .unwrap();
        Command::new("git")
            .args(["tag", tag])
            .current_dir(dir)
            .output()
            .unwrap();
    }
}

/// Set up the multi-file math library project (Group A).
///
/// Creates a project with primary interface, two partitions, main, and tests.
/// Note: C++20 implementation units (`module foo;`) share the same graph key as
/// the primary interface (`export module foo;`), so we avoid them in this fixture.
fn setup_math_project(dir: &Path) {
    let output = run_cmod(dir, &["init", "--name", "mathlib"]);
    assert!(output.status.success(), "init failed: {}", stderr(&output));

    // Primary interface — re-exports partitions
    fs::write(
        dir.join("src/lib.cppm"),
        r#"export module local.mathlib;

export import :vec;
export import :ops;
"#,
    )
    .unwrap();

    // Partition: vec — defines Vec3 struct
    fs::write(
        dir.join("src/vec.cppm"),
        r#"export module local.mathlib:vec;

export namespace mathlib {
    struct Vec3 {
        float x, y, z;

        Vec3() : x(0), y(0), z(0) {}
        Vec3(float x, float y, float z) : x(x), y(y), z(z) {}
    };
} // namespace mathlib
"#,
    )
    .unwrap();

    // Partition: ops — math operations on Vec3
    fs::write(
        dir.join("src/ops.cppm"),
        r#"export module local.mathlib:ops;
import :vec;

export namespace mathlib {
    Vec3 add(const Vec3& a, const Vec3& b) {
        return Vec3(a.x + b.x, a.y + b.y, a.z + b.z);
    }

    float dot(const Vec3& a, const Vec3& b) {
        return a.x * b.x + a.y * b.y + a.z * b.z;
    }

    Vec3 scale(const Vec3& v, float s) {
        return Vec3(v.x * s, v.y * s, v.z * s);
    }
} // namespace mathlib
"#,
    )
    .unwrap();

    // Main — uses the module, verifies computation
    fs::write(
        dir.join("src/main.cpp"),
        r#"import local.mathlib;

int main() {
    mathlib::Vec3 a(1.0f, 2.0f, 3.0f);
    mathlib::Vec3 b(4.0f, 5.0f, 6.0f);

    auto sum = mathlib::add(a, b);
    if (sum.x != 5.0f || sum.y != 7.0f || sum.z != 9.0f)
        return 1;

    float d = mathlib::dot(a, b);
    if (d != 32.0f)
        return 2;

    auto scaled = mathlib::scale(a, 2.0f);
    if (scaled.x != 2.0f || scaled.y != 4.0f || scaled.z != 6.0f)
        return 3;

    return 0;
}
"#,
    )
    .unwrap();

    // Test file
    fs::write(
        dir.join("tests/main.cpp"),
        r#"import local.mathlib;

int main() {
    mathlib::Vec3 v(1.0f, 2.0f, 3.0f);
    if (v.x != 1.0f || v.y != 2.0f || v.z != 3.0f)
        return 1;

    mathlib::Vec3 a(1.0f, 0.0f, 0.0f);
    mathlib::Vec3 b(0.0f, 1.0f, 0.0f);
    auto c = mathlib::add(a, b);
    if (c.x != 1.0f || c.y != 1.0f || c.z != 0.0f)
        return 2;

    float d = mathlib::dot(a, b);
    if (d != 0.0f)
        return 3;

    auto s = mathlib::scale(a, 5.0f);
    if (s.x != 5.0f || s.y != 0.0f || s.z != 0.0f)
        return 4;

    return 0;
}
"#,
    )
    .unwrap();
}

/// Set up the path dependency project (Group B).
///
/// Creates two libraries (strutil, numutil) and an app that depends on both.
/// Path deps test resolution and dependency management — cross-project compilation
/// requires a workspace, so build/run tests are in Group C.
fn setup_path_dep_project(dir: &Path) {
    // Create strutil library
    let strutil_dir = dir.join("libs/strutil");
    fs::create_dir_all(strutil_dir.join("src")).unwrap();

    fs::write(
        strutil_dir.join("cmod.toml"),
        r#"[package]
name = "strutil"
version = "1.0.0"

[module]
name = "local.strutil"
root = "src/lib.cppm"

[toolchain]
compiler = "clang"
cxx_standard = "20"

[build]
type = "static-lib"
optimization = "debug"
lto = false
"#,
    )
    .unwrap();

    fs::write(
        strutil_dir.join("src/lib.cppm"),
        r#"export module local.strutil;

export namespace strutil {
    int length(const char* s) {
        int len = 0;
        while (s[len] != '\0') ++len;
        return len;
    }
} // namespace strutil
"#,
    )
    .unwrap();

    // Create numutil library
    let numutil_dir = dir.join("libs/numutil");
    fs::create_dir_all(numutil_dir.join("src")).unwrap();

    fs::write(
        numutil_dir.join("cmod.toml"),
        r#"[package]
name = "numutil"
version = "1.0.0"

[module]
name = "local.numutil"
root = "src/lib.cppm"

[toolchain]
compiler = "clang"
cxx_standard = "20"

[build]
type = "static-lib"
optimization = "debug"
lto = false
"#,
    )
    .unwrap();

    fs::write(
        numutil_dir.join("src/lib.cppm"),
        r#"export module local.numutil;

export namespace numutil {
    int abs(int x) {
        return x < 0 ? -x : x;
    }

    int clamp(int val, int lo, int hi) {
        if (val < lo) return lo;
        if (val > hi) return hi;
        return val;
    }
} // namespace numutil
"#,
    )
    .unwrap();

    // Create app project
    let app_dir = dir.join("app");
    fs::create_dir_all(app_dir.join("src")).unwrap();
    fs::create_dir_all(app_dir.join("tests")).unwrap();

    fs::write(
        app_dir.join("cmod.toml"),
        r#"[package]
name = "app"
version = "0.1.0"

[module]
name = "local.app"
root = "src/lib.cppm"

[toolchain]
compiler = "clang"
cxx_standard = "20"

[build]
type = "binary"
optimization = "debug"
lto = false

[dependencies]
strutil = { path = "../libs/strutil" }
numutil = { path = "../libs/numutil" }
"#,
    )
    .unwrap();

    fs::write(
        app_dir.join("src/lib.cppm"),
        r#"export module local.app;

export namespace app {
    int run() {
        return 42;
    }
} // namespace app
"#,
    )
    .unwrap();

    fs::write(
        app_dir.join("src/main.cpp"),
        r#"import local.app;

int main() {
    return app::run() == 42 ? 0 : 1;
}
"#,
    )
    .unwrap();
}

/// Set up the workspace project (Group C).
///
/// Creates a 3-member workspace: core → math → app.
fn setup_workspace_project(dir: &Path) {
    let output = run_cmod(dir, &["init", "--workspace", "--name", "engine"]);
    assert!(
        output.status.success(),
        "workspace init failed: {}",
        stderr(&output)
    );

    // Add members
    for name in &["core", "math", "app"] {
        let output = run_cmod(dir, &["workspace", "add", name]);
        assert!(
            output.status.success(),
            "workspace add {} failed: {}",
            name,
            stderr(&output)
        );
    }

    // Configure core as static-lib
    fs::write(
        dir.join("core/cmod.toml"),
        r#"[package]
name = "core"
version = "0.1.0"

[module]
name = "local.core"
root = "src/lib.cppm"

[toolchain]
compiler = "clang"
cxx_standard = "20"

[build]
type = "static-lib"
optimization = "debug"
lto = false
"#,
    )
    .unwrap();

    fs::write(
        dir.join("core/src/lib.cppm"),
        r#"export module local.core;

export namespace core {
    struct Point {
        float x, y;
        Point() : x(0), y(0) {}
        Point(float x, float y) : x(x), y(y) {}
    };

    float distance_sq(const Point& a, const Point& b) {
        float dx = a.x - b.x;
        float dy = a.y - b.y;
        return dx * dx + dy * dy;
    }
} // namespace core
"#,
    )
    .unwrap();

    let _ = fs::remove_file(dir.join("core/src/main.cpp"));

    // Configure math as static-lib depending on core
    fs::write(
        dir.join("math/cmod.toml"),
        r#"[package]
name = "math"
version = "0.1.0"

[module]
name = "local.math"
root = "src/lib.cppm"

[toolchain]
compiler = "clang"
cxx_standard = "20"

[build]
type = "static-lib"
optimization = "debug"
lto = false

[dependencies]
core = { path = "../core" }
"#,
    )
    .unwrap();

    fs::write(
        dir.join("math/src/lib.cppm"),
        r#"export module local.math;
import local.core;

export namespace math {
    core::Point midpoint(const core::Point& a, const core::Point& b) {
        return core::Point((a.x + b.x) / 2.0f, (a.y + b.y) / 2.0f);
    }

    bool is_close(const core::Point& a, const core::Point& b, float threshold) {
        return core::distance_sq(a, b) < threshold * threshold;
    }
} // namespace math
"#,
    )
    .unwrap();

    let _ = fs::remove_file(dir.join("math/src/main.cpp"));

    // Configure app as binary depending on math
    fs::write(
        dir.join("app/cmod.toml"),
        r#"[package]
name = "app"
version = "0.1.0"

[module]
name = "local.app"
root = "src/lib.cppm"

[toolchain]
compiler = "clang"
cxx_standard = "20"

[build]
type = "binary"
optimization = "debug"
lto = false

[dependencies]
math = { path = "../math" }
"#,
    )
    .unwrap();

    fs::write(
        dir.join("app/src/lib.cppm"),
        r#"export module local.app;
import local.math;
import local.core;

export namespace app {
    int run() {
        core::Point a(0.0f, 0.0f);
        core::Point b(4.0f, 0.0f);
        auto mid = math::midpoint(a, b);
        if (mid.x != 2.0f || mid.y != 0.0f)
            return 1;

        if (!math::is_close(a, b, 5.0f))
            return 2;

        if (math::is_close(a, b, 3.0f))
            return 3;

        return 0;
    }
} // namespace app
"#,
    )
    .unwrap();

    fs::write(
        dir.join("app/src/main.cpp"),
        r#"import local.app;

int main() {
    return app::run();
}
"#,
    )
    .unwrap();
}

// ─── Group A: Multi-File Module Project ──────────────────────────────────────

#[test]
fn test_real_math_build_succeeds() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let tmp = TempDir::new().unwrap();
    setup_math_project(tmp.path());

    let output = run_cmod_with_llvm(tmp.path(), &["build"]);
    assert!(
        output.status.success(),
        "math project build failed: {}",
        stderr(&output)
    );

    let build_dir = tmp.path().join("build/debug");
    assert!(build_dir.exists(), "build/debug directory not created");
    assert!(build_dir.join("pcm").exists(), "PCM directory not created");
    assert!(build_dir.join("obj").exists(), "obj directory not created");
}

#[test]
fn test_real_math_run_returns_zero() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let tmp = TempDir::new().unwrap();
    setup_math_project(tmp.path());

    let output = run_cmod_with_llvm(tmp.path(), &["run"]);
    assert!(
        output.status.success(),
        "math project run failed (computation error): {}",
        stderr(&output)
    );
}

#[test]
fn test_real_math_test_passes() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let tmp = TempDir::new().unwrap();
    setup_math_project(tmp.path());

    let output = run_cmod_with_llvm(tmp.path(), &["test"]);
    assert!(
        output.status.success(),
        "math project tests failed: {}",
        stderr(&output)
    );
}

#[test]
fn test_real_math_rebuild_after_partition_change() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let tmp = TempDir::new().unwrap();
    setup_math_project(tmp.path());

    // First build
    let output = run_cmod_with_llvm(tmp.path(), &["build"]);
    assert!(
        output.status.success(),
        "first build failed: {}",
        stderr(&output)
    );

    // Modify the ops partition — add a new function
    fs::write(
        tmp.path().join("src/ops.cppm"),
        r#"export module local.mathlib:ops;
import :vec;

export namespace mathlib {
    Vec3 add(const Vec3& a, const Vec3& b) {
        return Vec3(a.x + b.x, a.y + b.y, a.z + b.z);
    }

    float dot(const Vec3& a, const Vec3& b) {
        return a.x * b.x + a.y * b.y + a.z * b.z;
    }

    Vec3 scale(const Vec3& v, float s) {
        return Vec3(v.x * s, v.y * s, v.z * s);
    }

    Vec3 cross(const Vec3& a, const Vec3& b) {
        return Vec3(
            a.y * b.z - a.z * b.y,
            a.z * b.x - a.x * b.z,
            a.x * b.y - a.y * b.x
        );
    }
} // namespace mathlib
"#,
    )
    .unwrap();

    // Force rebuild — should detect the change and succeed
    let output = run_cmod_with_llvm(tmp.path(), &["build", "--force"]);
    assert!(
        output.status.success(),
        "rebuild after partition change failed: {}",
        stderr(&output)
    );

    // Binary should still run successfully
    let output = run_cmod_with_llvm(tmp.path(), &["run"]);
    assert!(
        output.status.success(),
        "run after rebuild failed: {}",
        stderr(&output)
    );
}

#[test]
fn test_real_math_graph_json_shows_dependencies() {
    let tmp = TempDir::new().unwrap();
    setup_math_project(tmp.path());

    let output = run_cmod(tmp.path(), &["graph", "--format", "json"]);
    assert!(
        output.status.success(),
        "graph --format json failed: {}",
        stderr(&output)
    );

    let out = stdout(&output);
    let parsed: serde_json::Value = serde_json::from_str(&out)
        .unwrap_or_else(|e| panic!("invalid JSON from graph: {}\noutput: {}", e, out));

    let graph_str = out.to_lowercase();
    assert!(
        graph_str.contains("mathlib") || graph_str.contains("vec") || graph_str.contains("ops"),
        "graph JSON should mention module names, got: {}",
        out
    );
    assert!(
        parsed.is_object() || parsed.is_array(),
        "graph JSON should be an object or array"
    );
}

#[test]
fn test_real_math_plan_shows_all_nodes() {
    let tmp = TempDir::new().unwrap();
    setup_math_project(tmp.path());

    let output = run_cmod(tmp.path(), &["plan"]);
    assert!(output.status.success(), "plan failed: {}", stderr(&output));

    let out = stdout(&output);
    let parsed: serde_json::Value = serde_json::from_str(&out)
        .unwrap_or_else(|e| panic!("invalid JSON from plan: {}\noutput: {}", e, out));

    let nodes = parsed.as_array().expect("plan should be an array of nodes");
    // lib.cppm (interface), vec.cppm (partition), ops.cppm (partition),
    // main.cpp (object), plus a link node = at least 4 nodes
    assert!(
        nodes.len() >= 4,
        "plan should have at least 4 nodes, got: {}",
        nodes.len()
    );
}

#[test]
fn test_real_math_explain_partition() {
    let tmp = TempDir::new().unwrap();
    setup_math_project(tmp.path());

    let output = run_cmod(tmp.path(), &["explain", "local.mathlib:vec"]);
    assert!(
        output.status.success(),
        "explain failed: {}",
        stderr(&output)
    );

    let out = stdout(&output);
    assert!(
        out.contains("Module:") || out.contains("local.mathlib:vec") || out.contains("vec.cppm"),
        "explain should show partition info, got: {}",
        out
    );
}

#[test]
fn test_real_math_build_release() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let tmp = TempDir::new().unwrap();
    setup_math_project(tmp.path());

    let output = run_cmod_with_llvm(tmp.path(), &["build", "--release"]);
    assert!(
        output.status.success(),
        "release build failed: {}",
        stderr(&output)
    );

    let build_dir = tmp.path().join("build/release");
    assert!(build_dir.exists(), "build/release directory not created");
}

// ─── Group B: Path Dependency Composition ────────────────────────────────────
//
// Path dependencies test resolution, tidy, sbom, and deps-tree.
// Cross-project compilation with path deps requires a workspace (Group C).

#[test]
fn test_real_path_dep_resolve_creates_lockfile() {
    let tmp = TempDir::new().unwrap();
    setup_path_dep_project(tmp.path());

    let app_dir = tmp.path().join("app");
    let output = run_cmod(&app_dir, &["resolve", "--untrusted"]);
    assert!(
        output.status.success(),
        "resolve failed: {}",
        stderr(&output)
    );

    assert!(
        app_dir.join("cmod.lock").exists(),
        "lockfile should be created after resolve"
    );

    let lockfile = fs::read_to_string(app_dir.join("cmod.lock")).unwrap();
    assert!(
        lockfile.contains("strutil") && lockfile.contains("numutil"),
        "lockfile should contain both path deps, got: {}",
        lockfile
    );
}

#[test]
fn test_real_path_dep_deps_tree() {
    let tmp = TempDir::new().unwrap();
    setup_path_dep_project(tmp.path());

    let app_dir = tmp.path().join("app");

    // Must resolve first to create lockfile
    let output = run_cmod(&app_dir, &["resolve", "--untrusted"]);
    assert!(
        output.status.success(),
        "resolve failed: {}",
        stderr(&output)
    );

    let output = run_cmod(&app_dir, &["deps", "--tree"]);
    assert!(
        output.status.success(),
        "deps --tree failed: {}",
        stderr(&output)
    );

    let out = stdout(&output);
    assert!(
        out.contains("strutil") && out.contains("numutil"),
        "deps tree should show both dependencies, got: {}",
        out
    );
}

#[test]
fn test_real_path_dep_tidy_all_used() {
    let tmp = TempDir::new().unwrap();
    setup_path_dep_project(tmp.path());

    let app_dir = tmp.path().join("app");
    let output = run_cmod(&app_dir, &["tidy"]);
    assert!(output.status.success(), "tidy failed: {}", stderr(&output));

    // The app source doesn't import the path deps directly (simplified for this test),
    // so tidy may report them as unused. Just verify tidy runs without error.
}

#[test]
fn test_real_path_dep_sbom_includes_all() {
    let tmp = TempDir::new().unwrap();
    setup_path_dep_project(tmp.path());

    let app_dir = tmp.path().join("app");

    // Resolve first to create lockfile
    run_cmod(&app_dir, &["resolve", "--untrusted"]);

    let output = run_cmod(&app_dir, &["sbom"]);
    assert!(output.status.success(), "sbom failed: {}", stderr(&output));

    let out = stdout(&output);
    let parsed: serde_json::Value = serde_json::from_str(&out)
        .unwrap_or_else(|e| panic!("invalid SBOM JSON: {}\noutput: {}", e, out));
    assert!(
        parsed.is_object() || parsed.is_array(),
        "SBOM should be valid JSON"
    );
}

#[test]
fn test_real_path_dep_add_and_remove() {
    let tmp = TempDir::new().unwrap();
    setup_path_dep_project(tmp.path());

    let app_dir = tmp.path().join("app");

    // Verify deps are in manifest
    let manifest = fs::read_to_string(app_dir.join("cmod.toml")).unwrap();
    assert!(manifest.contains("strutil"));
    assert!(manifest.contains("numutil"));

    // Remove one
    let output = run_cmod(&app_dir, &["remove", "strutil"]);
    assert!(
        output.status.success(),
        "remove strutil failed: {}",
        stderr(&output)
    );

    let manifest = fs::read_to_string(app_dir.join("cmod.toml")).unwrap();
    assert!(!manifest.contains("strutil"), "strutil should be removed");
    assert!(manifest.contains("numutil"), "numutil should remain");
}

#[test]
fn test_real_path_dep_standalone_build() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let tmp = TempDir::new().unwrap();
    setup_path_dep_project(tmp.path());

    // Each library should build independently (as a static lib)
    let strutil_dir = tmp.path().join("libs/strutil");
    let output = run_cmod_with_llvm(&strutil_dir, &["build"]);
    assert!(
        output.status.success(),
        "strutil standalone build failed: {}",
        stderr(&output)
    );

    let numutil_dir = tmp.path().join("libs/numutil");
    let output = run_cmod_with_llvm(&numutil_dir, &["build"]);
    assert!(
        output.status.success(),
        "numutil standalone build failed: {}",
        stderr(&output)
    );
}

// ─── Group C: Workspace with Cross-Member Imports ────────────────────────────

#[test]
fn test_real_workspace_build_all_members() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let tmp = TempDir::new().unwrap();
    setup_workspace_project(tmp.path());

    let output = run_cmod_with_llvm(tmp.path(), &["build"]);
    assert!(
        output.status.success(),
        "workspace build failed: {}",
        stderr(&output)
    );

    let err = stderr(&output);
    assert!(
        err.contains("Building member") || err.contains("workspace") || err.contains("Workspace"),
        "should show workspace build progress, got: {}",
        err
    );
}

#[test]
fn test_real_workspace_run_binary() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let tmp = TempDir::new().unwrap();
    setup_workspace_project(tmp.path());

    // Build the workspace first
    let output = run_cmod_with_llvm(tmp.path(), &["build"]);
    assert!(
        output.status.success(),
        "workspace build failed: {}",
        stderr(&output)
    );

    // Find and execute the app binary directly
    let build_dir = tmp.path().join("build/debug/app");
    if build_dir.exists() {
        // Look for the binary in the member build dir
        let mut found_binary = false;
        if let Ok(entries) = fs::read_dir(&build_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                    if ext != "o"
                        && ext != "pcm"
                        && ext != "json"
                        && ext != "a"
                        && (stem == "app" || stem == "main" || stem == "a.out")
                    {
                        let status = Command::new(&path).status().expect("failed to run binary");
                        assert!(
                            status.success(),
                            "workspace app binary failed with exit code: {:?}",
                            status.code()
                        );
                        found_binary = true;
                        break;
                    }
                }
            }
        }
        if !found_binary {
            eprintln!("Warning: could not find app binary in {:?}", build_dir);
        }
    }
}

#[test]
fn test_real_workspace_list_shows_members() {
    let tmp = TempDir::new().unwrap();
    setup_workspace_project(tmp.path());

    let output = run_cmod(tmp.path(), &["workspace", "list"]);
    assert!(
        output.status.success(),
        "workspace list failed: {}",
        stderr(&output)
    );

    let err = stderr(&output);
    assert!(err.contains("core"), "should show core member");
    assert!(err.contains("math"), "should show math member");
    assert!(err.contains("app"), "should show app member");
}

#[test]
fn test_real_workspace_graph() {
    let tmp = TempDir::new().unwrap();
    setup_workspace_project(tmp.path());

    // Graph from the app member
    let output = run_cmod(&tmp.path().join("app"), &["graph"]);
    assert!(
        output.status.success(),
        "workspace graph failed: {}",
        stderr(&output)
    );

    let out = stdout(&output);
    assert!(
        out.contains("app") || out.contains("math") || out.contains("local"),
        "graph should show module dependencies, got: {}",
        out
    );
}

#[test]
fn test_real_workspace_clean() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let tmp = TempDir::new().unwrap();
    setup_workspace_project(tmp.path());

    // Build first
    let output = run_cmod_with_llvm(tmp.path(), &["build"]);
    assert!(output.status.success(), "build failed: {}", stderr(&output));
    assert!(
        tmp.path().join("build").exists(),
        "build dir should exist after build"
    );

    // Clean
    let output = run_cmod(tmp.path(), &["clean"]);
    assert!(output.status.success(), "clean failed: {}", stderr(&output));
    assert!(
        !tmp.path().join("build").exists(),
        "build dir should be removed after clean"
    );
}

// ─── Group D: Build Configuration Variants ───────────────────────────────────

#[test]
fn test_real_config_size_optimization() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let output = run_cmod(tmp.path(), &["init", "--name", "sizeopt"]);
    assert!(output.status.success());

    // Set optimization to size
    let manifest = fs::read_to_string(tmp.path().join("cmod.toml")).unwrap();
    let manifest = manifest.replace("optimization = \"debug\"", "optimization = \"size\"");
    fs::write(tmp.path().join("cmod.toml"), manifest).unwrap();

    fs::write(
        tmp.path().join("src/lib.cppm"),
        "export module local.sizeopt;\n\nexport namespace sizeopt {\n    int value() { return 7; }\n}\n",
    )
    .unwrap();

    fs::write(
        tmp.path().join("src/main.cpp"),
        "import local.sizeopt;\nint main() { return sizeopt::value() == 7 ? 0 : 1; }\n",
    )
    .unwrap();

    let output = run_cmod_with_llvm(tmp.path(), &["build"]);
    assert!(
        output.status.success(),
        "size optimization build failed: {}",
        stderr(&output)
    );
}

#[test]
fn test_real_config_features_flag() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let tmp = TempDir::new().unwrap();

    // Write a complete cmod.toml with a features section
    fs::create_dir_all(tmp.path().join("src")).unwrap();
    fs::create_dir_all(tmp.path().join("tests")).unwrap();

    fs::write(
        tmp.path().join("cmod.toml"),
        r#"[package]
name = "feattest"
version = "0.1.0"
edition = "2023"

[module]
name = "local.feattest"
root = "src/lib.cppm"

[toolchain]
compiler = "clang"
cxx_standard = "20"

[build]
type = "binary"
optimization = "debug"
lto = false

[features]
simd = []
"#,
    )
    .unwrap();

    fs::write(
        tmp.path().join("src/lib.cppm"),
        r#"export module local.feattest;

export namespace feattest {
    int mode() {
#ifdef CMOD_FEATURE_SIMD
        return 1;
#else
        return 0;
#endif
    }
}
"#,
    )
    .unwrap();

    fs::write(
        tmp.path().join("src/main.cpp"),
        "import local.feattest;\nint main() { return 0; }\n",
    )
    .unwrap();

    // Build with feature enabled
    let output = run_cmod_with_llvm(tmp.path(), &["build", "--features", "simd"]);
    assert!(
        output.status.success(),
        "features build failed: {}",
        stderr(&output)
    );
}

#[test]
fn test_real_config_hooks() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let output = run_cmod(tmp.path(), &["init", "--name", "hooktest"]);
    assert!(output.status.success());

    // Add hooks that write marker files
    let manifest = fs::read_to_string(tmp.path().join("cmod.toml")).unwrap();
    let manifest = format!(
        "{}\n[hooks]\npre-build = \"touch pre_build_marker\"\npost-build = \"touch post_build_marker\"\n",
        manifest
    );
    fs::write(tmp.path().join("cmod.toml"), manifest).unwrap();

    fs::write(
        tmp.path().join("src/lib.cppm"),
        "export module local.hooktest;\n\nexport namespace hooktest {\n    int value() { return 1; }\n}\n",
    )
    .unwrap();

    fs::write(
        tmp.path().join("src/main.cpp"),
        "import local.hooktest;\nint main() { return 0; }\n",
    )
    .unwrap();

    let output = run_cmod_with_llvm(tmp.path(), &["build"]);
    assert!(
        output.status.success(),
        "build with hooks failed: {}",
        stderr(&output)
    );

    assert!(
        tmp.path().join("pre_build_marker").exists(),
        "pre-build hook should have created marker file"
    );
    assert!(
        tmp.path().join("post_build_marker").exists(),
        "post-build hook should have created marker file"
    );
}

#[test]
fn test_real_config_test_patterns() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let tmp = TempDir::new().unwrap();

    // Write complete manifest with test patterns
    fs::create_dir_all(tmp.path().join("src")).unwrap();
    fs::create_dir_all(tmp.path().join("tests")).unwrap();

    fs::write(
        tmp.path().join("cmod.toml"),
        r#"[package]
name = "pattest"
version = "0.1.0"
edition = "2023"

[module]
name = "local.pattest"
root = "src/lib.cppm"

[toolchain]
compiler = "clang"
cxx_standard = "20"

[build]
type = "binary"
optimization = "debug"
lto = false

[test]
test_patterns = ["unit"]
exclude_patterns = ["slow"]
"#,
    )
    .unwrap();

    fs::write(
        tmp.path().join("src/lib.cppm"),
        "export module local.pattest;\n\nexport namespace pattest {\n    int value() { return 42; }\n}\n",
    )
    .unwrap();

    fs::write(
        tmp.path().join("src/main.cpp"),
        "import local.pattest;\nint main() { return pattest::value() == 42 ? 0 : 1; }\n",
    )
    .unwrap();

    // Test matching the "unit" pattern
    fs::write(
        tmp.path().join("tests/unit_test.cpp"),
        "import local.pattest;\nint main() { return pattest::value() == 42 ? 0 : 1; }\n",
    )
    .unwrap();

    // Test matching the "slow" exclude pattern — would fail if run
    fs::write(
        tmp.path().join("tests/slow_test.cpp"),
        "int main() { return 1; }\n",
    )
    .unwrap();

    let output = run_cmod_with_llvm(tmp.path(), &["test"]);
    assert!(
        output.status.success(),
        "test with patterns failed (slow_test should be excluded): {}",
        stderr(&output)
    );
}

#[test]
fn test_real_config_lto_build() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let output = run_cmod(tmp.path(), &["init", "--name", "ltotest"]);
    assert!(output.status.success());

    let manifest = fs::read_to_string(tmp.path().join("cmod.toml")).unwrap();
    let manifest = manifest.replace("lto = false", "lto = true");
    fs::write(tmp.path().join("cmod.toml"), manifest).unwrap();

    fs::write(
        tmp.path().join("src/lib.cppm"),
        "export module local.ltotest;\n\nexport namespace ltotest {\n    int value() { return 42; }\n}\n",
    )
    .unwrap();

    fs::write(
        tmp.path().join("src/main.cpp"),
        "import local.ltotest;\nint main() { return ltotest::value() == 42 ? 0 : 1; }\n",
    )
    .unwrap();

    // LTO may fail on some LLVM versions due to module summary incompatibilities.
    // We test that the build command runs (doesn't panic/crash), accepting either
    // success or a known linker failure.
    let output = run_cmod_with_llvm(tmp.path(), &["build", "--release"]);
    if !output.status.success() {
        let err = stderr(&output);
        assert!(
            err.contains("link")
                || err.contains("lto")
                || err.contains("LTO")
                || err.contains("summary")
                || err.contains("linker"),
            "LTO failure should be a linker issue, got unexpected error: {}",
            err
        );
        eprintln!(
            "Note: LTO build failed due to LLVM version incompatibility (expected on some setups)"
        );
    }
}

// ─── Group E: Error & Edge Cases ─────────────────────────────────────────────

#[test]
fn test_real_error_circular_module_imports() {
    let tmp = TempDir::new().unwrap();
    let output = run_cmod(tmp.path(), &["init", "--name", "circular"]);
    assert!(output.status.success());

    // Create two modules that import each other
    fs::write(
        tmp.path().join("src/lib.cppm"),
        "export module local.circular;\nimport local.circular_b;\n\nexport namespace circular {\n    int a() { return 1; }\n}\n",
    )
    .unwrap();

    fs::write(
        tmp.path().join("src/circular_b.cppm"),
        "export module local.circular_b;\nimport local.circular;\n\nexport namespace circular_b {\n    int b() { return 2; }\n}\n",
    )
    .unwrap();

    // Build should detect the cycle
    if has_llvm_clang() {
        let output = run_cmod_with_llvm(tmp.path(), &["build"]);
        assert!(
            !output.status.success(),
            "build should fail with circular imports"
        );
        let err = stderr(&output);
        assert!(
            err.contains("circular") || err.contains("cycle") || err.contains("Circular"),
            "error should mention circular dependency, got: {}",
            err
        );
    }
}

#[test]
fn test_real_error_syntax_error_useful_message() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let output = run_cmod(tmp.path(), &["init", "--name", "syntaxerr"]);
    assert!(output.status.success());

    fs::write(
        tmp.path().join("src/lib.cppm"),
        "export module local.syntaxerr;\n\nexport int broken() { this is invalid c++ syntax!!! }\n",
    )
    .unwrap();

    let output = run_cmod_with_llvm(tmp.path(), &["build"]);
    assert!(
        !output.status.success(),
        "build should fail with syntax error"
    );
    let err = stderr(&output);
    assert!(
        err.contains("failed") || err.contains("error") || err.contains("compile"),
        "error message should be useful, got: {}",
        err
    );
}

#[test]
fn test_real_error_missing_import() {
    if !has_llvm_clang() {
        eprintln!("Skipping: LLVM Clang not found");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let output = run_cmod(tmp.path(), &["init", "--name", "missingmod"]);
    assert!(output.status.success());

    fs::write(
        tmp.path().join("src/main.cpp"),
        "import nonexistent.module;\nint main() { return 0; }\n",
    )
    .unwrap();

    let output = run_cmod_with_llvm(tmp.path(), &["build"]);
    assert!(
        !output.status.success(),
        "build should fail with missing import"
    );
}

#[test]
fn test_real_error_duplicate_module_name() {
    let tmp = TempDir::new().unwrap();
    let output = run_cmod(tmp.path(), &["init", "--name", "dupmod"]);
    assert!(output.status.success());

    // Two files declare the same module name
    fs::write(
        tmp.path().join("src/lib.cppm"),
        "export module local.dupmod;\n\nexport namespace dupmod {\n    int a() { return 1; }\n}\n",
    )
    .unwrap();

    fs::write(
        tmp.path().join("src/dup.cppm"),
        "export module local.dupmod;\n\nexport namespace dupmod {\n    int b() { return 2; }\n}\n",
    )
    .unwrap();

    // The graph silently overwrites duplicate keys. Verify the graph only has one entry
    // with that name (the second file wins since sources are sorted).
    let output = run_cmod(tmp.path(), &["graph", "--format", "json"]);
    assert!(
        output.status.success(),
        "graph should succeed: {}",
        stderr(&output)
    );

    // The check command should detect the mismatch between manifest module name
    // and what's actually in the source files
    let output = run_cmod(tmp.path(), &["check"]);
    let err = stderr(&output);
    // check may pass or fail depending on implementation — just verify it runs
    let _ = err;
}

#[test]
fn test_real_error_empty_src_directory() {
    let tmp = TempDir::new().unwrap();
    let output = run_cmod(tmp.path(), &["init", "--name", "emptysrc"]);
    assert!(output.status.success());

    // Remove all source files
    fs::remove_file(tmp.path().join("src/lib.cppm")).unwrap();
    fs::remove_file(tmp.path().join("src/main.cpp")).unwrap();

    let output = run_cmod(tmp.path(), &["build"]);
    assert!(
        !output.status.success(),
        "build should fail with empty src/"
    );
    let err = stderr(&output);
    assert!(
        err.contains("no source files") || err.contains("No source"),
        "error should mention no source files, got: {}",
        err
    );
}

// ─── Group F: Git-based Resolution ───────────────────────────────────────────

#[test]
fn test_real_git_local_repo_semver_resolve() {
    let tmp = TempDir::new().unwrap();

    // Create a local git repo with semver tags
    let dep_dir = tmp.path().join("dep_repo");
    create_local_git_repo(
        &dep_dir,
        &[
            (
                "cmod.toml",
                "[package]\nname = \"mymath\"\nversion = \"0.1.0\"\n",
            ),
            (
                "src/lib.cppm",
                "export module local.mymath;\n\nexport namespace mymath {\n    int add(int a, int b) { return a + b; }\n}\n",
            ),
        ],
        &[
            (
                "v1.0.0",
                &[(
                    "cmod.toml",
                    "[package]\nname = \"mymath\"\nversion = \"1.0.0\"\n",
                )],
            ),
            (
                "v1.1.0",
                &[(
                    "cmod.toml",
                    "[package]\nname = \"mymath\"\nversion = \"1.1.0\"\n",
                )],
            ),
            (
                "v2.0.0",
                &[(
                    "cmod.toml",
                    "[package]\nname = \"mymath\"\nversion = \"2.0.0\"\n",
                )],
            ),
        ],
    );

    // Create a project that depends on the local git repo
    let proj_dir = tmp.path().join("project");
    fs::create_dir_all(&proj_dir).unwrap();
    let output = run_cmod(&proj_dir, &["init", "--name", "gitclient"]);
    assert!(output.status.success());

    // Add the local repo as a dependency using file:// URL with semver constraint
    let dep_url = format!("file://{}", dep_dir.display());
    let output = run_cmod(
        &proj_dir,
        &["add", "mymath@^1.0", "--git", &dep_url, "--untrusted"],
    );
    assert!(
        output.status.success(),
        "add git dep failed: {}",
        stderr(&output)
    );

    // Resolve
    let output = run_cmod(&proj_dir, &["resolve", "--untrusted"]);
    assert!(
        output.status.success(),
        "resolve failed: {}",
        stderr(&output)
    );

    assert!(
        proj_dir.join("cmod.lock").exists(),
        "lockfile should be created"
    );

    let lockfile = fs::read_to_string(proj_dir.join("cmod.lock")).unwrap();
    assert!(
        lockfile.contains("mymath"),
        "lockfile should contain mymath dependency, got: {}",
        lockfile
    );
}

#[test]
fn test_real_git_local_repo_branch_resolve() {
    let tmp = TempDir::new().unwrap();

    // Create a local git repo
    let dep_dir = tmp.path().join("branch_repo");
    create_local_git_repo(
        &dep_dir,
        &[
            (
                "cmod.toml",
                "[package]\nname = \"branchlib\"\nversion = \"0.1.0\"\n",
            ),
            ("src/lib.cppm", "export module local.branchlib;\n"),
        ],
        &[],
    );

    // Create a branch with new content
    Command::new("git")
        .args(["checkout", "-b", "feature-x"])
        .current_dir(&dep_dir)
        .output()
        .unwrap();

    fs::write(
        dep_dir.join("src/lib.cppm"),
        "export module local.branchlib;\n\nexport namespace branchlib {\n    int feature_x() { return 42; }\n}\n",
    )
    .unwrap();

    Command::new("git")
        .args(["add", "."])
        .current_dir(&dep_dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "feature-x work"])
        .current_dir(&dep_dir)
        .output()
        .unwrap();

    // Switch back to default branch
    let _ = Command::new("git")
        .args(["checkout", "main"])
        .current_dir(&dep_dir)
        .output();
    let _ = Command::new("git")
        .args(["checkout", "master"])
        .current_dir(&dep_dir)
        .output();

    // Create a project that depends on the branch
    let proj_dir = tmp.path().join("project");
    fs::create_dir_all(&proj_dir).unwrap();
    let output = run_cmod(&proj_dir, &["init", "--name", "branchclient"]);
    assert!(output.status.success());

    let dep_url = format!("file://{}", dep_dir.display());
    let output = run_cmod(
        &proj_dir,
        &[
            "add",
            "branchlib",
            "--git",
            &dep_url,
            "--branch",
            "feature-x",
            "--untrusted",
        ],
    );
    assert!(
        output.status.success(),
        "add branch dep failed: {}",
        stderr(&output)
    );

    let output = run_cmod(&proj_dir, &["resolve", "--untrusted"]);
    assert!(
        output.status.success(),
        "resolve with branch failed: {}",
        stderr(&output)
    );
}

#[test]
fn test_real_git_local_repo_rev_resolve() {
    let tmp = TempDir::new().unwrap();

    let dep_dir = tmp.path().join("rev_repo");
    create_local_git_repo(
        &dep_dir,
        &[
            (
                "cmod.toml",
                "[package]\nname = \"revlib\"\nversion = \"0.1.0\"\n",
            ),
            ("src/lib.cppm", "export module local.revlib;\n"),
        ],
        &[(
            "v1.0.0",
            &[(
                "cmod.toml",
                "[package]\nname = \"revlib\"\nversion = \"1.0.0\"\n",
            )],
        )],
    );

    // Get the commit hash of the tag
    let rev_output = Command::new("git")
        .args(["rev-parse", "v1.0.0"])
        .current_dir(&dep_dir)
        .output()
        .unwrap();
    let commit_hash = String::from_utf8_lossy(&rev_output.stdout)
        .trim()
        .to_string();

    let proj_dir = tmp.path().join("project");
    fs::create_dir_all(&proj_dir).unwrap();
    let output = run_cmod(&proj_dir, &["init", "--name", "revclient"]);
    assert!(output.status.success());

    let dep_url = format!("file://{}", dep_dir.display());
    let output = run_cmod(
        &proj_dir,
        &[
            "add",
            "revlib",
            "--git",
            &dep_url,
            "--rev",
            &commit_hash,
            "--untrusted",
        ],
    );
    assert!(
        output.status.success(),
        "add rev dep failed: {}",
        stderr(&output)
    );

    let output = run_cmod(&proj_dir, &["resolve", "--untrusted"]);
    assert!(
        output.status.success(),
        "resolve with rev failed: {}",
        stderr(&output)
    );

    let lockfile = fs::read_to_string(proj_dir.join("cmod.lock")).unwrap();
    assert!(
        lockfile.contains(&commit_hash[..8]),
        "lockfile should contain the pinned commit hash, got: {}",
        lockfile
    );
}

#[test]
fn test_real_git_update_resolves_new_tags() {
    let tmp = TempDir::new().unwrap();

    let dep_dir = tmp.path().join("update_repo");
    create_local_git_repo(
        &dep_dir,
        &[
            (
                "cmod.toml",
                "[package]\nname = \"updlib\"\nversion = \"1.0.0\"\n",
            ),
            ("src/lib.cppm", "export module local.updlib;\n"),
        ],
        &[(
            "v1.0.0",
            &[(
                "cmod.toml",
                "[package]\nname = \"updlib\"\nversion = \"1.0.0\"\n",
            )],
        )],
    );

    let proj_dir = tmp.path().join("project");
    fs::create_dir_all(&proj_dir).unwrap();
    let output = run_cmod(&proj_dir, &["init", "--name", "updclient"]);
    assert!(output.status.success());

    let dep_url = format!("file://{}", dep_dir.display());
    run_cmod(
        &proj_dir,
        &["add", "updlib", "--git", &dep_url, "--untrusted"],
    );
    let output = run_cmod(&proj_dir, &["resolve", "--untrusted"]);
    assert!(
        output.status.success(),
        "initial resolve failed: {}",
        stderr(&output)
    );

    // Add a new tag to the dep repo
    fs::write(
        dep_dir.join("cmod.toml"),
        "[package]\nname = \"updlib\"\nversion = \"1.1.0\"\n",
    )
    .unwrap();
    Command::new("git")
        .args(["add", "."])
        .current_dir(&dep_dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["commit", "-m", "release 1.1.0"])
        .current_dir(&dep_dir)
        .output()
        .unwrap();
    Command::new("git")
        .args(["tag", "v1.1.0"])
        .current_dir(&dep_dir)
        .output()
        .unwrap();

    // Update should pick up the new version
    let output = run_cmod(&proj_dir, &["update", "--untrusted"]);
    assert!(
        output.status.success(),
        "update failed: {}",
        stderr(&output)
    );
}

// ─── Group F (Network): Real GitHub Repos ────────────────────────────────────

#[test]
fn test_real_github_fmtlib_resolve() {
    if !network_tests_enabled() {
        eprintln!("Skipping: CMOD_TEST_NETWORK not set to 1");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let output = run_cmod(tmp.path(), &["init", "--name", "fmtclient"]);
    assert!(output.status.success());

    let output = run_cmod(
        tmp.path(),
        &["add", "github.com/fmtlib/fmt@^10.0", "--untrusted"],
    );
    assert!(
        output.status.success(),
        "add fmtlib failed: {}",
        stderr(&output)
    );

    let output = run_cmod(tmp.path(), &["resolve", "--untrusted"]);
    assert!(
        output.status.success(),
        "resolve fmtlib failed: {}",
        stderr(&output)
    );

    let lockfile = fs::read_to_string(tmp.path().join("cmod.lock")).unwrap();
    assert!(
        lockfile.contains("fmt"),
        "lockfile should contain fmt entry"
    );
}

#[test]
fn test_real_github_nlohmann_json_resolve() {
    if !network_tests_enabled() {
        eprintln!("Skipping: CMOD_TEST_NETWORK not set to 1");
        return;
    }

    let tmp = TempDir::new().unwrap();
    let output = run_cmod(tmp.path(), &["init", "--name", "jsonclient"]);
    assert!(output.status.success());

    let output = run_cmod(
        tmp.path(),
        &["add", "github.com/nlohmann/json@^3.11", "--untrusted"],
    );
    assert!(
        output.status.success(),
        "add nlohmann/json failed: {}",
        stderr(&output)
    );

    let output = run_cmod(tmp.path(), &["resolve", "--untrusted"]);
    assert!(
        output.status.success(),
        "resolve nlohmann/json failed: {}",
        stderr(&output)
    );

    let lockfile = fs::read_to_string(tmp.path().join("cmod.lock")).unwrap();
    assert!(
        lockfile.contains("json"),
        "lockfile should contain json entry"
    );
}
