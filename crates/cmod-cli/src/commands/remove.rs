use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::lockfile::Lockfile;
use cmod_core::shell::Shell;
use cmod_resolver::Resolver;

/// Run `cmod remove <name>` — remove a dependency.
pub fn run(name: String, shell: &Shell) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let mut config = Config::load(&cwd)?;

    // Remove from manifest
    Resolver::remove_dependency(&mut config.manifest, &name)?;

    // Save updated manifest, then the lockfile: a manifest that cannot be
    // written leaves both as they were.
    config.manifest.save_dependencies(&config.manifest_path)?;

    // Update lockfile: remove the package
    if let Ok(mut lockfile) = Lockfile::load(&config.lockfile_path) {
        lockfile.remove_package(&name);
        lockfile.save(&config.lockfile_path)?;
    }

    shell.status("Removing", format!("dependency '{}'", name));

    Ok(())
}
