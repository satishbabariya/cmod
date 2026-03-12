use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use cmod_core::error::CmodError;
use cmod_core::manifest::{default_manifest, Build, Compat, Manifest, Module, Package, Toolchain};
use cmod_core::shell::Shell;
use cmod_core::types::{BuildType, Compiler};

/// Information extracted from a CMakeLists.txt file.
#[derive(Debug, Default)]
struct CmakeInfo {
    project_name: Option<String>,
    project_version: Option<String>,
    /// Versions discovered via set(<NAME>_VERSION ...) or set(PROJECT_VERSION ...).
    /// Keyed by the exact CMake variable name (e.g. "SPDLOG_VERSION", "PROJECT_VERSION").
    set_versions: HashMap<String, String>,
    cxx_standard: Option<String>,
    /// All C++ standards seen (for picking the highest).
    all_cxx_standards: Vec<String>,
    build_type: Option<BuildType>,
    /// Whether an add_library was seen (takes priority over add_executable).
    has_library: bool,
    sources: Vec<String>,
    include_dirs: Vec<String>,
    extra_flags: Vec<String>,
    linked_libraries: Vec<String>,
    packages: Vec<String>,
    has_tests: bool,
    subdirectories: Vec<String>,
}

/// Run the CMake migration: parse CMakeLists.txt and generate cmod.toml.
pub fn run(path: Option<PathBuf>, shell: &Shell) -> Result<(), CmodError> {
    let project_dir = match path {
        Some(p) => {
            if p.is_absolute() {
                p
            } else {
                std::env::current_dir()?.join(p)
            }
        }
        None => std::env::current_dir()?,
    };

    let cmake_path = project_dir.join("CMakeLists.txt");
    if !cmake_path.exists() {
        return Err(CmodError::InvalidManifest {
            reason: format!("No CMakeLists.txt found at {}", cmake_path.display()),
        });
    }

    let cmod_toml_path = project_dir.join("cmod.toml");
    if cmod_toml_path.exists() {
        return Err(CmodError::InvalidManifest {
            reason: "cmod.toml already exists. Remove it first to re-migrate.".to_string(),
        });
    }

    shell.status("Migrating", format!("from {}", cmake_path.display()));

    let content = std::fs::read_to_string(&cmake_path)?;
    let info = parse_cmake(&content);

    let name = info
        .project_name
        .clone()
        .unwrap_or_else(|| dir_name(&project_dir));

    if let Some(ref n) = info.project_name {
        let version_str = info.project_version.as_deref().unwrap_or("0.1.0");
        shell.status("Detected", format!("project: {} v{}", n, version_str));
    }

    let build_type = info.build_type.unwrap_or(BuildType::Binary);
    shell.status(
        "Detected",
        format!("build type: {}", build_type_label(build_type)),
    );

    if let Some(ref std) = info.cxx_standard {
        shell.status("Detected", format!("C++ standard: {}", std));
    }

    if !info.sources.is_empty() {
        shell.status(
            "Found",
            format!("{} source file(s) in CMake config", info.sources.len()),
        );
    }

    // Build manifest from extracted info.
    let manifest = build_manifest(&info, &name);

    // Write cmod.toml.
    let toml_str = manifest.to_toml_string()?;

    // Append TODO comments for dependencies that need manual mapping.
    let final_content = append_migration_comments(&toml_str, &info);
    std::fs::write(&cmod_toml_path, &final_content)?;

    shell.status("Generated", "cmod.toml");

    // Create src/ directory if missing.
    let src_dir = project_dir.join("src");
    if !src_dir.exists() {
        std::fs::create_dir_all(&src_dir)?;
        shell.status("Created", "src/ directory");
    }

    // Scan for existing C++ source files.
    let existing_sources = scan_cpp_sources(&project_dir);
    if !existing_sources.is_empty() {
        shell.verbose(
            "Found",
            format!("{} existing C++ source file(s)", existing_sources.len()),
        );
    }

    // Print warnings and notes.
    if !info.packages.is_empty() {
        shell.warn(format!(
            "{} find_package() call(s) need manual dependency mapping (see TODOs in cmod.toml)",
            info.packages.len()
        ));
    }

    if !info.subdirectories.is_empty() {
        shell.warn(format!(
            "{} add_subdirectory() call(s) detected — subdirectories are not migrated automatically",
            info.subdirectories.len()
        ));
        for sub in &info.subdirectories {
            shell.verbose("Subdirectory", sub);
        }
    }

    shell.note("Add C++20 module declarations to your source files");
    shell.note("Run `cmod build` to verify the migration");

    Ok(())
}

