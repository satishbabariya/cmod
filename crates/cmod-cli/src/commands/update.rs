use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::lockfile::Lockfile;
use cmod_resolver::Resolver;

/// Run `cmod update` — re-resolve dependencies to latest matching versions.
pub fn run(name: Option<String>, patch_only: bool, verbose: bool) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    if patch_only {
        eprintln!("  Updating dependencies (patch-level only)...");
    } else {
        eprintln!("  Updating dependencies...");
    }

    let mut resolver = Resolver::new(config.deps_dir());

    if let Some(ref dep_name) = name {
        // Update a single dependency: remove it from the lockfile and re-resolve
        let mut existing_lock = Lockfile::load(&config.lockfile_path).unwrap_or_default();

        // If --patch, remember the current version so we can validate later
        let old_version = if patch_only {
            existing_lock
                .find_package(dep_name)
                .map(|p| p.version.clone())
        } else {
            None
        };

        existing_lock.remove_package(dep_name);

        let lockfile = resolver.resolve(&config.manifest, Some(&existing_lock), false, false)?;

        // If --patch, verify the update is only a patch bump
        if patch_only {
            if let (Some(old_ver), Some(new_pkg)) = (&old_version, lockfile.find_package(dep_name))
            {
                if !is_patch_update(old_ver, &new_pkg.version) {
                    return Err(CmodError::Other(format!(
                        "update for '{}' from {} to {} exceeds patch level; use without --patch to allow",
                        dep_name, old_ver, new_pkg.version
                    )));
                }
            }
        }

        lockfile.save(&config.lockfile_path)?;

        if let Some(pkg) = lockfile.find_package(dep_name) {
            eprintln!("  Updated {} to v{}", dep_name, pkg.version);
        }
    } else {
        // Full re-resolve (ignore existing lockfile)
        let existing_lock = if patch_only {
            Lockfile::load(&config.lockfile_path).ok()
        } else {
            None
        };

        let lockfile = resolver.resolve(&config.manifest, None, false, false)?;

        // If --patch, validate all updates are patch-only
        if patch_only {
            if let Some(ref old_lock) = existing_lock {
                for new_pkg in &lockfile.packages {
                    if let Some(old_pkg) = old_lock.find_package(&new_pkg.name) {
                        if !is_patch_update(&old_pkg.version, &new_pkg.version) {
                            return Err(CmodError::Other(format!(
                                "update for '{}' from {} to {} exceeds patch level; use without --patch to allow",
                                new_pkg.name, old_pkg.version, new_pkg.version
                            )));
                        }
                    }
                }
            }
        }

        lockfile.save(&config.lockfile_path)?;

        eprintln!("  Updated {} dependencies", lockfile.packages.len());
        if verbose {
            for pkg in &lockfile.packages {
                eprintln!("    {} v{}", pkg.name, pkg.version);
            }
        }
    }

    Ok(())
}

/// Check if a version update is patch-level only (same major.minor).
fn is_patch_update(old: &str, new: &str) -> bool {
    let old_parts: Vec<&str> = old.split('.').collect();
    let new_parts: Vec<&str> = new.split('.').collect();

    if old_parts.len() < 2 || new_parts.len() < 2 {
        return false;
    }

    // Major and minor must be the same
    old_parts[0] == new_parts[0] && old_parts[1] == new_parts[1]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_patch_update() {
        assert!(is_patch_update("1.2.3", "1.2.4"));
        assert!(is_patch_update("1.2.3", "1.2.99"));
        assert!(is_patch_update("0.1.0", "0.1.1"));
        assert!(!is_patch_update("1.2.3", "1.3.0"));
        assert!(!is_patch_update("1.2.3", "2.0.0"));
        assert!(!is_patch_update("1.2.3", "2.2.3"));
    }

    #[test]
    fn test_is_patch_update_edge_cases() {
        assert!(is_patch_update("1.0.0", "1.0.0")); // same version is valid
        assert!(!is_patch_update("1", "2")); // too few parts
        assert!(is_patch_update("1.0", "1.0")); // two parts, same
        assert!(!is_patch_update("1.0", "1.1")); // two parts, minor changed
    }
}
