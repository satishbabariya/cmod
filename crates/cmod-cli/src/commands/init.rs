use std::io::Write;
use std::path::Path;

use cmod_core::error::CmodError;
use cmod_core::manifest;
use cmod_core::shell::Shell;

/// Version control for a new package, as `--vcs` selects it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum Vcs {
    /// A git repository (unless the directory is already in one) and a
    /// `.gitignore` for the build directory.
    Git,
    /// No version control files.
    None,
}

/// Run `cmod init` — initialize a new module or workspace.
pub fn run(
    workspace: bool,
    name: Option<String>,
    vcs: Vcs,
    shell: &Shell,
) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;

    // Check if cmod.toml already exists
    if cwd.join("cmod.toml").exists() {
        return Err(CmodError::InvalidManifest {
            reason: "cmod.toml already exists in this directory".to_string(),
        });
    }

    // Determine project name
    let project_name = name.unwrap_or_else(|| {
        cwd.file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("my_project")
            .to_string()
    });

    // Validate project name
    validate_project_name(&project_name)?;
    if !workspace {
        validate_cpp_identifier(&project_name)?;
    }

    // The repository first: if it cannot be made, nothing is written and
    // `cmod init` can be run again.
    if vcs == Vcs::Git {
        init_repository(&cwd, shell)?;
    }
    if workspace {
        init_workspace(&cwd, &project_name, shell)?;
    } else {
        init_module(&cwd, &project_name, shell)?;
    }
    if vcs == Vcs::Git {
        ignore_build_dir(&cwd, shell)?;
    }
    Ok(())
}

/// Make `dir` a git repository unless it is already inside one (a package
/// in a monorepo, a workspace member). Inside a repository that ignores
/// `dir`, it gets its own, as Cargo does: the outer one would not track it.
fn init_repository(dir: &Path, shell: &Shell) -> Result<(), CmodError> {
    match git2::Repository::discover(dir) {
        Ok(repo) => {
            if !is_ignored_by(&repo, dir) {
                return Ok(());
            }
        }
        Err(e) if e.code() == git2::ErrorCode::NotFound => {}
        // An enclosing repository cmod cannot open (owned by someone else,
        // damaged): a repository here would be nested in it.
        Err(e) => {
            return Err(CmodError::GitError {
                reason: format!(
                "cannot tell whether {} is in a git repository ({}); fix that, or pass --vcs none",
                dir.display(),
                e.message()
            ),
            })
        }
    }
    git2::Repository::init(dir).map_err(|e| CmodError::GitError {
        reason: format!(
            "failed to create a git repository in {}: {}",
            dir.display(),
            e.message()
        ),
    })?;
    shell.verbose("Created", "git repository");
    Ok(())
}

/// Whether `repo` ignores the directory `dir` inside its work tree.
fn is_ignored_by(repo: &git2::Repository, dir: &Path) -> bool {
    let (Some(workdir), Ok(dir)) = (repo.workdir(), dir.canonicalize()) else {
        return false;
    };
    let Ok(workdir) = workdir.canonicalize() else {
        return false;
    };
    match dir.strip_prefix(&workdir) {
        Ok(rel) if !rel.as_os_str().is_empty() => {
            let rel = format!("{}/", rel.to_string_lossy().replace('\\', "/"));
            repo.is_path_ignored(Path::new(&rel)).unwrap_or(false)
        }
        _ => false,
    }
}

/// Ignore the build directory in `dir/.gitignore`, appending to one that
/// exists (whatever its encoding) unless it already ignores `build`.
fn ignore_build_dir(dir: &Path, shell: &Shell) -> Result<(), CmodError> {
    let gitignore = dir.join(".gitignore");
    let existing = match std::fs::read(&gitignore) {
        Ok(bytes) => bytes,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(e) => return Err(e.into()),
    };
    if String::from_utf8_lossy(&existing)
        .lines()
        .any(|line| matches!(line.trim(), "/build" | "/build/" | "build" | "build/"))
    {
        return Ok(());
    }
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&gitignore)?;
    if !existing.is_empty() && !existing.ends_with(b"\n") {
        file.write_all(b"\n")?;
    }
    file.write_all(b"/build/\n")?;
    shell.verbose(
        if existing.is_empty() {
            "Created"
        } else {
            "Updated"
        },
        ".gitignore",
    );
    Ok(())
}