/// Build a `Manifest` from parsed CMake information.
fn build_manifest(info: &CmakeInfo, name: &str) -> Manifest {
    let mut manifest = default_manifest(name);

    // Package.
    let version = info
        .project_version
        .clone()
        .or_else(|| {
            // Fall back to PROJECT_VERSION or <NAME>_VERSION from set() calls.
            info.set_versions
                .get("PROJECT_VERSION")
                .or_else(|| {
                    let upper_name = name.to_uppercase().replace('-', "_");
                    info.set_versions.get(&format!("{}_VERSION", upper_name))
                })
                .cloned()
        })
        .unwrap_or_else(|| "0.1.0".to_string());
    manifest.package = Package {
        name: name.to_string(),
        version,
        edition: Some("2023".to_string()),
        description: None,
        authors: vec![],
        license: None,
        repository: None,
        homepage: None,
    };

    // Module.
    let module_root = if info.build_type == Some(BuildType::Binary) {
        "src/main.cppm"
    } else {
        "src/lib.cppm"
    };
    manifest.module = Some(Module {
        name: format!("local.{}", name.replace('-', "_")),
        root: PathBuf::from(module_root),
    });

    // Toolchain — only set cxx_standard when the value is a concrete number.
    let resolved_std = info
        .cxx_standard
        .as_deref()
        .filter(|s| !s.contains("${") && s.chars().all(|c| c.is_ascii_digit()))
        .map(|s| s.to_string());
    let cxx_std = resolved_std.unwrap_or_else(|| "20".to_string());
    manifest.toolchain = Some(Toolchain {
        compiler: Some(Compiler::Clang),
        version: None,
        cxx_standard: Some(cxx_std.clone()),
        stdlib: None,
        target: None,
        sysroot: None,
    });

    // Compat.
    manifest.compat = Some(Compat {
        cpp: Some(format!(">={}", cxx_std)),
        llvm: None,
        abi: None,
        platforms: vec![],
    });

    // Build.
    let build_type = info.build_type.unwrap_or(BuildType::Binary);
    manifest.build = Some(Build {
        build_type: Some(build_type),
        optimization: None,
        lto: None,
        parallel: Some(true),
        incremental: Some(true),
        include_dirs: info.include_dirs.clone(),
        extra_flags: info.extra_flags.clone(),
        sources: Vec::new(),
        exclude: Vec::new(),
        distributed: None,
    });

    // Dependencies remain empty — users must manually map find_package() to Git URLs.
    manifest.dependencies = BTreeMap::new();

    manifest
}

/// Append TOML comments for manual migration steps.
fn append_migration_comments(toml: &str, info: &CmakeInfo) -> String {
    let mut result = toml.to_string();

    if !info.packages.is_empty() || !info.linked_libraries.is_empty() {
        result.push_str("\n# ==========================================================\n");
        result.push_str("# TODO: Manual dependency mapping needed\n");
        result.push_str("# ==========================================================\n");

        for pkg in &info.packages {
            result.push_str(&format!(
                "# find_package({}) -> add Git URL to [dependencies]\n",
                pkg
            ));
            if let Some(hint) = well_known_package_hint(pkg) {
                result.push_str(&format!("#   e.g. {} = \"^1.0\"\n", hint));
            }
        }

        if !info.linked_libraries.is_empty() {
            result.push_str("# Linked libraries: ");
            result.push_str(&info.linked_libraries.join(", "));
            result.push('\n');
        }
    }

    result
}

/// Provide Git URL hints for well-known CMake packages.
fn well_known_package_hint(pkg: &str) -> Option<&'static str> {
    match pkg.to_lowercase().as_str() {
        "fmt" => Some("\"github.com/fmtlib/fmt\""),
        "nlohmann_json" | "json" => Some("\"github.com/nlohmann/json\""),
        "spdlog" => Some("\"github.com/gabime/spdlog\""),
        "catch2" => Some("\"github.com/catchorg/Catch2\""),
        "gtest" | "googletest" => Some("\"github.com/google/googletest\""),
        "benchmark" => Some("\"github.com/google/benchmark\""),
        "abseil" | "absl" => Some("\"github.com/abseil/abseil-cpp\""),
        "boost" => Some("\"github.com/boostorg/boost\""),
        "protobuf" => Some("\"github.com/protocolbuffers/protobuf\""),
        "grpc" => Some("\"github.com/grpc/grpc\""),
        "zlib" => Some("\"github.com/madler/zlib\""),
        "openssl" => Some("\"github.com/openssl/openssl\""),
        "curl" | "libcurl" => Some("\"github.com/curl/curl\""),
        "eigen3" | "eigen" => Some("\"gitlab.com/libeigen/eigen\""),
        _ => None,
    }
}

// ---------------------------------------------------------------------------
// CMake parser
// ---------------------------------------------------------------------------

