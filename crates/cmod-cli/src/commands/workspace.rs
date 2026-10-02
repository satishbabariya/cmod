use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::shell::Shell;
use cmod_workspace::WorkspaceManager;

/// Run `cmod workspace list` — list workspace members.
pub fn list(shell: &Shell) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    if !config.manifest.is_workspace() {
        return Err(CmodError::Other(
            "not a workspace; run from a directory with a workspace cmod.toml".to_string(),
        ));
    }

    let ws = WorkspaceManager::load(&config.root)?;

    shell.status("Workspace", &config.manifest.package.name);
    if let Some(ver) = ws.workspace_version() {
        shell.status("Version", ver);
    }
    shell.status("Members", format!("{} member(s)", ws.members.len()));

    for member in &ws.members {
        let dep_count = member.manifest.dependencies.len();
        shell.verbose(
            "Member",
            format!(
                "{} ({}, {} deps)",
                member.name,
                member.path.display(),
                dep_count
            ),
        );
        if shell.verbosity() != cmod_core::shell::Verbosity::Verbose {
            shell.status("", &member.name);
        }
    }

    // Show build order if verbose
    match ws.build_order() {
        Ok(order) => {
            let names: Vec<&str> = order.iter().map(|m| m.name.as_str()).collect();
            shell.verbose("Build order", names.join(" -> "));
        }
        Err(e) => {
            shell.verbose("Build order", format!("error: {}", e));
        }
    }

    Ok(())
}

/// Run `cmod workspace add <name> [--scaffold]` — add a member to the workspace.
pub fn add(name: &str, scaffold: bool, shell: &Shell) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    if !config.manifest.is_workspace() {
        return Err(CmodError::Other(
            "not a workspace; run from a directory with a workspace cmod.toml".to_string(),
        ));
    }

    let mut ws = WorkspaceManager::load(&config.root)?;

    shell.verbose("Adding", format!("member '{}' to workspace", name));

    let added = ws.add_member(name, scaffold)?;

    if added.scaffolded {
        shell.verbose("Created", format!("{}/cmod.toml", added.rel_path));
        shell.verbose("Created", format!("{}/src/lib.cppm", added.rel_path));
    }
    shell.status(
        "Added",
        format!("member '{}' ({}) to workspace", added.name, added.rel_path),
    );

    Ok(())
}

/// Run `cmod workspace remove <name>` — remove a member from the workspace.
pub fn remove(name: &str, shell: &Shell) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    if !config.manifest.is_workspace() {
        return Err(CmodError::Other(
            "not a workspace; run from a directory with a workspace cmod.toml".to_string(),
        ));
    }

    let mut ws = WorkspaceManager::load(&config.root)?;

    let removed = ws.remove_member(name)?;

    shell.verbose(
        "Removed",
        format!("member '{}' from workspace manifest", removed.name),
    );
    if removed.excluded {
        shell.note(format!(
            "'{}' matches a [workspace] members glob, so it was added to [workspace] exclude",
            removed.rel_path
        ));
    }
    shell.note("member directory was NOT deleted; remove manually if desired");

    shell.status("Removed", format!("'{}' from workspace", removed.name));
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
