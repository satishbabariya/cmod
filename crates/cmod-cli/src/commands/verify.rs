use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::lockfile::Lockfile;
use cmod_core::types::ModuleId;
use cmod_security::hash::verify_content_hash;
use cmod_security::policy::{SecurityPolicy, ViolationSeverity};
use cmod_security::verify::{verify_all_packages, SignatureStatus};

/// Run `cmod verify` — verify integrity and correctness.
pub fn run(verbose: bool, check_signatures: bool) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    eprintln!("  Verifying project...");
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // 1. Validate manifest
    validate_manifest(&config, &mut errors, &mut warnings, verbose);

    // 2. Validate module identity
    validate_module_identity(&config, &mut errors, &mut warnings, verbose);

    // 3. Validate lockfile consistency
    validate_lockfile(&config, &mut errors, &mut warnings, verbose);

    // 4. Validate source structure
    validate_sources(&config, &mut errors, &mut warnings, verbose);

    // 5. Validate module name matches source declaration
    validate_module_declaration(&config, &mut errors, &mut warnings, verbose);

    // 6. Validate signatures if requested
    if check_signatures {
        validate_signatures(&config, &mut errors, &mut warnings, verbose);
    }

    // 7. Enforce security policy from [security] section
    validate_security_policy(&config, &mut errors, &mut warnings, verbose);

    // Print warnings
    if !warnings.is_empty() {
        eprintln!("  {} warning(s):", warnings.len());
        for (i, warn) in warnings.iter().enumerate() {
            eprintln!("    {}. [warn] {}", i + 1, warn);
        }
    }

    if errors.is_empty() {
        eprintln!("  Verification passed. No issues found.");
        Ok(())
    } else {
        eprintln!("  Verification found {} error(s):", errors.len());
        for (i, err) in errors.iter().enumerate() {
            eprintln!("    {}. {}", i + 1, err);
        }
        Err(CmodError::Other(format!(
            "verification failed with {} issue(s)",
            errors.len()
        )))
    }
}

fn validate_manifest(
    config: &Config,
    errors: &mut Vec<String>,
    warnings: &mut Vec<String>,
    verbose: bool,
) {
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

    // Check package name is not empty
    if config.manifest.package.name.is_empty() {
        errors.push("package name is empty".to_string());
    }

    // Warn if no edition specified
    if config.manifest.package.edition.is_none() {
        warnings.push(
            "no edition specified in [package]; consider adding edition = \"2023\"".to_string(),
        );
    }

    // Warn about missing optional fields
    if config.manifest.package.license.is_none() {
        warnings.push("no license specified".to_string());
    }
}

fn validate_module_identity(
    config: &Config,
    errors: &mut Vec<String>,
    _warnings: &mut Vec<String>,
    verbose: bool,
) {
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

        // Check module name format
        if module.name.is_empty() {
            errors.push("module name is empty".to_string());
        }

        // Validate module name characters (must be alphanumeric, dots, underscores, colons)
        if !module
            .name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '.' || c == '_' || c == ':')
        {
            errors.push(format!(
                "module name '{}' contains invalid characters (allowed: alphanumeric, '.', '_', ':')",
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

        // If this is not a local module, validate Git URL derivation
        if !id.is_local() {
            if verbose {
                eprintln!("    Module: {} (non-local)", module.name);
            }
        } else if verbose {
            eprintln!("    Module: {} (local)", module.name);
        }
    }
}

fn validate_lockfile(
    config: &Config,
    errors: &mut Vec<String>,
    warnings: &mut Vec<String>,
    verbose: bool,
) {
    if verbose {
        eprintln!("  Checking lockfile...");
    }

    if config.manifest.dependencies.is_empty() {
        if verbose {
            eprintln!("    No dependencies to check.");
        }
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

            // Check for orphaned lock entries
            for pkg in &lockfile.packages {
                if !config.manifest.dependencies.contains_key(&pkg.name) {
                    warnings.push(format!(
                        "lockfile contains '{}' which is not in dependencies; run `cmod resolve`",
                        pkg.name
                    ));
                }
            }

            // Check lockfile has version field
            if lockfile.version != 1 {
                warnings.push(format!(
                    "lockfile version {} is unexpected (expected 1)",
                    lockfile.version
                ));
            }

            // Verify lockfile integrity hash
            if let Err(e) = lockfile.verify_integrity() {
                errors.push(format!("lockfile integrity check failed: {}", e));
            } else if verbose {
                eprintln!("    Lockfile integrity hash verified.");
            }

            // Verify content hashes of checked-out dependencies
            let deps_dir = config.deps_dir();
            for pkg in &lockfile.packages {
                if pkg.hash.is_none() {
                    if pkg.source.as_deref() == Some("git") {
                        warnings.push(format!(
                            "package '{}' has no content hash; re-run `cmod resolve`",
                            pkg.name
                        ));
                    }
                    continue;
                }

                let repo_path = deps_dir.join(&pkg.name);
                if !repo_path.exists() {
                    // Dep not checked out — skip (could be offline)
                    if verbose {
                        eprintln!("    {} — not checked out, skipping hash check", pkg.name);
                    }
                    continue;
                }

                match verify_content_hash(pkg, &repo_path) {
                    Ok(result) => {
                        if result.valid {
                            if verbose {
                                eprintln!("    {} — content hash verified", pkg.name);
                            }
                        } else {
                            errors.push(format!(
                                "package '{}' content hash mismatch: expected {}, got {}",
                                pkg.name,
                                result.expected_hash,
                                result.actual_hash.unwrap_or_else(|| "none".to_string()),
                            ));
                        }
                    }
                    Err(e) => {
                        warnings.push(format!(
                            "could not verify content hash for '{}': {}",
                            pkg.name, e
                        ));
                    }
                }
            }

            if verbose {
                eprintln!("    {} locked packages", lockfile.packages.len());
            }
        }
        Err(_) => {
            errors.push("lockfile not found; run `cmod resolve`".to_string());
        }
    }
}

