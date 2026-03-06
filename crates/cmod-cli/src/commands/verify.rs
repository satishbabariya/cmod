use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::lockfile::Lockfile;
use cmod_core::shell::Shell;
use cmod_core::types::ModuleId;
use cmod_security::hash::verify_content_hash;
use cmod_security::policy::{SecurityPolicy, ViolationSeverity};
use cmod_security::signing::{resolve_signing_config, verify_file, VerifyStatus};
use cmod_security::verify::{verify_all_packages, SignatureStatus};

/// Run `cmod verify` — verify integrity and correctness.
pub fn run(shell: &Shell, check_signatures: bool) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    shell.status("Verifying", "project...");
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // 1. Validate manifest
    validate_manifest(&config, &mut errors, &mut warnings, shell);

    // 2. Validate module identity
    validate_module_identity(&config, &mut errors, &mut warnings, shell);

    // 3. Validate lockfile consistency
    validate_lockfile(&config, &mut errors, &mut warnings, shell);

    // 4. Validate source structure
    validate_sources(&config, &mut errors, &mut warnings, shell);

    // 5. Validate module name matches source declaration
    validate_module_declaration(&config, &mut errors, &mut warnings, shell);

    // 6. Validate signatures if requested
    if check_signatures {
        validate_signatures(&config, &mut errors, &mut warnings, shell);
        validate_artifact_signatures(&config, &mut errors, &mut warnings, shell);
    }

    // 7. Enforce security policy from [security] section
    validate_security_policy(&config, &mut errors, &mut warnings, shell);

    // Print warnings
    for warn in &warnings {
        shell.warn(warn);
    }

    if errors.is_empty() {
        shell.status("Verified", "no issues found");
        Ok(())
    } else {
        for err in &errors {
            shell.error(err);
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
    shell: &Shell,
) {
    shell.verbose("Checking", "manifest...");

    if semver::Version::parse(&config.manifest.package.version).is_err() {
        errors.push(format!(
            "package version '{}' is not valid semver",
            config.manifest.package.version
        ));
    }

    if config.manifest.package.name.is_empty() {
        errors.push("package name is empty".to_string());
    }

    if config.manifest.package.edition.is_none() {
        warnings.push(
            "no edition specified in [package]; consider adding edition = \"2023\"".to_string(),
        );
    }

    if config.manifest.package.license.is_none() {
        warnings.push("no license specified".to_string());
    }
}

fn validate_module_identity(
    config: &Config,
    errors: &mut Vec<String>,
    _warnings: &mut Vec<String>,
    shell: &Shell,
) {
    shell.verbose("Checking", "module identity...");

    if let Some(ref module) = config.manifest.module {
        let id = ModuleId(module.name.clone());

        if id.is_reserved() {
            errors.push(format!(
                "module name '{}' uses a reserved prefix (std.* / stdx.*)",
                module.name
            ));
        }

        if module.name.is_empty() {
            errors.push("module name is empty".to_string());
        }

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

        let root_path = config.root.join(&module.root);
        if !root_path.exists() {
            errors.push(format!(
                "module root file '{}' does not exist",
                module.root.display()
            ));
        }

        if !id.is_local() {
            shell.verbose("Module", format!("{} (non-local)", module.name));
        } else {
            shell.verbose("Module", format!("{} (local)", module.name));
        }
    }
}

fn validate_lockfile(
    config: &Config,
    errors: &mut Vec<String>,
    warnings: &mut Vec<String>,
    shell: &Shell,
) {
    shell.verbose("Checking", "lockfile...");

    if config.manifest.dependencies.is_empty() {
        shell.verbose("Lockfile", "no dependencies to check");
        return;
    }

    match Lockfile::load(&config.lockfile_path) {
        Ok(lockfile) => {
            for name in config.manifest.dependencies.keys() {
                if lockfile.find_package(name).is_none() {
                    errors.push(format!(
                        "dependency '{}' is not in the lockfile; run `cmod resolve`",
                        name
                    ));
                }
            }

            for pkg in &lockfile.packages {
                if !config.manifest.dependencies.contains_key(&pkg.name) {
                    warnings.push(format!(
                        "lockfile contains '{}' which is not in dependencies; run `cmod resolve`",
                        pkg.name
                    ));
                }
            }

            if lockfile.version != 1 {
                warnings.push(format!(
                    "lockfile version {} is unexpected (expected 1)",
                    lockfile.version
                ));
            }

            if let Err(e) = lockfile.verify_integrity() {
                errors.push(format!("lockfile integrity check failed: {}", e));
            } else {
                shell.verbose("Lockfile", "integrity hash verified");
            }

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
                    shell.verbose("Skipping", format!("{} — not checked out", pkg.name));
                    continue;
                }

                match verify_content_hash(pkg, &repo_path) {
                    Ok(result) => {
                        if result.valid {
                            shell.verbose("Verified", format!("{} — content hash OK", pkg.name));
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

            shell.verbose(
                "Lockfile",
                format!("{} locked packages", lockfile.packages.len()),
            );
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
    shell: &Shell,
) {
    if config.manifest.is_workspace() && config.manifest.module.is_none() {
        shell.verbose("Skipping", "source check for workspace root");
        return;
    }

    shell.verbose("Checking", "source files...");

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
            } else {
                shell.verbose("Sources", format!("found {} source files", sources.len()));
                for s in &sources {
                    let kind = cmod_build::runner::classify_source(s)
                        .map(|k| format!("{:?}", k))
                        .unwrap_or_else(|_| "unknown".to_string());
                    shell.verbose("Source", format!("{} ({})", s.display(), kind));
                }
            }
        }
        Err(e) => {
            errors.push(format!("failed to scan sources: {}", e));
        }
    }
}

fn validate_module_declaration(
    config: &Config,
    errors: &mut Vec<String>,
    _warnings: &mut Vec<String>,
    shell: &Shell,
) {
    shell.verbose("Checking", "module declaration...");

    if let Some(ref module) = config.manifest.module {
        let root_path = config.root.join(&module.root);
        if !root_path.exists() {
            return;
        }

        match cmod_build::runner::extract_module_name(&root_path) {
            Ok(Some(declared_name)) => {
                if declared_name != module.name {
                    errors.push(format!(
                        "module root declares 'export module {};' but manifest says '{}'",
                        declared_name, module.name
                    ));
                } else {
                    shell.verbose(
                        "Declaration",
                        format!("module declaration matches manifest: {}", module.name),
                    );
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

fn validate_security_policy(
    config: &Config,
    errors: &mut Vec<String>,
    warnings: &mut Vec<String>,
    shell: &Shell,
) {
    let policy = SecurityPolicy::from_manifest(config.manifest.security.as_ref());

    if !policy.is_active() {
        shell.verbose("Policy", "no security policy configured");
        return;
    }

    shell.verbose("Enforcing", "security policy...");

    let lockfile = match Lockfile::load(&config.lockfile_path) {
        Ok(l) => l,
        Err(_) => return,
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

fn validate_signatures(
    config: &Config,
    errors: &mut Vec<String>,
    _warnings: &mut Vec<String>,
    shell: &Shell,
) {
    shell.verbose("Checking", "commit signatures...");

    let lockfile = match Lockfile::load(&config.lockfile_path) {
        Ok(l) => l,
        Err(_) => return,
    };

    if lockfile.packages.is_empty() {
        return;
    }

    let deps_dir = config.deps_dir();
    let results = verify_all_packages(&lockfile.packages, &deps_dir, true);

    for result in &results {
        match &result.signature_status {
            SignatureStatus::Valid { signer } => {
                shell.verbose(
                    "Signature",
                    format!("{} — signed by {}", result.package_name, signer),
                );
            }
            SignatureStatus::Untrusted { signer } => {
                shell.verbose(
                    "Signature",
                    format!(
                        "{} — signed by {} (untrusted key)",
                        result.package_name, signer
                    ),
                );
            }
            SignatureStatus::Unsigned => {
                shell.verbose(
                    "Signature",
                    format!("{} — unsigned commit", result.package_name),
                );
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

fn validate_artifact_signatures(
    config: &Config,
    errors: &mut Vec<String>,
    warnings: &mut Vec<String>,
    shell: &Shell,
) {
    shell.verbose("Checking", "artifact signatures...");

    let signing_config = config.manifest.security.as_ref().and_then(|sec| {
        resolve_signing_config(sec.signing_key.as_deref(), sec.signing_backend.as_deref())
    });

    let lockfile = match Lockfile::load(&config.lockfile_path) {
        Ok(l) => l,
        Err(_) => return,
    };

    if lockfile.packages.is_empty() {
        return;
    }

    let deps_dir = config.deps_dir();

    for pkg in &lockfile.packages {
        let repo_path = deps_dir.join(&pkg.name);
        if !repo_path.exists() {
            continue;
        }

        // Check for a detached signature file alongside the repo
        let sig_path = repo_path.with_extension("sig");
        if !sig_path.exists() {
            shell.verbose(
                "Signature",
                format!("{} — no artifact signature file", pkg.name),
            );
            continue;
        }

        // Find the artifact to verify (look for common artifact files)
        let artifact_candidates = [repo_path.join("cmod.toml")];

        for artifact in &artifact_candidates {
            if !artifact.exists() {
                continue;
            }

            let artifact_sig = artifact.with_extension(format!(
                "{}.sig",
                artifact.extension().unwrap_or_default().to_string_lossy()
            ));

            let effective_sig = if artifact_sig.exists() {
                artifact_sig
            } else if sig_path.exists() {
                sig_path.clone()
            } else {
                continue;
            };

            match verify_file(artifact, &effective_sig, signing_config.as_ref()) {
                Ok(status) => match status {
                    VerifyStatus::Valid { signer, backend } => {
                        shell.verbose(
                            "Signature",
                            format!(
                                "{} — verified ({}, signed by {})",
                                pkg.name,
                                backend.as_str(),
                                signer,
                            ),
                        );
                    }
                    VerifyStatus::Untrusted {
                        signer,
                        backend,
                        reason,
                    } => {
                        warnings.push(format!(
                            "package '{}' signature untrusted ({}, signer: {}): {}",
                            pkg.name,
                            backend.as_str(),
                            signer,
                            reason,
                        ));
                    }
                    VerifyStatus::Unsigned => {
                        shell.verbose("Signature", format!("{} — unsigned artifact", pkg.name));
                    }
                    VerifyStatus::Invalid { reason } => {
                        errors.push(format!(
                            "package '{}' has invalid artifact signature: {}",
                            pkg.name, reason,
                        ));
                    }
                },
                Err(e) => {
                    warnings.push(format!(
                        "could not verify artifact signature for '{}': {}",
                        pkg.name, e,
                    ));
                }
            }
        }
    }
}