/// Parse a CMakeLists.txt file and extract build information.
fn parse_cmake(content: &str) -> CmakeInfo {
    let mut info = CmakeInfo::default();

    // Join continuation lines (backslash at end of line) and collect commands.
    let joined = join_continuation_lines(content);
    let commands = extract_commands(&joined);

    for (cmd_name, args_str) in &commands {
        match cmd_name.to_lowercase().as_str() {
            "project" => parse_project(args_str, &mut info),
            "set" => parse_set(args_str, &mut info),
            "add_executable" => parse_add_target(args_str, &mut info, BuildType::Binary),
            "add_library" => parse_add_library(args_str, &mut info),
            "find_package" => parse_find_package(args_str, &mut info),
            "target_link_libraries" => parse_target_link_libraries(args_str, &mut info),
            "target_compile_options" => parse_target_compile_options(args_str, &mut info),
            "target_include_directories" => {
                parse_target_include_directories(args_str, &mut info);
            }
            "target_compile_features" => parse_target_compile_features(args_str, &mut info),
            "enable_testing" => {
                info.has_tests = true;
            }
            "add_subdirectory" => {
                let tokens = tokenize_args(args_str);
                if let Some(dir) = tokens.first() {
                    info.subdirectories.push(dir.clone());
                }
            }
            _ => {}
        }
    }

    // Post-process: resolve version if it contains ${VAR}.
    if let Some(ref ver) = info.project_version {
        if let Some(var_name) = extract_variable_name(ver) {
            // Look up the exact variable referenced, then fall back to
            // PROJECT_VERSION or <PROJECT_NAME>_VERSION.
            let resolved = info
                .set_versions
                .get(&var_name)
                .or_else(|| info.set_versions.get("PROJECT_VERSION"))
                .or_else(|| {
                    info.project_name.as_ref().and_then(|pn| {
                        info.set_versions
                            .get(&format!("{}_VERSION", pn.to_uppercase()))
                    })
                })
                .cloned();
            info.project_version = resolved;
        }
    }

    // Post-process: pick the highest C++ standard seen.
    if !info.all_cxx_standards.is_empty() {
        let best = info
            .all_cxx_standards
            .iter()
            .filter_map(|s| s.parse::<u32>().ok())
            .max();
        if let Some(best_std) = best {
            info.cxx_standard = Some(best_std.to_string());
        }
    }

    // Post-process: if a library was seen, prefer library build type.
    // Many projects have both add_library (the actual product) and add_executable (examples/tests).
    if info.has_library && info.build_type == Some(BuildType::Binary) {
        info.build_type = Some(BuildType::StaticLib);
    }

    // Post-process: filter out MSVC-style flags (/flag) since cmod targets Clang,
    // and flags containing unresolved CMake variables (${...}).
    info.extra_flags
        .retain(|f| !f.starts_with('/') && !f.contains("${"));

    // Same for include_dirs — filter unresolved variables.
    info.include_dirs.retain(|d| !d.contains("${"));

    info
}

/// Join backslash-continued lines into single logical lines.
fn join_continuation_lines(content: &str) -> String {
    let mut result = String::with_capacity(content.len());
    let mut continuation = false;

    for line in content.lines() {
        let trimmed = line.trim_end();
        if continuation {
            result.push(' ');
            if let Some(stripped) = trimmed.strip_suffix('\\') {
                result.push_str(stripped);
            } else {
                result.push_str(trimmed);
                continuation = false;
            }
        } else if let Some(stripped) = trimmed.strip_suffix('\\') {
            result.push_str(stripped);
            continuation = true;
        } else {
            result.push_str(trimmed);
            result.push('\n');
        }
    }

    result
}

/// Extract top-level CMake commands as (name, args_str) pairs.
/// Handles multi-line commands by tracking parenthesis nesting.
fn extract_commands(content: &str) -> Vec<(String, String)> {
    let mut commands = Vec::new();
    let mut chars = content.chars().peekable();
    let mut in_comment = false;

    while let Some(&ch) = chars.peek() {
        if ch == '#' {
            in_comment = true;
            chars.next();
            continue;
        }
        if ch == '\n' {
            in_comment = false;
            chars.next();
            continue;
        }
        if in_comment {
            chars.next();
            continue;
        }
        if ch.is_whitespace() {
            chars.next();
            continue;
        }

        // Try to read a command name.
        if ch.is_alphanumeric() || ch == '_' {
            let mut name = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_alphanumeric() || c == '_' {
                    name.push(c);
                    chars.next();
                } else {
                    break;
                }
            }

            // Skip whitespace before '('.
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() && c != '\n' {
                    chars.next();
                } else {
                    break;
                }
            }

            if chars.peek() == Some(&'(') {
                chars.next(); // consume '('
                let mut depth = 1;
                let mut args = String::new();
                let mut in_quotes = false;
                while let Some(&c) = chars.peek() {
                    chars.next();
                    if c == '"' {
                        in_quotes = !in_quotes;
                        args.push(c);
                    } else if in_quotes {
                        // Inside quotes, parentheses and # are literal.
                        args.push(c);
                    } else if c == '#' {
                        // Inline comment: skip until end of line.
                        // Replace with a space so adjacent tokens stay separated.
                        args.push(' ');
                        while let Some(&nc) = chars.peek() {
                            if nc == '\n' {
                                break;
                            }
                            chars.next();
                        }
                    } else if c == '(' {
                        depth += 1;
                        args.push(c);
                    } else if c == ')' {
                        depth -= 1;
                        if depth == 0 {
                            break;
                        }
                        args.push(c);
                    } else {
                        args.push(c);
                    }
                }
                commands.push((name, args));
            }
            // If no '(' follows, it's not a command — skip.
        } else {
            chars.next();
        }
    }

    commands
}

/// Split a CMake argument string into tokens, respecting quoted strings.
fn tokenize_args(args: &str) -> Vec<String> {
    let mut tokens = Vec::new();
    let mut chars = args.chars().peekable();

    while let Some(&ch) = chars.peek() {
        if ch.is_whitespace() || ch == '\n' {
            chars.next();
            continue;
        }

        if ch == '"' {
            chars.next(); // consume opening quote
            let mut token = String::new();
            while let Some(&c) = chars.peek() {
                chars.next();
                if c == '"' {
                    break;
                }
                token.push(c);
            }
            tokens.push(token);
        } else {
            let mut token = String::new();
            while let Some(&c) = chars.peek() {
                if c.is_whitespace() || c == '\n' {
                    break;
                }
                token.push(c);
                chars.next();
            }
            tokens.push(token);
        }
    }

    tokens
}

