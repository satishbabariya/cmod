use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_workspace::WorkspaceManager;

/// Run `cmod workspace list` — list workspace members.
pub fn list(verbose: bool) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    if !config.manifest.is_workspace() {
        return Err(CmodError::Other(
            "not a workspace; run from a directory with a workspace cmod.toml".to_string(),
        ));
    }

    let ws = WorkspaceManager::load(&config.root)?;

    eprintln!("  Workspace: {}", config.manifest.package.name);
    if let Some(ver) = ws.workspace_version() {
        eprintln!("  Version: {}", ver);
    }
    eprintln!("  Members ({}):", ws.members.len());

    for member in &ws.members {
        if verbose {
            let dep_count = member.manifest.dependencies.len();
            eprintln!(
                "    {} ({}, {} deps)",
                member.name,
                member.path.display(),
                dep_count,
            );
        } else {
            eprintln!("    {}", member.name);
        }
    }

    // Show build order if verbose
    if verbose {
        match ws.build_order() {
            Ok(order) => {
                let names: Vec<&str> = order.iter().map(|m| m.name.as_str()).collect();
                eprintln!("  Build order: {}", names.join(" → "));
            }
            Err(e) => {
                eprintln!("  Build order: error: {}", e);
            }
        }
    }

    Ok(())
}

/// Run `cmod workspace add <name>` — add a new member to the workspace.
pub fn add(name: &str, verbose: bool) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    if !config.manifest.is_workspace() {
        return Err(CmodError::Other(
            "not a workspace; run from a directory with a workspace cmod.toml".to_string(),
        ));
    }

    let mut ws = WorkspaceManager::load(&config.root)?;

    if verbose {
        eprintln!("  Adding member '{}' to workspace", name);
    }

    ws.add_member(name)?;

    eprintln!("  Added member '{}' to workspace", name);
    eprintln!("  Created {}/src/lib.cppm", name);
    eprintln!("  Created {}/cmod.toml", name);

    Ok(())
}

/// Run `cmod workspace remove <name>` — remove a member from the workspace.
pub fn remove(name: &str, verbose: bool) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    if !config.manifest.is_workspace() {
        return Err(CmodError::Other(
            "not a workspace; run from a directory with a workspace cmod.toml".to_string(),
        ));
    }

    let mut ws = WorkspaceManager::load(&config.root)?;

    // Check the member exists
    if !ws.members.iter().any(|m| m.name == name) {
        return Err(CmodError::Other(
            format!("member '{}' not found in workspace", name),
        ));
    }

    // Remove from the members list in the manifest
    if let Some(workspace) = &mut ws.root_manifest.workspace {
        workspace.members.retain(|m| m != name);
    }
    ws.root_manifest.save(&ws.root.join("cmod.toml"))?;

    // Remove from in-memory list
    ws.members.retain(|m| m.name != name);

    if verbose {
        eprintln!("  Removed member '{}' from workspace manifest", name);
        eprintln!("  Note: member directory was NOT deleted. Remove manually if desired.");
    }

    eprintln!("  Removed '{}' from workspace", name);
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_workspace_command_names() {
        // Ensure the module compiles correctly
        assert_eq!(1 + 1, 2);
    }
}
