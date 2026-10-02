use std::collections::{BTreeMap, HashMap};
use std::path::{Component, Path, PathBuf};

use cmod_build::runner::{classify_source, discover_sources_multi, extract_module_name};
use cmod_core::error::CmodError;
use cmod_core::manifest::{default_manifest, Build, Compat, Manifest, Module, Package, Toolchain};
use cmod_core::shell::Shell;
use cmod_core::types::{BuildType, Compiler, ModuleUnitKind};

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
    /// The build type of the product target (see [`product_index`]).
    build_type: Option<BuildType>,
    /// Whether a concrete add_library was seen.
    has_library: bool,
    /// The sources of the product and of the targets it links.
    sources: Vec<String>,
    /// The sources of every other target (tests, examples, tools).
    other_sources: Vec<String>,
    include_dirs: Vec<String>,
    extra_flags: Vec<String>,
    /// What the product links that this file does not define.
    linked_libraries: Vec<String>,
    packages: Vec<String>,
    has_tests: bool,
    subdirectories: Vec<String>,
    /// Targets in the order they are defined (or first referenced).
    targets: Vec<CmakeTarget>,
    /// `add_library(<alias> ALIAS <target>)`.
    aliases: HashMap<String, String>,
    /// Directory-scoped settings: include_directories(), add_compile_options(),
    /// add_compile_definitions() and add_definitions().
    global_include_dirs: Vec<String>,
    global_flags: Vec<String>,
    /// Flag commands under a condition, with the target they set (`None`
    /// for directory-wide ones).
    conditional: Vec<(Option<String>, String)>,
    /// Those of them that would apply to the product, for the user to review.
    conditional_settings: Vec<String>,
}

/// One CMake target and what its target_*() commands give it.
#[derive(Debug, Default)]
struct CmakeTarget {
    name: String,
    kind: TargetKind,
    sources: Vec<String>,
    include_dirs: Vec<String>,
    flags: Vec<String>,
    links: Vec<String>,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
enum TargetKind {
    /// Only named by target_*() commands: defined elsewhere (a subdirectory).
    #[default]
    Unknown,
    Executable,
    Library(BuildType),
    /// add_library(... INTERFACE): no sources of its own, but usage
    /// requirements its dependents inherit.
    Interface,
    /// add_library(... IMPORTED): something outside this project.
    Imported,
}

/// What the migration found on disk: where the sources are and which module
/// the package provides.
#[derive(Debug, Default)]
struct Layout {
    module: Option<Module>,
    /// `[build] sources`; empty means the default `src/`.
    sources: Vec<String>,
    /// `[build] exclude`: sources of other targets inside those directories.
    exclude: Vec<String>,
    warnings: Vec<String>,
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

    let layout = detect_layout(&project_dir, &info);
    if let Some(ref module) = layout.module {
        shell.status(
            "Detected",
            format!("module {} ({})", module.name, module.root.display()),
        );
    }
    if !layout.sources.is_empty() {
        shell.status(
            "Detected",
            format!("source directories: {}", layout.sources.join(", ")),
        );
    }

    // Build manifest from extracted info.
    let manifest = build_manifest(&info, &name, &layout);

    // Write cmod.toml.
    let toml_str = manifest.to_toml_string()?;

    // Append TODO comments for dependencies that need manual mapping.
    let final_content = append_migration_comments(&toml_str, &info);
    std::fs::write(&cmod_toml_path, &final_content)?;

    shell.status("Generated", "cmod.toml");

    // Create src/ directory if missing, when that is where sources go.
    let src_dir = project_dir.join("src");
    if layout.sources.is_empty() && !src_dir.exists() {
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
    for warning in &layout.warnings {
        shell.warn(warning);
    }

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

    if layout.module.is_none() {
        shell.note("No module interface found: add C++20 module declarations to your sources");
    }
    shell.note("Run `cmod build` to verify the migration");

    Ok(())
}

/// Build a `Manifest` from parsed CMake information.
fn build_manifest(info: &CmakeInfo, name: &str, layout: &Layout) -> Manifest {
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

    // Module: the one the sources declare, if any.
    manifest.module = layout.module.clone();

    // Toolchain — only set cxx_standard when the value is a concrete number.
    let resolved_std = info
        .cxx_standard
        .as_deref()
        .filter(|s| !s.contains("${") && s.chars().all(|c| c.is_ascii_digit()))
        .map(|s| s.to_string());
    let mut cxx_std = resolved_std.unwrap_or_else(|| "20".to_string());
    // Modules need C++20, whatever the oldest standard the project supports.
    if layout.module.is_some() && cxx_std.parse::<u32>().is_ok_and(|n| n < 20) {
        cxx_std = "20".to_string();
    }
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
        sources: layout.sources.clone(),
        exclude: layout.exclude.clone(),
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
                comment_text(pkg)
            ));
            if let Some(hint) = well_known_package_hint(pkg) {
                result.push_str(&format!("#   e.g. {} = \"^1.0\"\n", hint));
            }
        }

        if !info.linked_libraries.is_empty() {
            result.push_str("# Linked libraries: ");
            result.push_str(&comment_text(&info.linked_libraries.join(", ")));
            result.push('\n');
        }
    }

    if !info.conditional_settings.is_empty() {
        result.push_str("\n# ==========================================================\n");
        result.push_str("# TODO: Flags set under a condition in CMakeLists.txt, not migrated;\n");
        result.push_str("# add those that apply to [build] extra_flags (definitions as -D)\n");
        result.push_str("# ==========================================================\n");
        for command in &info.conditional_settings {
            result.push_str(&format!("# {}\n", comment_text(command)));
        }
    }

    result
}