fn parse_project(args: &str, info: &mut CmakeInfo) {
    let tokens = tokenize_args(args);
    if tokens.is_empty() {
        return;
    }

    info.project_name = Some(tokens[0].clone());

    // Look for VERSION keyword.
    for i in 1..tokens.len() {
        if tokens[i].eq_ignore_ascii_case("VERSION") {
            if let Some(ver) = tokens.get(i + 1) {
                info.project_version = Some(ver.clone());
            }
            break;
        }
    }
}

fn parse_set(args: &str, info: &mut CmakeInfo) {
    let tokens = tokenize_args(args);
    if tokens.len() < 2 {
        return;
    }

    let var_name = &tokens[0];
    let value = &tokens[1];

    if var_name == "CMAKE_CXX_STANDARD" {
        // Track all standards seen; post-processing picks the highest.
        // Only store concrete numeric values — skip unresolved variables.
        if !value.contains("${") && value.chars().all(|c| c.is_ascii_digit()) {
            info.all_cxx_standards.push(value.clone());
            info.cxx_standard = Some(value.clone());
        }
    }

    // Capture set(<NAME>_VERSION ...) and set(PROJECT_VERSION ...) as version hints.
    if (var_name.ends_with("_VERSION") || var_name == "PROJECT_VERSION") && !value.contains("${") {
        // Only capture version-like values (digits and dots).
        if value.chars().all(|c| c.is_ascii_digit() || c == '.') && value.contains('.') {
            info.set_versions.insert(var_name.clone(), value.clone());
        }
    }
}

fn parse_add_target(args: &str, info: &mut CmakeInfo, bt: BuildType) {
    let tokens = tokenize_args(args);
    if tokens.is_empty() {
        return;
    }

    info.build_type = Some(bt);

    // Remaining tokens (after target name) are source files,
    // skipping CMake keywords.
    let cmake_keywords = [
        "WIN32",
        "MACOSX_BUNDLE",
        "EXCLUDE_FROM_ALL",
        "IMPORTED",
        "ALIAS",
    ];
    for token in &tokens[1..] {
        if cmake_keywords.contains(&token.as_str()) {
            continue;
        }
        if token.starts_with('$') {
            continue; // skip variable references
        }
        info.sources.push(token.clone());
    }
}

fn parse_add_library(args: &str, info: &mut CmakeInfo) {
    let tokens = tokenize_args(args);
    if tokens.is_empty() {
        return;
    }

    // Determine library type.
    let mut bt = BuildType::StaticLib;
    let mut source_start = 1;
    let mut is_concrete = true;

    let lib_keywords = [
        "STATIC",
        "SHARED",
        "MODULE",
        "OBJECT",
        "INTERFACE",
        "IMPORTED",
        "ALIAS",
        "EXCLUDE_FROM_ALL",
    ];

    for (i, token) in tokens.iter().enumerate().skip(1) {
        match token.as_str() {
            "SHARED" => {
                bt = BuildType::SharedLib;
                source_start = i + 1;
            }
            "STATIC" => {
                bt = BuildType::StaticLib;
                source_start = i + 1;
            }
            "MODULE" | "OBJECT" => {
                source_start = i + 1;
            }
            "INTERFACE" | "IMPORTED" | "ALIAS" => {
                // Non-concrete targets — skip source collection entirely.
                is_concrete = false;
                break;
            }
            _ => {
                if !lib_keywords.contains(&token.as_str()) {
                    // First non-keyword after name is start of sources.
                    source_start = i;
                    break;
                }
            }
        }
    }

    if !is_concrete {
        return;
    }

    info.has_library = true;
    info.build_type = Some(bt);

    for token in &tokens[source_start..] {
        if lib_keywords.contains(&token.as_str()) || token.starts_with('$') {
            continue;
        }
        info.sources.push(token.clone());
    }
}

fn parse_find_package(args: &str, info: &mut CmakeInfo) {
    let tokens = tokenize_args(args);
    if let Some(pkg) = tokens.first() {
        info.packages.push(pkg.clone());
    }
}

fn parse_target_link_libraries(args: &str, info: &mut CmakeInfo) {
    let tokens = tokenize_args(args);
    let visibility_keywords = ["PUBLIC", "PRIVATE", "INTERFACE"];

    // Skip target name (first token), then collect non-keyword tokens.
    for token in tokens.iter().skip(1) {
        if visibility_keywords.contains(&token.as_str()) || token.starts_with('$') {
            continue;
        }
        info.linked_libraries.push(token.clone());
    }
}

fn parse_target_compile_options(args: &str, info: &mut CmakeInfo) {
    let tokens = tokenize_args(args);
    let visibility_keywords = ["PUBLIC", "PRIVATE", "INTERFACE"];

    for token in tokens.iter().skip(1) {
        if visibility_keywords.contains(&token.as_str()) || token.starts_with('$') {
            continue;
        }
        info.extra_flags.push(token.clone());
    }
}

