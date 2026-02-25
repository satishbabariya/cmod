use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::lockfile::Lockfile;
use cmod_resolver::Resolver;
use cmod_workspace::WorkspaceManager;

/// Run `cmod resolve` — resolve dependencies and generate the lockfile.
pub fn run(locked: bool, offline: bool, verbose: bool) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    eprintln!("  Resolving dependencies...");

    // Check if this is a workspace
    if config.manifest.is_workspace() {
        return resolve_workspace(&config, locked, offline, verbose);
    }

    let existing_lock = Lockfile::load(&config.lockfile_path).ok();
    let resolver = Resolver::new(config.deps_dir());

    let lockfile = resolver.resolve(
        &config.manifest,
        existing_lock.as_ref(),
        locked,
        offline,
    )?;

    lockfile.save(&config.lockfile_path)?;

    eprintln!(
        "  Resolved {} dependencies",
        lockfile.packages.len()
    );
    if verbose {
        for pkg in &lockfile.packages {
            eprintln!(
                "    {} v{} ({})",
                pkg.name,
                pkg.version,
                pkg.commit.as_deref().unwrap_or("local")
            );
        }
    }

    Ok(())
}

/// Resolve dependencies for a workspace (all members share one lockfile).
fn resolve_workspace(
    config: &Config,
    locked: bool,
    offline: bool,
    verbose: bool,
) -> Result<(), CmodError> {
    let ws = WorkspaceManager::load(&config.root)?;

    // Collect all dependencies from all members
    let all_deps = ws.all_dependencies()?;

    // Create a synthetic manifest with all deps for resolution
    let mut combined_manifest = config.manifest.clone();
    combined_manifest.dependencies = all_deps;

    let existing_lock = Lockfile::load(&ws.lockfile_path()).ok();
    let resolver = Resolver::new(config.deps_dir());

    let lockfile = resolver.resolve(
        &combined_manifest,
        existing_lock.as_ref(),
        locked,
        offline,
    )?;

    lockfile.save(&ws.lockfile_path())?;

    eprintln!(
        "  Resolved {} dependencies for workspace ({} members)",
        lockfile.packages.len(),
        ws.members.len()
    );
    if verbose {
        for pkg in &lockfile.packages {
            eprintln!(
                "    {} v{} ({})",
                pkg.name,
                pkg.version,
                pkg.commit.as_deref().unwrap_or("local")
            );
        }
    }

    Ok(())
}
