use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::lockfile::Lockfile;
use cmod_core::types::ModuleId;

/// Run `cmod verify` — verify integrity and correctness.
pub fn run(verbose: bool) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    eprintln!("  Verifying project...");
    let mut errors = Vec::new();

    // 1. Validate manifest
    validate_manifest(&config, &mut errors, verbose);

    // 2. Validate module identity
    validate_module_identity(&config, &mut errors, verbose);

    // 3. Validate lockfile consistency
    validate_lockfile(&config, &mut errors, verbose);

    // 4. Validate source structure
    validate_sources(&config, &mut errors, verbose);

    if errors.is_empty() {
        eprintln!("  Verification passed. No issues found.");
        Ok(())
    } else {
        eprintln!("  Verification found {} issue(s):", errors.len());
        for (i, err) in errors.iter().enumerate() {
            eprintln!("    {}. {}", i + 1, err);
        }
        Err(CmodError::Other(format!(
            "verification failed with {} issue(s)",
            errors.len()
        )))
    }
}

fn validate_manifest(config: &Config, errors: &mut Vec<String>, verbose: bool) {
    if verbose {
        eprintln!("  Checking manifest...");
    }

    // Check version is valid semver
    if semver::Version::parse(&config.manifest.package.version).is_err() {
        errors.push(format!(
            "package version '{}' is not valid semver",
            config.manifest.package.version
        ));
    }
}

fn validate_module_identity(config: &Config, errors: &mut Vec<String>, verbose: bool) {
    if verbose {
        eprintln!("  Checking module identity...");
    }

    if let Some(ref module) = config.manifest.module {
        let id = ModuleId(module.name.clone());

        // Check for reserved prefixes
        if id.is_reserved() {
            errors.push(format!(
                "module name '{}' uses a reserved prefix (std.* / stdx.*)",
                module.name
            ));
        }

        // Check that root source file exists
        let root_path = config.root.join(&module.root);
        if !root_path.exists() {
            errors.push(format!(
                "module root file '{}' does not exist",
                module.root.display()
            ));
        }
    }
}

fn validate_lockfile(config: &Config, errors: &mut Vec<String>, verbose: bool) {
    if verbose {
        eprintln!("  Checking lockfile...");
    }

    if config.manifest.dependencies.is_empty() {
        return;
    }

    match Lockfile::load(&config.lockfile_path) {
        Ok(lockfile) => {
            // Check that every dependency has a lock entry
            for name in config.manifest.dependencies.keys() {
                if lockfile.find_package(name).is_none() {
                    errors.push(format!(
                        "dependency '{}' is not in the lockfile; run `cmod resolve`",
                        name
                    ));
                }
            }
        }
        Err(_) => {
            errors.push("lockfile not found; run `cmod resolve`".to_string());
        }
    }
}

fn validate_sources(config: &Config, errors: &mut Vec<String>, verbose: bool) {
    if verbose {
        eprintln!("  Checking source files...");
    }

    let src_dir = config.src_dir();
    if !src_dir.exists() {
        errors.push(format!(
            "source directory '{}' does not exist",
            src_dir.display()
        ));
        return;
    }

    match cmod_build::runner::discover_sources(&src_dir) {
        Ok(sources) => {
            if sources.is_empty() {
                errors.push("no C++ source files found in src/".to_string());
            } else if verbose {
                eprintln!("    Found {} source files", sources.len());
            }
        }
        Err(e) => {
            errors.push(format!("failed to scan sources: {}", e));
        }
    }
}