fn validate_sources(
    config: &Config,
    errors: &mut Vec<String>,
    _warnings: &mut Vec<String>,
    verbose: bool,
) {
    // Workspace roots have no source files — skip this check
    if config.manifest.is_workspace() && config.manifest.module.is_none() {
        if verbose {
            eprintln!("  Skipping source check for workspace root.");
        }
        return;
    }

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
                eprintln!("    Found {} source files:", sources.len());
                for s in &sources {
                    let kind = cmod_build::runner::classify_source(s)
                        .map(|k| format!("{:?}", k))
                        .unwrap_or_else(|_| "unknown".to_string());
                    eprintln!("      {} ({})", s.display(), kind);
                }
            }
        }
        Err(e) => {
            errors.push(format!("failed to scan sources: {}", e));
        }
    }
}

/// Validate that the module name declared in source matches the manifest.
fn validate_module_declaration(
    config: &Config,
    errors: &mut Vec<String>,
    _warnings: &mut Vec<String>,
    verbose: bool,
) {
    if verbose {
        eprintln!("  Checking module declaration...");
    }

    if let Some(ref module) = config.manifest.module {
        let root_path = config.root.join(&module.root);
        if !root_path.exists() {
            return; // Already reported in validate_module_identity
        }

        match cmod_build::runner::extract_module_name(&root_path) {
            Ok(Some(declared_name)) => {
                if declared_name != module.name {
                    errors.push(format!(
                        "module root declares 'export module {};' but manifest says '{}'",
                        declared_name, module.name
                    ));
                } else if verbose {
                    eprintln!("    Module declaration matches manifest: {}", module.name);
                }
            }
            Ok(None) => {
                errors.push(format!(
                    "module root '{}' does not contain an 'export module' declaration",
                    module.root.display()
                ));
            }
            Err(e) => {
                errors.push(format!(
                    "failed to read module root '{}': {}",
                    module.root.display(),
                    e
                ));
            }
        }
    }
}

/// Enforce security policy from the `[security]` manifest section.
fn validate_security_policy(
    config: &Config,
    errors: &mut Vec<String>,
    warnings: &mut Vec<String>,
    verbose: bool,
) {
    let policy = SecurityPolicy::from_manifest(config.manifest.security.as_ref());

    if !policy.is_active() {
        if verbose {
            eprintln!("  No security policy configured.");
        }
        return;
    }

    if verbose {
        eprintln!("  Enforcing security policy...");
    }

    let lockfile = match Lockfile::load(&config.lockfile_path) {
        Ok(l) => l,
        Err(_) => return, // Already reported
    };

    if lockfile.packages.is_empty() {
        return;
    }

    let deps_dir = config.deps_dir();
    let trust_db = cmod_security::trust::TrustDb::load_default().ok();
    let violations = policy.enforce(&lockfile.packages, &deps_dir, trust_db.as_ref());

    for v in &violations {
        match v.severity {
            ViolationSeverity::Error => {
                errors.push(format!("[policy] {}: {}", v.package, v.reason));
            }
            ViolationSeverity::Warning => {
                warnings.push(format!("[policy] {}: {}", v.package, v.reason));
            }
        }
    }
}

/// Verify commit signatures of locked dependencies.
fn validate_signatures(
    config: &Config,
    errors: &mut Vec<String>,
    _warnings: &mut Vec<String>,
    verbose: bool,
) {
    if verbose {
        eprintln!("  Checking commit signatures...");
    }

    let lockfile = match Lockfile::load(&config.lockfile_path) {
        Ok(l) => l,
        Err(_) => return, // Lockfile issues already reported
    };

    if lockfile.packages.is_empty() {
        return;
    }

    let deps_dir = config.deps_dir();
    let results = verify_all_packages(&lockfile.packages, &deps_dir, true);

    for result in &results {
        match &result.signature_status {
            SignatureStatus::Valid { signer } => {
                if verbose {
                    eprintln!("    {} — signed by {}", result.package_name, signer);
                }
            }
            SignatureStatus::Untrusted { signer } => {
                if verbose {
                    eprintln!(
                        "    {} — signed by {} (untrusted key)",
                        result.package_name, signer
                    );
                }
            }
            SignatureStatus::Unsigned => {
                if verbose {
                    eprintln!("    {} — unsigned commit", result.package_name);
                }
            }
            SignatureStatus::Invalid { reason } => {
                errors.push(format!(
                    "package '{}' has invalid signature: {}",
                    result.package_name, reason
                ));
            }
        }
    }
}
