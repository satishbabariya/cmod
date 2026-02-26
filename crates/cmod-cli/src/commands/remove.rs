use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::lockfile::Lockfile;
use cmod_resolver::Resolver;

/// Run `cmod remove <name>` — remove a dependency.
pub fn run(name: String) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let mut config = Config::load(&cwd)?;

    // Remove from manifest
    Resolver::remove_dependency(&mut config.manifest, &name)?;

    // Update lockfile: remove the package
    if let Ok(mut lockfile) = Lockfile::load(&config.lockfile_path) {
        lockfile.remove_package(&name);
        lockfile.save(&config.lockfile_path)?;
    }

    // Save updated manifest
    config.manifest.save(&config.manifest_path)?;

    eprintln!("  Removed dependency '{}'", name);

    Ok(())
}