/// `text` on one comment line: line breaks and other control characters
/// would end the comment and let the rest be read as TOML.
fn comment_text(text: &str) -> String {
    text.chars()
        .map(|c| if c.is_control() { ' ' } else { c })
        .collect()
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

    // Commands inside function() and macro() bodies only run when called;
    // those inside if(), foreach() and while() only under some condition.
    let mut body_depth = 0usize;
    let mut cond_depth = 0usize;
    for (cmd_name, args_str) in &commands {
        let cmd = cmd_name.to_lowercase();
        // `add_executable(${PROJECT_NAME} ...)` names the target after the project.
        let substituted;
        let args_str: &str = match info.project_name.as_deref() {
            Some(project) if !project.contains('$') && args_str.contains("PROJECT_NAME}") => {
                substituted = args_str
                    .replace("${PROJECT_NAME}", project)
                    .replace("${CMAKE_PROJECT_NAME}", project);
                &substituted
            }
            _ => args_str,
        };
        match cmd.as_str() {
            "function" | "macro" => {
                body_depth += 1;
                continue;
            }
            "endfunction" | "endmacro" => {
                body_depth = body_depth.saturating_sub(1);
                continue;
            }
            _ if body_depth > 0 => continue,
            "if" | "foreach" | "while" => {
                cond_depth += 1;
                continue;
            }
            "endif" | "endforeach" | "endwhile" => {
                cond_depth = cond_depth.saturating_sub(1);
                continue;
            }
            // Flags that depend on a condition (a platform, an option) are
            // listed for the user to decide on rather than applied.
            "target_compile_definitions"
            | "target_compile_options"
            | "add_compile_definitions"
            | "add_compile_options"
            | "add_definitions"
                if cond_depth > 0 =>
            {
                let tokens = tokenize_args(args_str);
                let target = cmd
                    .starts_with("target_")
                    .then(|| tokens.first().cloned())
                    .flatten();
                info.conditional
                    .push((target, format!("{}({})", cmd, tokens.join(" "))));
                continue;
            }
            _ => {}
        }
        match cmd.as_str() {
            "project" => parse_project(args_str, &mut info),
            "set" => parse_set(args_str, &mut info),
            "add_executable" => parse_add_executable(args_str, &mut info),
            "add_library" => parse_add_library(args_str, &mut info),
            "find_package" => parse_find_package(args_str, &mut info),
            "target_sources" => parse_target_sources(args_str, &mut info),
            "target_link_libraries" => parse_target_link_libraries(args_str, &mut info),
            "target_compile_options" => parse_target_compile_options(args_str, &mut info),
            "target_compile_definitions" => {
                parse_target_compile_definitions(args_str, &mut info);
            }
            "target_include_directories" => {
                parse_target_include_directories(args_str, &mut info);
            }
            "target_compile_features" => parse_target_compile_features(args_str, &mut info),
            "include_directories" => {
                let dirs = include_dir_args(&tokenize_args(args_str));
                info.global_include_dirs.extend(dirs);
            }
            "add_compile_options" | "add_definitions" => {
                let tokens = tokenize_args(args_str);
                info.global_flags
                    .extend(tokens.into_iter().filter(|t| !t.starts_with('$')));
            }
            "add_compile_definitions" => {
                let defines = define_flags(&tokenize_args(args_str));
                info.global_flags.extend(defines);
            }
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

    collect_product(&mut info);

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

/// `project(<name> [VERSION <version>] ...)`.
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

/// `set(<variable> <value>...)`: the C++ standard and version variables.
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

/// The target named `name`, created as [`TargetKind::Unknown`] when this
/// file has not defined it (yet).
fn target_mut<'a>(info: &'a mut CmakeInfo, name: &str) -> &'a mut CmakeTarget {
    let name = info
        .aliases
        .get(name)
        .cloned()
        .unwrap_or_else(|| name.to_string());
    let index = match info.targets.iter().position(|t| t.name == name) {
        Some(index) => index,
        None => {
            info.targets.push(CmakeTarget {
                name,
                ..Default::default()
            });
            info.targets.len() - 1
        }
    };
    &mut info.targets[index]
}

/// The sources among `tokens`: paths, with variables and generator
/// expressions left out.
fn source_args(tokens: &[String]) -> Vec<String> {
    tokens
        .iter()
        .filter_map(|t| project_path(t))
        .filter(|t| t != ".")
        .collect()
}

/// `add_executable(<name> [WIN32] [MACOSX_BUNDLE] [EXCLUDE_FROM_ALL] <sources>...)`,
/// and its `IMPORTED` and `ALIAS` forms.
fn parse_add_executable(args: &str, info: &mut CmakeInfo) {
    let tokens = tokenize_args(args);
    let Some(name) = tokens.first() else {
        return;
    };
    if tokens.get(1).map(String::as_str) == Some("ALIAS") {
        if let Some(target) = tokens.get(2) {
            info.aliases.insert(name.clone(), target.clone());
        }
        return;
    }
    if tokens.iter().any(|t| t == "IMPORTED") {
        target_mut(info, name).kind = TargetKind::Imported;
        return;
    }

    // Remaining tokens (after target name) are source files,
    // skipping CMake keywords.
    let cmake_keywords = ["WIN32", "MACOSX_BUNDLE", "EXCLUDE_FROM_ALL"];
    let sources: Vec<String> = tokens[1..]
        .iter()
        .filter(|t| !cmake_keywords.contains(&t.as_str()))
        .cloned()
        .collect();
    let sources = source_args(&sources);

    let target = target_mut(info, name);
    target.kind = TargetKind::Executable;
    target.sources.extend(sources);
}

/// `add_library(<name> [STATIC|SHARED|MODULE|OBJECT] <sources>...)`, and its
/// `INTERFACE`, `IMPORTED` and `ALIAS` forms.
fn parse_add_library(args: &str, info: &mut CmakeInfo) {
    let tokens = tokenize_args(args);
    let Some(name) = tokens.first() else {
        return;
    };

    // Determine library type.
    let mut bt = BuildType::StaticLib;
    let mut source_start = 1;
    let mut kind = None;

    let lib_keywords = [
        "STATIC",
        "SHARED",
        "MODULE",
        "OBJECT",
        "INTERFACE",
        "IMPORTED",
        "ALIAS",
        "EXCLUDE_FROM_ALL",
        "GLOBAL",
        "UNKNOWN",
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
            "ALIAS" => {
                // Not a target of its own: another name for one.
                if let Some(target) = tokens.get(i + 1) {
                    info.aliases.insert(name.clone(), target.clone());
                }
                return;
            }
            "IMPORTED" => {
                kind = Some(TargetKind::Imported);
                break;
            }
            "INTERFACE" => {
                kind = Some(TargetKind::Interface);
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

    let sources = match kind {
        Some(_) => Vec::new(),
        None => {
            let sources: Vec<String> = tokens[source_start.min(tokens.len())..]
                .iter()
                .filter(|t| !lib_keywords.contains(&t.as_str()))
                .cloned()
                .collect();
            source_args(&sources)
        }
    };

    let target = target_mut(info, name);
    target.kind = kind.unwrap_or(TargetKind::Library(bt));
    target.sources.extend(sources);
}

/// `find_package(<package> ...)`: a dependency to map by hand.
fn parse_find_package(args: &str, info: &mut CmakeInfo) {
    let tokens = tokenize_args(args);
    if let Some(pkg) = tokens.first() {
        info.packages.push(pkg.clone());
    }
}

/// `target_sources(<target> <PUBLIC|PRIVATE|INTERFACE> [items...]
/// [FILE_SET <set> [TYPE <type>] [BASE_DIRS <dirs>...] [FILES <files>...]]...)`.
fn parse_target_sources(args: &str, info: &mut CmakeInfo) {
    #[derive(PartialEq)]
    enum State {
        Files,
        SetName,
        Skip,
    }

    let tokens = tokenize_args(args);
    let Some(name) = tokens.first() else {
        return;
    };
    let mut state = State::Files;
    let mut files = Vec::new();
    for token in &tokens[1..] {
        match token.as_str() {
            "PUBLIC" | "PRIVATE" | "INTERFACE" | "FILES" => state = State::Files,
            "FILE_SET" => state = State::SetName,
            "TYPE" | "BASE_DIRS" => state = State::Skip,
            _ => match state {
                State::Files => files.push(token.clone()),
                // The set's name; what follows is TYPE, BASE_DIRS or FILES.
                State::SetName => state = State::Skip,
                State::Skip => {}
            },
        }
    }
    let files = source_args(&files);
    target_mut(info, name).sources.extend(files);
}

/// `target_link_libraries(<target> [<visibility>] <items>...)`.
fn parse_target_link_libraries(args: &str, info: &mut CmakeInfo) {
    let tokens = tokenize_args(args);
    let Some(name) = tokens.first() else {
        return;
    };
    let keywords = [
        "PUBLIC",
        "PRIVATE",
        "INTERFACE",
        "LINK_PUBLIC",
        "LINK_PRIVATE",
        "LINK_INTERFACE_LIBRARIES",
        "debug",
        "optimized",
        "general",
    ];

    // Skip target name (first token), then collect non-keyword tokens.
    let links: Vec<String> = tokens[1..]
        .iter()
        .filter(|t| !keywords.contains(&t.as_str()) && !t.starts_with('$'))
        .cloned()
        .collect();
    target_mut(info, name).links.extend(links);
}

/// `target_compile_options(<target> [BEFORE] <visibility> <options>...)`.
fn parse_target_compile_options(args: &str, info: &mut CmakeInfo) {
    let tokens = tokenize_args(args);
    let Some(name) = tokens.first() else {
        return;
    };
    let keywords = ["PUBLIC", "PRIVATE", "INTERFACE", "BEFORE"];

    let flags: Vec<String> = tokens[1..]
        .iter()
        .filter(|t| !keywords.contains(&t.as_str()) && !t.starts_with('$'))
        .cloned()
        .collect();
    target_mut(info, name).flags.extend(flags);
}

/// `-D` flags for the definitions among `tokens` (`NAME`, `NAME=value`, or
/// `-DNAME`, which CMake accepts too).
fn define_flags(tokens: &[String]) -> Vec<String> {
    let keywords = ["PUBLIC", "PRIVATE", "INTERFACE"];
    tokens
        .iter()
        .filter(|t| !keywords.contains(&t.as_str()) && !t.starts_with('$') && !t.is_empty())
        .map(|t| format!("-D{}", t.strip_prefix("-D").unwrap_or(t)))
        .collect()
}

/// `target_compile_definitions(<target> <visibility> <definitions>...)`, as
/// `-D` flags.
fn parse_target_compile_definitions(args: &str, info: &mut CmakeInfo) {
    let tokens = tokenize_args(args);
    let Some(name) = tokens.first() else {
        return;
    };
    let defines = define_flags(&tokens[1..]);
    target_mut(info, name).flags.extend(defines);
}

/// The include directories among `tokens`.
fn include_dir_args(tokens: &[String]) -> Vec<String> {
    let keywords = [
        "PUBLIC",
        "PRIVATE",
        "INTERFACE",
        "SYSTEM",
        "BEFORE",
        "AFTER",
    ];
    tokens
        .iter()
        .filter(|t| !keywords.contains(&t.as_str()))
        .filter_map(|t| project_path(t))
        .collect()
}

/// `target_include_directories(<target> [SYSTEM] [BEFORE|AFTER] <visibility> <dirs>...)`.
fn parse_target_include_directories(args: &str, info: &mut CmakeInfo) {
    let tokens = tokenize_args(args);
    let Some(name) = tokens.first() else {
        return;
    };
    let dirs = include_dir_args(&tokens[1..]);
    target_mut(info, name).include_dirs.extend(dirs);
}

/// Index of the target the package is made from: an executable named after
/// the project, else a library named after it, else the first library (the
/// executables beside a library are usually its examples and tests), else
/// the first executable.
fn product_index(info: &CmakeInfo) -> Option<usize> {
    let named = |t: &CmakeTarget| {
        info.project_name
            .as_deref()
            .is_some_and(|p| t.name.eq_ignore_ascii_case(p))
    };
    let is_lib = |t: &CmakeTarget| matches!(t.kind, TargetKind::Library(_));
    let is_exe = |t: &CmakeTarget| t.kind == TargetKind::Executable;
    let targets = &info.targets;
    targets
        .iter()
        .position(|t| is_exe(t) && named(t))
        .or_else(|| targets.iter().position(|t| is_lib(t) && named(t)))
        .or_else(|| targets.iter().position(is_lib))
        .or_else(|| targets.iter().position(is_exe))
}

/// Fill in the build type, sources, flags, include directories and links of
/// the product: its own and those of the targets it links, transitively.
/// Settings of targets defined elsewhere (a subdirectory) are kept too, as
/// there is no telling whom they belong to; those of this file's other
/// targets (tests, examples) are not.
fn collect_product(info: &mut CmakeInfo) {
    info.has_library = info
        .targets
        .iter()
        .any(|t| matches!(t.kind, TargetKind::Library(_)));

    let product = product_index(info);
    info.build_type = product.map(|i| match info.targets[i].kind {
        TargetKind::Library(bt) => bt,
        _ => BuildType::Binary,
    });

    // The product and the internal targets it links, in definition order.
    let internal = |name: &str| {
        let name = info.aliases.get(name).map(String::as_str).unwrap_or(name);
        info.targets.iter().position(|t| {
            t.name == name && !matches!(t.kind, TargetKind::Unknown | TargetKind::Imported)
        })
    };
    let mut included = vec![false; info.targets.len()];
    let mut queue: Vec<usize> = product.into_iter().collect();
    while let Some(i) = queue.pop() {
        if std::mem::replace(&mut included[i], true) {
            continue;
        }
        queue.extend(info.targets[i].links.iter().filter_map(|l| internal(l)));
    }
    for (i, target) in info.targets.iter().enumerate() {
        if target.kind == TargetKind::Unknown {
            included[i] = true;
        }
    }

    let mut sources = Vec::new();
    let mut other_sources = Vec::new();
    let mut include_dirs = std::mem::take(&mut info.global_include_dirs);
    let mut flags = std::mem::take(&mut info.global_flags);
    let mut links = Vec::new();
    for (i, target) in info.targets.iter().enumerate() {
        if !included[i] {
            other_sources.extend(target.sources.iter().cloned());
            continue;
        }
        sources.extend(target.sources.iter().cloned());
        include_dirs.extend(target.include_dirs.iter().cloned());
        flags.extend(target.flags.iter().cloned());
        links.extend(
            target
                .links
                .iter()
                .filter(|l| internal(l).is_none())
                .cloned(),
        );
    }

    dedup_in_order(&mut sources);
    dedup_in_order(&mut include_dirs);
    dedup_in_order(&mut flags);
    dedup_in_order(&mut links);
    other_sources.retain(|s| !sources.contains(s));
    dedup_in_order(&mut other_sources);

    let conditional_settings = info
        .conditional
        .iter()
        .filter(|(target, _)| {
            let Some(name) = target.as_deref() else {
                return true;
            };
            let name = info.aliases.get(name).map(String::as_str).unwrap_or(name);
            match info.targets.iter().position(|t| t.name == name) {
                Some(i) => included[i],
                None => true,
            }
        })
        .map(|(_, command)| command.clone())
        .collect();

    info.conditional_settings = conditional_settings;
    info.sources = sources;
    info.other_sources = other_sources;
    info.include_dirs = include_dirs;
    info.extra_flags = flags;
    info.linked_libraries = links;
}

/// Drop repeated items, keeping the first of each.
fn dedup_in_order(items: &mut Vec<String>) {
    let mut seen = std::collections::HashSet::new();
    items.retain(|item| seen.insert(item.clone()));
}

/// `target_compile_features(<target> <visibility> cxx_std_<N>...)`: the C++ standard.
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

/// A path in the project as CMake spells it, relative to the project root:
/// `src/a.cpp`, `${CMAKE_CURRENT_SOURCE_DIR}/src/a.cpp` and
/// `$<BUILD_INTERFACE:${PROJECT_SOURCE_DIR}/include>` all name one. `None`
/// for what depends on variables or other generator expressions.
fn project_path(token: &str) -> Option<String> {
    let mut path = token;
    if let Some(inner) = path
        .strip_prefix("$<BUILD_INTERFACE:")
        .and_then(|p| p.strip_suffix('>'))
    {
        path = inner;
    }
    for var in [
        "${CMAKE_CURRENT_SOURCE_DIR}",
        "${CMAKE_CURRENT_LIST_DIR}",
        "${PROJECT_SOURCE_DIR}",
        "${CMAKE_SOURCE_DIR}",
    ] {
        if let Some(rest) = path.strip_prefix(var) {
            if rest.is_empty() {
                return Some(".".to_string());
            }
            path = rest.strip_prefix('/')?;
            break;
        }
    }
    if path.is_empty() || path.contains('$') {
        return None;
    }
    while let Some(rest) = path.strip_prefix("./") {
        path = rest;
    }
    let path = path.trim_end_matches('/');
    Some(if path.is_empty() { "." } else { path }.to_string())
}

/// Whether `path` is a file cmod compiles.
fn is_cpp_source(path: &str) -> bool {
    matches!(
        Path::new(path).extension().and_then(|e| e.to_str()),
        Some("cppm" | "ixx" | "mpp" | "cpp" | "cc" | "cxx" | "c")
    )
}

/// `path` with `/` separators, as cmod.toml paths and exclude patterns use.
fn slash_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Work out where the product's sources are and which module it provides.
///
/// The source directories are the outermost ones holding the product's
/// sources (left at the default when that is `src/`); sources of the other
/// targets inside them are excluded. The module is the one a primary
/// interface among the sources cmod compiles there exports: one named after
/// the project, else the first the product lists, else the first found.
fn detect_layout(project_dir: &Path, info: &CmakeInfo) -> Layout {
    let mut layout = Layout::default();
    let product: Vec<&str> = info
        .sources
        .iter()
        .map(String::as_str)
        .filter(|s| is_cpp_source(s))
        .collect();

    let mut dirs: Vec<PathBuf> = Vec::new();
    for source in &product {
        let path = Path::new(source);
        if path.is_absolute() || path.components().any(|c| c == Component::ParentDir) {
            layout.warnings.push(format!(
                "{} is outside the project directory and was not migrated",
                source
            ));
            continue;
        }
        match path.parent().filter(|d| !d.as_os_str().is_empty()) {
            Some(dir) => dirs.push(dir.to_path_buf()),
            None => layout.warnings.push(format!(
                "{} is in the project root; cmod builds the sources in [build] sources directories, so move it to src/",
                source
            )),
        }
    }
    dirs.sort();
    dirs.dedup();
    let mut roots: Vec<PathBuf> = dirs
        .iter()
        .filter(|d| !dirs.iter().any(|o| o != *d && d.starts_with(o)))
        .cloned()
        .collect();
    if roots.is_empty() {
        roots.push(PathBuf::from("src"));
    }
    if roots != [PathBuf::from("src")] {
        layout.sources = roots.iter().map(|r| slash_path(r)).collect();
    }

    // An exclude pattern is matched in every source directory, against
    // paths relative to it and against bare file names: one that would also
    // match a product source is left out, with a warning.
    let product_names: Vec<String> = product
        .iter()
        .flat_map(|source| {
            let path = Path::new(source);
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
            let rels = roots
                .iter()
                .filter_map(move |r| path.strip_prefix(r).ok().map(slash_path));
            name.into_iter().chain(rels)
        })
        .collect();
    for other in info.other_sources.iter().filter(|s| is_cpp_source(s)) {
        let path = Path::new(other.as_str());
        let Some(rel) = roots.iter().find_map(|r| path.strip_prefix(r).ok()) else {
            continue;
        };
        let pattern = glob::Pattern::escape(&slash_path(rel));
        let hits_product = glob::Pattern::new(&pattern)
            .map(|pat| product_names.iter().any(|name| pat.matches(name)))
            .unwrap_or(true);
        if hits_product {
            layout.warnings.push(format!(
                "{} belongs to another target, but excluding it would exclude the package's own sources too; cmod builds it as part of the package",
                other
            ));
        } else if !layout.exclude.contains(&pattern) {
            layout.exclude.push(pattern);
        }
    }

    // The product's listed sources first, then the rest of what cmod
    // compiles: every source in those directories.
    let mut candidates: Vec<PathBuf> = product.iter().map(PathBuf::from).collect();
    let dirs: Vec<PathBuf> = roots.iter().map(|r| project_dir.join(r)).collect();
    for path in discover_sources_multi(&dirs, &layout.exclude).unwrap_or_default() {
        if let Ok(rel) = path.strip_prefix(project_dir) {
            let rel = PathBuf::from(slash_path(rel));
            if !candidates.contains(&rel) {
                candidates.push(rel);
            }
        }
    }
    let interfaces: Vec<(String, PathBuf)> = candidates
        .into_iter()
        .filter_map(|rel| {
            let path = project_dir.join(&rel);
            match classify_source(&path) {
                Ok(ModuleUnitKind::InterfaceUnit) => {}
                _ => return None,
            }
            let name = extract_module_name(&path).ok().flatten()?;
            Some((name, rel))
        })
        .collect();
    let wanted = info
        .project_name
        .as_deref()
        .map(|p| p.to_lowercase().replace('-', "_"));
    let chosen = wanted
        .as_deref()
        .and_then(|w| {
            interfaces.iter().find(|(name, _)| {
                let name = name.to_lowercase();
                name == w || name.rsplit('.').next() == Some(w)
            })
        })
        .or_else(|| interfaces.first());
    layout.module = chosen.map(|(name, root)| Module {
        name: name.clone(),
        root: PathBuf::from(slash_path(root)),
    });

    layout
}

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
        let manifest = build_manifest(&info, "myapp", &Layout::default());
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
        let manifest = build_manifest(
            &info,
            info.project_name.as_deref().unwrap(),
            &Layout::default(),
        );
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
        let manifest = build_manifest(
            &info,
            info.project_name.as_deref().unwrap(),
            &Layout::default(),
        );
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

    #[test]
    fn test_target_sources_file_set_files_are_sources() {
        let cmake = "\
add_library(lib STATIC)
target_sources(lib
  PUBLIC FILE_SET mods TYPE CXX_MODULES BASE_DIRS src FILES src/a.cppm src/b.cppm
  PRIVATE src/impl.cpp
  PUBLIC FILE_SET HEADERS FILES include/a.h)
";
        let info = parse_cmake(cmake);
        assert_eq!(
            info.sources,
            vec!["src/a.cppm", "src/b.cppm", "src/impl.cpp", "include/a.h"]
        );
    }

    #[test]
    fn test_compile_definitions_become_flags() {
        let cmake = "\
add_compile_definitions(GLOBAL=1)
add_library(lib src/a.cpp)
target_compile_definitions(lib PUBLIC FAST=1 -DTRACE PRIVATE $<$<CONFIG:Debug>:DBG>)
";
        let info = parse_cmake(cmake);
        assert_eq!(info.extra_flags, vec!["-DGLOBAL=1", "-DFAST=1", "-DTRACE"]);
    }

    #[test]
    fn test_executable_named_after_project_is_the_product() {
        let cmake = "\
project(app)
add_library(core STATIC src/core.cppm)
add_executable(app src/main.cpp)
target_link_libraries(app PRIVATE core)
";
        let info = parse_cmake(cmake);
        assert_eq!(info.build_type, Some(BuildType::Binary));
        assert_eq!(info.sources, vec!["src/core.cppm", "src/main.cpp"]);
    }

    #[test]
    fn test_library_named_after_project_beats_other_libraries() {
        let cmake = "\
project(geo)
add_library(helpers STATIC src/helpers.cpp)
add_library(geo SHARED src/geo.cpp)
add_executable(demo examples/demo.cpp)
";
        let info = parse_cmake(cmake);
        assert_eq!(info.build_type, Some(BuildType::SharedLib));
        assert_eq!(info.sources, vec!["src/geo.cpp"]);
        assert_eq!(
            info.other_sources,
            vec!["src/helpers.cpp", "examples/demo.cpp"]
        );
    }

    #[test]
    fn test_internal_targets_are_not_linked_libraries() {
        let cmake = "\
project(app)
add_library(core STATIC src/core.cpp)
add_library(app::core ALIAS core)
add_library(cfg INTERFACE)
target_compile_definitions(cfg INTERFACE CFG=1)
target_link_libraries(core PUBLIC cfg fmt::fmt)
add_executable(app src/main.cpp)
target_link_libraries(app PRIVATE app::core Threads::Threads)
";
        let info = parse_cmake(cmake);
        assert_eq!(info.linked_libraries, vec!["fmt::fmt", "Threads::Threads"]);
        // The interface library's usage requirements reach the product.
        assert_eq!(info.extra_flags, vec!["-DCFG=1"]);
    }

    #[test]
    fn test_settings_of_other_targets_are_left_out() {
        let cmake = "\
project(app)
add_executable(app src/main.cpp)
target_compile_options(app PRIVATE -Wall)
add_executable(app_tests tests/main.cpp)
target_compile_options(app_tests PRIVATE -fsanitize=address)
target_include_directories(app_tests PRIVATE tests/include)
target_link_libraries(app_tests PRIVATE GTest::gtest_main)
";
        let info = parse_cmake(cmake);
        assert_eq!(info.extra_flags, vec!["-Wall"]);
        assert!(info.include_dirs.is_empty());
        assert!(info.linked_libraries.is_empty());
        assert_eq!(info.other_sources, vec!["tests/main.cpp"]);
    }

    #[test]
    fn test_project_path_spellings() {
        assert_eq!(project_path("src/a.cpp").as_deref(), Some("src/a.cpp"));
        assert_eq!(project_path("./src/a.cpp").as_deref(), Some("src/a.cpp"));
        assert_eq!(
            project_path("${CMAKE_CURRENT_SOURCE_DIR}/src/a.cpp").as_deref(),
            Some("src/a.cpp")
        );
        assert_eq!(
            project_path("$<BUILD_INTERFACE:${PROJECT_SOURCE_DIR}/include>").as_deref(),
            Some("include")
        );
        assert_eq!(project_path("include/").as_deref(), Some("include"));
        assert_eq!(
            project_path("${CMAKE_CURRENT_SOURCE_DIR}").as_deref(),
            Some(".")
        );
        assert_eq!(project_path("$<INSTALL_INTERFACE:include>"), None);
        assert_eq!(project_path("${SOURCES}"), None);
        assert_eq!(project_path("${CMAKE_CURRENT_SOURCE_DIR}x/a.cpp"), None);
    }

    fn write_files(root: &Path, files: &[(&str, &str)]) {
        for (path, content) in files {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, content).unwrap();
        }
    }

    #[test]
    fn test_layout_finds_the_module_the_sources_export() {
        let tmp = tempfile::TempDir::new().unwrap();
        write_files(
            tmp.path(),
            &[
                (
                    "src/math.cppm",
                    "module;\n#include <x>\nexport module mig.math;\n",
                ),
                ("src/math-part.cppm", "export module mig.math:part;\n"),
                ("src/main.cpp", "import mig.math;\nint main() {}\n"),
            ],
        );
        let info = parse_cmake(
            "project(migdemo)
add_library(miglib STATIC)
target_sources(miglib PUBLIC FILE_SET CXX_MODULES FILES src/math-part.cppm src/math.cppm)
add_executable(migdemo src/main.cpp)
target_link_libraries(migdemo PRIVATE miglib)
",
        );
        let layout = detect_layout(tmp.path(), &info);
        let module = layout.module.unwrap();
        assert_eq!(module.name, "mig.math");
        assert_eq!(module.root, PathBuf::from("src/math.cppm"));
        assert!(layout.sources.is_empty(), "src/ is the default");
        assert!(layout.exclude.is_empty());
        assert!(layout.warnings.is_empty());
    }

    #[test]
    fn test_layout_prefers_the_module_named_after_the_project() {
        let tmp = tempfile::TempDir::new().unwrap();
        write_files(
            tmp.path(),
            &[
                ("src/util.cppm", "export module acme.util;\n"),
                ("src/geo.cppm", "export module acme.geo;\n"),
            ],
        );
        let info = parse_cmake("project(geo)\nadd_library(geo src/util.cppm src/geo.cppm)\n");
        let layout = detect_layout(tmp.path(), &info);
        assert_eq!(layout.module.unwrap().name, "acme.geo");
    }

    #[test]
    fn test_layout_source_dirs_and_excludes() {
        let tmp = tempfile::TempDir::new().unwrap();
        let info = parse_cmake(
            "project(app)
add_library(core lib/core/a.cpp lib/core/nested/b.cpp lib/io/c.cpp)
add_executable(app app/main.cpp)
target_link_libraries(app core)
add_executable(example lib/core/example.cpp)
add_executable(tests tests/t.cpp)
",
        );
        let layout = detect_layout(tmp.path(), &info);
        assert_eq!(layout.sources, vec!["app", "lib/core", "lib/io"]);
        assert_eq!(layout.exclude, vec!["example.cpp"]);
        assert!(layout.module.is_none());
    }

    #[test]
    fn test_layout_never_excludes_a_product_source() {
        let tmp = tempfile::TempDir::new().unwrap();
        let info = parse_cmake(
            "project(app)
add_library(core lib/core/geo.cpp lib/core/nested/util.cpp)
add_executable(app app/main.cpp)
target_link_libraries(app core)
add_executable(example lib/core/main.cpp)
add_executable(tool lib/core/util.cpp)
add_executable(bench lib/core/bench[1].cpp)
",
        );
        let layout = detect_layout(tmp.path(), &info);
        assert_eq!(layout.sources, vec!["app", "lib/core"]);
        // `main.cpp` would also drop app/main.cpp, and `util.cpp` the
        // library's nested/util.cpp, so neither is excluded.
        assert_eq!(layout.exclude, vec!["bench[[]1[]].cpp"]);
        assert_eq!(layout.warnings.len(), 2, "{:?}", layout.warnings);
        assert!(layout.warnings[0].starts_with("lib/core/main.cpp "));
        assert!(layout.warnings[1].starts_with("lib/core/util.cpp "));
        // The escaped pattern matches the file it names, and only it.
        let pattern = glob::Pattern::new(&layout.exclude[0]).unwrap();
        assert!(pattern.matches("bench[1].cpp"));
        assert!(!pattern.matches("bench1.cpp"));
    }

    #[test]
    fn test_layout_warns_about_sources_in_the_project_root() {
        let tmp = tempfile::TempDir::new().unwrap();
        let info = parse_cmake("add_executable(app main.cpp ../shared/x.cpp)\n");
        let layout = detect_layout(tmp.path(), &info);
        assert!(layout.sources.is_empty());
        assert_eq!(layout.warnings.len(), 2, "{:?}", layout.warnings);
        assert!(
            layout.warnings[0].contains("../shared/x.cpp")
                || layout.warnings[1].contains("../shared/x.cpp")
        );
    }

    #[test]
    fn test_layout_searches_src_when_cmake_lists_no_sources() {
        let tmp = tempfile::TempDir::new().unwrap();
        write_files(
            tmp.path(),
            &[
                ("src/lib.cppm", "export module local.globbed;\n"),
                ("src/impl.cpp", "module local.globbed;\n"),
            ],
        );
        let info =
            parse_cmake("file(GLOB SRCS src/*.cpp src/*.cppm)\nadd_library(globbed ${SRCS})\n");
        let layout = detect_layout(tmp.path(), &info);
        let module = layout.module.unwrap();
        assert_eq!(module.name, "local.globbed");
        assert_eq!(module.root, PathBuf::from("src/lib.cppm"));
    }

    #[test]
    fn test_module_raises_the_standard_to_cxx20() {
        let info = parse_cmake("project(m)\nset(CMAKE_CXX_STANDARD 17)\n");
        let plain = build_manifest(&info, "m", &Layout::default());
        assert_eq!(plain.toolchain.unwrap().cxx_standard.as_deref(), Some("17"));
        let layout = Layout {
            module: Some(Module {
                name: "m".to_string(),
                root: PathBuf::from("src/m.cppm"),
            }),
            ..Default::default()
        };
        let modular = build_manifest(&info, "m", &layout);
        assert_eq!(
            modular.toolchain.unwrap().cxx_standard.as_deref(),
            Some("20")
        );
        assert_eq!(modular.compat.unwrap().cpp.as_deref(), Some(">=20"));
    }

    #[test]
    fn test_conditional_flags_are_listed_not_applied() {
        let cmake = "\
project(app)
function(setup target)
  target_compile_definitions(${target} PRIVATE FROM_FUNCTION)
  add_library(from_function STATIC x.cpp)
endfunction()
add_executable(app src/main.cpp)
target_compile_definitions(app PRIVATE ALWAYS)
if(WIN32)
  target_compile_definitions(app PRIVATE ON_WINDOWS)
  add_compile_options(-Wglobal)
endif()
add_executable(tool tools/tool.cpp)
if(FUZZ)
  target_compile_definitions(tool PRIVATE TOOL_ONLY)
endif()
";
        let info = parse_cmake(cmake);
        assert_eq!(info.extra_flags, vec!["-DALWAYS"]);
        assert_eq!(info.targets.len(), 2, "{:?}", info.targets);
        assert_eq!(
            info.conditional_settings,
            vec![
                "target_compile_definitions(app PRIVATE ON_WINDOWS)",
                "add_compile_options(-Wglobal)"
            ]
        );
        let comments = append_migration_comments("", &info);
        assert!(comments.contains("# target_compile_definitions(app PRIVATE ON_WINDOWS)\n"));
        assert!(!comments.contains("TOOL_ONLY"));
    }

    #[test]
    fn test_targets_named_by_project_name_variable() {
        let cmake = "\
project(app)
add_library(${PROJECT_NAME}_core STATIC src/core.cpp)
add_executable(${PROJECT_NAME} src/main.cpp)
target_link_libraries(${PROJECT_NAME} PRIVATE ${CMAKE_PROJECT_NAME}_core fmt::fmt)
target_compile_definitions(${PROJECT_NAME} PRIVATE APP=1)
";
        let info = parse_cmake(cmake);
        assert_eq!(info.build_type, Some(BuildType::Binary));
        assert_eq!(info.sources, vec!["src/core.cpp", "src/main.cpp"]);
        assert_eq!(info.linked_libraries, vec!["fmt::fmt"]);
        assert_eq!(info.extra_flags, vec!["-DAPP=1"]);
    }

    #[test]
    fn test_comment_text_stays_on_one_line() {
        let info = CmakeInfo {
            linked_libraries: vec!["evil\n[package]\nname = \"x\"".to_string()],
            ..Default::default()
        };
        let comments = append_migration_comments("", &info);
        assert!(comments.lines().all(|l| l.is_empty() || l.starts_with('#')));
    }

    #[test]
    fn test_manifest_without_module_has_no_module_section() {
        let info = parse_cmake("project(plain)\nadd_executable(plain src/main.cpp)\n");
        let manifest = build_manifest(&info, "plain", &Layout::default());
        assert!(manifest.module.is_none());
        let toml = manifest.to_toml_string().unwrap();
        assert!(!toml.contains("[module]"), "{}", toml);
    }
}
