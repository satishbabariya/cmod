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

    if workspace {
        init_workspace(&cwd, &project_name)
    } else {
        init_module(&cwd, &project_name)
    }
}

/// Initialize a single module project.
fn init_module(dir: &Path, name: &str) -> Result<(), CmodError> {
    let m = manifest::default_manifest(name);

    // Create directory structure
    std::fs::create_dir_all(dir.join("src"))?;
    std::fs::create_dir_all(dir.join("tests"))?;

    // Write manifest
    m.save(&dir.join("cmod.toml"))?;

    // Create stub module interface
    let module_name = m
        .module
        .as_ref()
        .map(|m| m.name.clone())
        .unwrap_or_else(|| format!("local.{}", name));

    std::fs::write(
        dir.join("src/lib.cppm"),
        format!(
            "export module {};\n\nexport namespace {} {{\n\n}} // namespace {}\n",
            module_name, name, name
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
    eprintln!("  - tests/main.cpp");

    Ok(())
}

/// Initialize a workspace.
fn init_workspace(dir: &Path, name: &str) -> Result<(), CmodError> {
    let m = manifest::default_workspace_manifest(name);
    m.save(&dir.join("cmod.toml"))?;

    eprintln!(
        "  Created workspace '{}' in {}",
        name,
        dir.display()
    );
    eprintln!("  - cmod.toml (workspace)");
    eprintln!();
    eprintln!("  Add members with `cmod init --name <member>` in subdirectories.");

    Ok(())
}