fn parse_target_include_directories(args: &str, info: &mut CmakeInfo) {
    let tokens = tokenize_args(args);
    let keywords = [
        "PUBLIC",
        "PRIVATE",
        "INTERFACE",
        "SYSTEM",
        "BEFORE",
        "AFTER",
    ];

    for token in tokens.iter().skip(1) {
        if keywords.contains(&token.as_str()) || token.starts_with('$') {
            continue;
        }
        info.include_dirs.push(token.clone());
    }
}

fn parse_target_compile_features(args: &str, info: &mut CmakeInfo) {
    let tokens = tokenize_args(args);
    for token in &tokens {
        if let Some(stripped) = token.strip_prefix("cxx_std_") {
            info.all_cxx_standards.push(stripped.to_string());
            info.cxx_standard = Some(stripped.to_string());
        }
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Extract the variable name from a CMake `${VAR}` reference.
/// Returns `None` if the string does not contain a `${...}` pattern.
fn extract_variable_name(s: &str) -> Option<String> {
    let start = s.find("${")?;
    let rest = &s[start + 2..];
    let end = rest.find('}')?;
    Some(rest[..end].to_string())
}

fn dir_name(path: &Path) -> String {
    path.file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("project")
        .to_string()
}

fn build_type_label(bt: BuildType) -> &'static str {
    match bt {
        BuildType::Binary => "binary",
        BuildType::StaticLib => "static-lib",
        BuildType::SharedLib => "shared-lib",
    }
}

/// Scan a directory for C++ source files.
fn scan_cpp_sources(dir: &Path) -> Vec<PathBuf> {
    let extensions = ["cpp", "cxx", "cc", "cppm", "ixx", "c++"];
    let mut result = Vec::new();

    fn walk(dir: &Path, exts: &[&str], result: &mut Vec<PathBuf>) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    // Skip build directories.
                    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
                    if name == "build"
                        || name == "cmake-build-debug"
                        || name == "cmake-build-release"
                        || name.starts_with('.')
                    {
                        continue;
                    }
                    walk(&path, exts, result);
                } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if exts.contains(&ext) {
                        result.push(path);
                    }
                }
            }
        }
    }

    walk(dir, &extensions, &mut result);
    result.sort();
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cmake_project() {
        let info = parse_cmake("project(myapp VERSION 1.2.3)");
        assert_eq!(info.project_name.as_deref(), Some("myapp"));
        assert_eq!(info.project_version.as_deref(), Some("1.2.3"));
    }

    #[test]
    fn test_parse_cmake_project_no_version() {
        let info = parse_cmake("project(myapp)");
        assert_eq!(info.project_name.as_deref(), Some("myapp"));
        assert_eq!(info.project_version, None);
    }

    #[test]
    fn test_parse_cmake_executable() {
        let info = parse_cmake("add_executable(myapp src/main.cpp src/utils.cpp)");
        assert_eq!(info.build_type, Some(BuildType::Binary));
        assert_eq!(info.sources, vec!["src/main.cpp", "src/utils.cpp"]);
    }

    #[test]
    fn test_parse_cmake_static_library() {
        let info = parse_cmake("add_library(mylib STATIC src/lib.cpp)");
        assert_eq!(info.build_type, Some(BuildType::StaticLib));
        assert_eq!(info.sources, vec!["src/lib.cpp"]);
    }

    #[test]
    fn test_parse_cmake_shared_library() {
        let info = parse_cmake("add_library(mylib SHARED src/lib.cpp)");
        assert_eq!(info.build_type, Some(BuildType::SharedLib));
        assert_eq!(info.sources, vec!["src/lib.cpp"]);
    }

    #[test]
    fn test_parse_cmake_cxx_standard() {
        let info = parse_cmake("set(CMAKE_CXX_STANDARD 20)");
        assert_eq!(info.cxx_standard.as_deref(), Some("20"));
    }

    #[test]
    fn test_parse_cmake_compile_features() {
        let info = parse_cmake("target_compile_features(myapp PRIVATE cxx_std_23)");
        assert_eq!(info.cxx_standard.as_deref(), Some("23"));
    }

    #[test]
    fn test_parse_cmake_find_package() {
        let info = parse_cmake("find_package(fmt REQUIRED)\nfind_package(Boost 1.80)");
        assert_eq!(info.packages, vec!["fmt", "Boost"]);
    }

    #[test]
    fn test_parse_cmake_compile_options() {
        let info = parse_cmake("target_compile_options(myapp PRIVATE -Wall -Wextra)");
        assert_eq!(info.extra_flags, vec!["-Wall", "-Wextra"]);
    }

    #[test]
    fn test_parse_cmake_include_dirs() {
        let info = parse_cmake("target_include_directories(myapp PUBLIC include PRIVATE src)");
        assert_eq!(info.include_dirs, vec!["include", "src"]);
    }

    #[test]
    fn test_parse_cmake_link_libraries() {
        let info = parse_cmake(
            "target_link_libraries(myapp PRIVATE fmt::fmt nlohmann_json::nlohmann_json)",
        );
        assert_eq!(
            info.linked_libraries,
            vec!["fmt::fmt", "nlohmann_json::nlohmann_json"]
        );
    }

    #[test]
    fn test_parse_cmake_enable_testing() {
        let info = parse_cmake("enable_testing()");
        assert!(info.has_tests);
    }

    #[test]
    fn test_parse_cmake_subdirectory() {
        let info = parse_cmake("add_subdirectory(libs/core)\nadd_subdirectory(libs/util)");
        assert_eq!(info.subdirectories, vec!["libs/core", "libs/util"]);
    }

    #[test]
    fn test_parse_cmake_multiline() {
        let cmake = "\
add_executable(myapp
    src/main.cpp
    src/utils.cpp
    src/core.cpp
)";
        let info = parse_cmake(cmake);
        assert_eq!(info.build_type, Some(BuildType::Binary));
        assert_eq!(
            info.sources,
            vec!["src/main.cpp", "src/utils.cpp", "src/core.cpp"]
        );
    }

    #[test]
    fn test_parse_cmake_comments_ignored() {
        let cmake = "\
# This is a comment
project(myapp VERSION 2.0.0)
# Another comment
add_executable(myapp src/main.cpp)
";
        let info = parse_cmake(cmake);
        assert_eq!(info.project_name.as_deref(), Some("myapp"));
        assert_eq!(info.project_version.as_deref(), Some("2.0.0"));
    }

    #[test]
    fn test_parse_cmake_variable_references_skipped() {
        let cmake = "add_executable(myapp ${SOURCES} src/main.cpp)";
        let info = parse_cmake(cmake);
        // Variable references are skipped, only literal sources captured.
        assert_eq!(info.sources, vec!["src/main.cpp"]);
    }

    #[test]
    fn test_parse_cmake_full_example() {
        let cmake = "\
cmake_minimum_required(VERSION 3.20)
project(myapp VERSION 1.2.3 LANGUAGES CXX)

set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD_REQUIRED ON)