/// The module (`local.<name>`) and namespace are named after the package:
/// its name, with `-` as `_`, must be a C++ identifier that is neither a
/// keyword nor a name reserved to the implementation (`std`, `__x`, `_X`).
fn validate_cpp_identifier(name: &str) -> Result<(), CmodError> {
    let ident = sanitize_cpp_name(name);
    if cmod_core::types::is_usable_cpp_identifier(&ident) {
        return Ok(());
    }
    Err(CmodError::Other(format!(
        "'{}' cannot name the package's module and namespace ('{}' is not a usable C++ identifier); pass --name with letters, digits, '_' or '-', not starting with a digit or naming a keyword",
        name, ident
    )))
}

/// Validate a project name for safety and correctness.
fn validate_project_name(name: &str) -> Result<(), CmodError> {
    if name.is_empty() {
        return Err(CmodError::Other("project name cannot be empty".to_string()));
    }

    if name.contains('/') || name.contains('\\') || name.starts_with('.') {
        return Err(CmodError::Other(format!(
            "invalid project name '{}': must not contain path separators or start with '.'",
            name
        )));
    }

    if name.len() > 128 {
        return Err(CmodError::Other(format!(
            "project name '{}' is too long ({} chars, max 128)",
            &name[..32],
            name.len()
        )));
    }

    Ok(())
}

/// Sanitize a name for use as a C++ identifier (module name, namespace).
///
/// Replaces hyphens with underscores since hyphens are not valid in C++
/// identifiers or module names.
fn sanitize_cpp_name(name: &str) -> String {
    name.replace('-', "_")
}

/// Initialize a single module project.
fn init_module(dir: &Path, name: &str, shell: &Shell) -> Result<(), CmodError> {
    let m = manifest::default_manifest(name);

    // Create directory structure
    std::fs::create_dir_all(dir.join("src"))?;
    std::fs::create_dir_all(dir.join("tests"))?;

    // Write manifest
    m.save(&dir.join("cmod.toml"))?;

    // Sanitize name for use in C++ identifiers
    let cpp_name = sanitize_cpp_name(name);

    // Create stub module interface
    let module_name = m
        .module
        .as_ref()
        .map(|m| m.name.clone())
        .unwrap_or_else(|| format!("local.{}", cpp_name));

    std::fs::write(
        dir.join("src/lib.cppm"),
        format!(
            "export module {module};\n\
             \n\
             export namespace {ns} {{\n\
             \n\
             /// A greeting from this module.\n\
             const char *greeting() {{ return \"Hello from {module}!\"; }}\n\
             \n\
             }} // namespace {ns}\n",
            module = module_name,
            ns = cpp_name
        ),
    )?;

    // Create main entry point for binary projects
    std::fs::write(
        dir.join("src/main.cpp"),
        format!(
            "#include <cstdio>\n\
             \n\
             import {module};\n\
             \n\
             int main() {{\n    std::puts({ns}::greeting());\n    return 0;\n}}\n",
            module = module_name,
            ns = cpp_name
        ),
    )?;

    // Create a test of the module
    std::fs::write(
        dir.join("tests/main.cpp"),
        format!(
            "#include <cstring>\n\
             \n\
             import {module};\n\
             \n\
             int main() {{\n    \
             const char *greeting = {ns}::greeting();\n    \
             return std::strncmp(greeting, \"Hello\", 5) == 0 ? 0 : 1;\n}}\n",
            module = module_name,
            ns = cpp_name
        ),
    )?;

    // Create .clang-format if it doesn't exist
    let clang_format_path = dir.join(".clang-format");
    if !clang_format_path.exists() {
        std::fs::write(
            &clang_format_path,
            "BasedOnStyle: LLVM\nIndentWidth: 4\nColumnLimit: 120\n",
        )?;
    }

    shell.status("Created", format!("module '{}' in {}", name, dir.display()));
    shell.verbose("Created", "cmod.toml");
    shell.verbose("Created", "src/lib.cppm");
    shell.verbose("Created", "src/main.cpp");
    shell.verbose("Created", "tests/main.cpp");
    shell.verbose("Created", ".clang-format");

    Ok(())
}

/// Initialize a workspace.
fn init_workspace(dir: &Path, name: &str, shell: &Shell) -> Result<(), CmodError> {
    let m = manifest::default_workspace_manifest(name);
    m.save(&dir.join("cmod.toml"))?;

    shell.status(
        "Created",
        format!("workspace '{}' in {}", name, dir.display()),
    );
    shell.note("add members with `cmod init --name <member>` in subdirectories");

    Ok(())
}
