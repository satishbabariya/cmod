use std::path::Path;

use cmod_core::error::CmodError;
use cmod_core::manifest;

/// Run `cmod init` — initialize a new module or workspace.
pub fn run(workspace: bool, name: Option<String>) -> Result<(), CmodError> {
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

    if workspace {
        init_workspace(&cwd, &project_name)
    } else {
        init_module(&cwd, &project_name)
    }
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
fn init_module(dir: &Path, name: &str) -> Result<(), CmodError> {
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
            "export module {};\n\nexport namespace {} {{\n\n}} // namespace {}\n",
            module_name, cpp_name, cpp_name
        ),
    )?;

    // Create main entry point for binary projects
    std::fs::write(
        dir.join("src/main.cpp"),
        format!(
            "import {};\n\nint main() {{\n    return 0;\n}}\n",
            module_name
        ),
    )?;

    // Create stub test file
    std::fs::write(
        dir.join("tests/main.cpp"),
        format!(
            "import {};\n\nint main() {{\n    return 0;\n}}\n",
            module_name
        ),
    )?;

    eprintln!("  Created module '{}' in {}", name, dir.display());
    eprintln!("  - cmod.toml");
    eprintln!("  - src/lib.cppm");
    eprintln!("  - src/main.cpp");
    eprintln!("  - tests/main.cpp");

    Ok(())
}

/// Initialize a workspace.
fn init_workspace(dir: &Path, name: &str) -> Result<(), CmodError> {
    let m = manifest::default_workspace_manifest(name);
    m.save(&dir.join("cmod.toml"))?;

    eprintln!("  Created workspace '{}' in {}", name, dir.display());
    eprintln!("  - cmod.toml (workspace)");
    eprintln!();
    eprintln!("  Add members with `cmod init --name <member>` in subdirectories.");

    Ok(())
}