find_package(fmt REQUIRED)
find_package(nlohmann_json REQUIRED)

add_executable(myapp
    src/main.cpp
    src/parser.cpp
    src/engine.cpp
)

target_include_directories(myapp PRIVATE include)
target_compile_options(myapp PRIVATE -Wall -Wextra -Wpedantic)
target_link_libraries(myapp PRIVATE fmt::fmt nlohmann_json::nlohmann_json)

enable_testing()
add_test(NAME unit_tests COMMAND myapp_test)
";
        let info = parse_cmake(cmake);
        assert_eq!(info.project_name.as_deref(), Some("myapp"));
        assert_eq!(info.project_version.as_deref(), Some("1.2.3"));
        assert_eq!(info.cxx_standard.as_deref(), Some("20"));
        assert_eq!(info.build_type, Some(BuildType::Binary));
        assert_eq!(info.sources.len(), 3);
        assert_eq!(info.packages, vec!["fmt", "nlohmann_json"]);
        assert_eq!(info.include_dirs, vec!["include"]);
        assert_eq!(info.extra_flags, vec!["-Wall", "-Wextra", "-Wpedantic"]);
        assert_eq!(
            info.linked_libraries,
            vec!["fmt::fmt", "nlohmann_json::nlohmann_json"]
        );
        assert!(info.has_tests);
    }

    #[test]
    fn test_parse_cmake_continuation_lines() {
        let cmake = "\
add_executable(myapp \\\n\
    src/main.cpp \\\n\
    src/utils.cpp)
";
        let info = parse_cmake(cmake);
        assert_eq!(info.build_type, Some(BuildType::Binary));
        assert_eq!(info.sources, vec!["src/main.cpp", "src/utils.cpp"]);
    }

    #[test]
    fn test_build_manifest_basic() {
        let info = CmakeInfo {
            project_name: Some("myapp".to_string()),
            project_version: Some("1.0.0".to_string()),
            cxx_standard: Some("20".to_string()),
            build_type: Some(BuildType::Binary),
            ..Default::default()
        };
        let manifest = build_manifest(&info, "myapp");
        assert_eq!(manifest.package.name, "myapp");
        assert_eq!(manifest.package.version, "1.0.0");
        assert_eq!(
            manifest.toolchain.as_ref().unwrap().cxx_standard.as_deref(),
            Some("20")
        );
        assert_eq!(
            manifest.build.as_ref().unwrap().build_type,
            Some(BuildType::Binary)
        );
    }

    #[test]
    fn test_well_known_package_hints() {
        assert!(well_known_package_hint("fmt").is_some());
        assert!(well_known_package_hint("nlohmann_json").is_some());
        assert!(well_known_package_hint("GTest").is_some());
        assert!(well_known_package_hint("unknown_pkg").is_none());
    }

    #[test]
    fn test_append_migration_comments() {
        let toml = "[package]\nname = \"test\"\n";
        let info = CmakeInfo {
            packages: vec!["fmt".to_string()],
            linked_libraries: vec!["fmt::fmt".to_string()],
            ..Default::default()
        };
        let result = append_migration_comments(toml, &info);
        assert!(result.contains("TODO: Manual dependency mapping needed"));
        assert!(result.contains("find_package(fmt)"));
        assert!(result.contains("github.com/fmtlib/fmt"));
    }

    #[test]
    fn test_migrate_generates_manifest() {
        let cmake = "\
project(hello VERSION 0.1.0)
set(CMAKE_CXX_STANDARD 20)
add_executable(hello src/main.cpp)
";
        let info = parse_cmake(cmake);
        let manifest = build_manifest(&info, info.project_name.as_deref().unwrap());
        let toml_str = manifest.to_toml_string().unwrap();

        assert!(toml_str.contains("name = \"hello\""));
        assert!(toml_str.contains("version = \"0.1.0\""));
        assert!(toml_str.contains("cxx_standard = \"20\""));
    }

    #[test]
    fn test_parse_cmake_version_from_set_variable() {
        // When project() uses ${VAR}, fall back to set(<NAME>_VERSION ...).
        let cmake = "\
set(SPDLOG_VERSION 1.14.1)
project(spdlog VERSION ${SPDLOG_VERSION})
";
        let info = parse_cmake(cmake);
        assert_eq!(info.project_version.as_deref(), Some("1.14.1"));
    }

    #[test]
    fn test_parse_cmake_highest_cxx_standard_wins() {
        // When multiple standards are set (e.g., fallback branches), pick the highest.
        let cmake = "\
set(CMAKE_CXX_STANDARD 20)
set(CMAKE_CXX_STANDARD 11)
";
        let info = parse_cmake(cmake);
        assert_eq!(info.cxx_standard.as_deref(), Some("20"));
    }

    #[test]
    fn test_parse_cmake_library_preferred_over_executable() {
        // Library projects often have both add_library and add_executable (for examples).
        let cmake = "\
add_library(mylib STATIC src/lib.cpp)
add_executable(example src/example.cpp)
";
        let info = parse_cmake(cmake);
        // add_library should take precedence.
        assert!(info.has_library);
        assert_eq!(info.build_type, Some(BuildType::StaticLib));
    }

    #[test]
    fn test_parse_cmake_msvc_flags_filtered() {
        let cmake = "\
target_compile_options(myapp PRIVATE -Wall /W4 /EHsc -Wextra /Zc:preprocessor)
";
        let info = parse_cmake(cmake);
        // MSVC-style /flags should be filtered out.
        assert_eq!(info.extra_flags, vec!["-Wall", "-Wextra"]);
    }

    #[test]
    fn test_parse_cmake_project_version_set_pattern() {
        let cmake = "\
set(FMT_VERSION 10.2.1)
project(FMT CXX)
";
        let info = parse_cmake(cmake);
        assert_eq!(info.project_name.as_deref(), Some("FMT"));
        // No VERSION in project(), but set_versions should be captured.
        assert_eq!(
            info.set_versions.get("FMT_VERSION").map(|s| s.as_str()),
            Some("10.2.1")
        );
    }

    #[test]
    fn test_parse_cmake_spdlog_like() {
        // Simulates spdlog's pattern: library with variable sources, ALIAS, INTERFACE.
        let cmake = "\
find_package(Threads REQUIRED)
add_library(spdlog SHARED ${SPDLOG_SRCS} ${SPDLOG_ALL_HEADERS})
add_library(spdlog STATIC ${SPDLOG_SRCS} ${SPDLOG_ALL_HEADERS})
add_library(spdlog::spdlog ALIAS spdlog)
add_library(spdlog_header_only INTERFACE)
";
        let info = parse_cmake(cmake);
        assert!(info.has_library, "should detect library");
        assert_ne!(
            info.build_type,
            Some(BuildType::Binary),
            "should not be binary"
        );
    }

    #[test]
    fn test_parse_cmake_quoted_parens_in_option() {
        // Parentheses inside quoted strings must not break command extraction.
        let cmake = r#"
option(FOO "Build something (requires bar)" OFF)
add_library(mylib STATIC src/lib.cpp)
find_package(fmt REQUIRED)
"#;
        let info = parse_cmake(cmake);
        assert!(
            info.has_library,
            "add_library should be found after quoted parens"
        );
        assert_eq!(info.build_type, Some(BuildType::StaticLib));
        assert_eq!(info.packages, vec!["fmt"]);
    }

    #[test]
    fn test_parse_cmake_spdlog_realistic() {
        // Realistic spdlog-like pattern with if/else/endif wrapping libraries.
        let cmake = "\
project(spdlog VERSION ${SPDLOG_VERSION} LANGUAGES CXX)
set(CMAKE_CXX_STANDARD 11)
find_package(Threads REQUIRED)
if(SPDLOG_BUILD_SHARED OR BUILD_SHARED_LIBS)
    if(WIN32)
        configure_file(${CMAKE_CURRENT_SOURCE_DIR}/cmake/version.rc.in ${CMAKE_CURRENT_BINARY_DIR}/version.rc @ONLY)
    endif()
    add_library(spdlog SHARED ${SPDLOG_SRCS} ${SPDLOG_ALL_HEADERS})
    target_compile_definitions(spdlog PUBLIC SPDLOG_SHARED_LIB)
else()
    add_library(spdlog STATIC ${SPDLOG_SRCS} ${SPDLOG_ALL_HEADERS})
endif()
add_library(spdlog::spdlog ALIAS spdlog)
target_include_directories(spdlog PUBLIC include)
add_library(spdlog_header_only INTERFACE)
add_library(spdlog::spdlog_header_only ALIAS spdlog_header_only)
set(CMAKE_CXX_STANDARD 20)
";
        let info = parse_cmake(cmake);
        assert!(info.has_library, "should detect library");
        assert_ne!(
            info.build_type,
            Some(BuildType::Binary),
            "should not be binary, got: {:?}",
            info.build_type
        );
        // Highest standard should win.
        assert_eq!(info.cxx_standard.as_deref(), Some("20"));
    }

    #[test]
    fn test_tokenize_quoted_args() {
        let tokens = tokenize_args(r#"myapp "path with spaces/main.cpp" src/utils.cpp"#);
        assert_eq!(tokens.len(), 3);
        assert_eq!(tokens[0], "myapp");
        assert_eq!(tokens[1], "path with spaces/main.cpp");
        assert_eq!(tokens[2], "src/utils.cpp");
    }

    #[test]
    fn test_unresolved_cxx_standard_skipped() {
        let cmake = "set(CMAKE_CXX_STANDARD ${MY_STD})";
        let info = parse_cmake(cmake);
        // Unresolved variable should not be stored.
        assert_eq!(info.cxx_standard, None);
        assert!(info.all_cxx_standards.is_empty());
    }

    #[test]
    fn test_set_version_resolved_by_variable_name() {
        // project() references ${SPDLOG_VERSION}, so look up SPDLOG_VERSION specifically.
        let cmake = "\
set(OTHER_VERSION 9.9.9)
set(SPDLOG_VERSION 1.14.1)
project(spdlog VERSION ${SPDLOG_VERSION})
";
        let info = parse_cmake(cmake);
        assert_eq!(info.project_version.as_deref(), Some("1.14.1"));
    }

    #[test]
    fn test_set_version_falls_back_to_project_name_version() {
        // No VERSION in project(), fall back to <NAME>_VERSION from set().
        let cmake = "\
set(FMT_VERSION 10.2.1)
project(FMT CXX)
";
        let info = parse_cmake(cmake);
        let manifest = build_manifest(&info, info.project_name.as_deref().unwrap());
        assert_eq!(manifest.package.version, "10.2.1");
    }

    #[test]
    fn test_alias_library_not_concrete() {
        let cmake = "add_library(spdlog::spdlog ALIAS spdlog)";
        let info = parse_cmake(cmake);
        assert!(
            !info.has_library,
            "ALIAS should not count as a concrete library"
        );
        assert_eq!(info.build_type, None);
    }

    #[test]
    fn test_imported_library_not_concrete() {
        let cmake = "add_library(ext IMPORTED)";
        let info = parse_cmake(cmake);
        assert!(
            !info.has_library,
            "IMPORTED should not count as a concrete library"
        );
    }

    #[test]
    fn test_interface_library_not_concrete() {
        let cmake = "add_library(header_only INTERFACE)";
        let info = parse_cmake(cmake);
        assert!(
            !info.has_library,
            "INTERFACE should not count as a concrete library"
        );
    }

    #[test]
    fn test_concrete_library_still_detected() {
        let cmake = "\
add_library(mylib STATIC src/lib.cpp)
add_library(mylib::mylib ALIAS mylib)
add_library(header_only INTERFACE)
";
        let info = parse_cmake(cmake);
        assert!(
            info.has_library,
            "concrete STATIC library should be detected"
        );
        assert_eq!(info.build_type, Some(BuildType::StaticLib));
        assert_eq!(info.sources, vec!["src/lib.cpp"]);
    }

    #[test]
    fn test_inline_comment_in_add_executable() {
        let cmake = "\
add_executable(myapp
    src/main.cpp
    # old.cpp
    src/utils.cpp
)
";
        let info = parse_cmake(cmake);
        assert_eq!(info.build_type, Some(BuildType::Binary));
        assert_eq!(
            info.sources,
            vec!["src/main.cpp", "src/utils.cpp"],
            "commented-out source should not appear"
        );
    }

    #[test]
    fn test_inline_comment_with_hash_in_quotes_preserved() {
        let cmake = r#"
set(MY_VAR "value # not a comment")
add_executable(myapp src/main.cpp)
"#;
        let info = parse_cmake(cmake);
        // Should still parse add_executable correctly.
        assert_eq!(info.sources, vec!["src/main.cpp"]);
    }

    #[test]
    fn test_folly_set_then_project_variable() {
        // Folly pattern: set(PACKAGE_NAME "folly") then project(${PACKAGE_NAME} ...)
        // We can't resolve arbitrary set() variables, but the version should not crash.
        let cmake = "\
set(PACKAGE_NAME \"folly\")
set(PACKAGE_VERSION \"2024.01.01.00\")
project(${PACKAGE_NAME} CXX C ASM)
add_library(folly SHARED src/lib.cpp)
";
        let info = parse_cmake(cmake);
        // project_name will be the literal "${PACKAGE_NAME}" — that's expected.
        // has_library should be true from the concrete add_library.
        assert!(info.has_library);
        assert_eq!(info.build_type, Some(BuildType::SharedLib));
    }
}
