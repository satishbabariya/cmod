use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::lockfile::Lockfile;
use cmod_resolver::Resolver;

/// Run `cmod update` — re-resolve dependencies to latest matching versions.
pub fn run(name: Option<String>, verbose: bool) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    eprintln!("  Updating dependencies...");

    let resolver = Resolver::new(config.deps_dir());

    if let Some(ref dep_name) = name {
        // Update a single dependency: remove it from the lockfile and re-resolve
        let mut existing_lock = Lockfile::load(&config.lockfile_path).unwrap_or_default();
        existing_lock.remove_package(dep_name);

        let lockfile = resolver.resolve(
            &config.manifest,
            Some(&existing_lock),
            false,
            false,
        )?;

        lockfile.save(&config.lockfile_path)?;

        if let Some(pkg) = lockfile.find_package(dep_name) {
            eprintln!("  Updated {} to v{}", dep_name, pkg.version);
        }
    } else {
        // Full re-resolve (ignore existing lockfile)
        let lockfile =
            resolver.resolve(&config.manifest, None, false, false)?;
        lockfile.save(&config.lockfile_path)?;

        eprintln!(
            "  Updated {} dependencies",
            lockfile.packages.len()
        );
        if verbose {
            for pkg in &lockfile.packages {
                eprintln!("    {} v{}", pkg.name, pkg.version);
            }
        }
    }

    Ok(())
}
