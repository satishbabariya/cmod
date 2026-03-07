use std::process::Command;

use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::shell::Shell;
use cmod_resolver::registry::{validate_for_publishing, GovernancePolicy, PublishModuleParams};
use cmod_security::signing::{resolve_signing_config, sign_data};

/// Run `cmod publish` — prepare and tag a release.
///
/// Steps:
/// 1. Validate the manifest (name, version, description, license)
/// 2. Validate governance policy
/// 3. Ensure there are no uncommitted changes
/// 4. Run `cmod verify` checks
/// 5. Create a Git tag `v{version}` (optionally signed)
/// 6. Optionally push the tag
pub fn run(
    dry_run: bool,
    push: bool,
    sign: bool,
    no_sign: bool,
    skip_governance: bool,
    shell: &Shell,
) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    let name = &config.manifest.package.name;
    let version = &config.manifest.package.version;
    let tag = format!("v{}", version);

    shell.status("Publishing", format!("{} v{}", name, version));

    // Step 1: Validate manifest for publishing
    validate_for_publish(&config, shell)?;

    // Step 2: Validate governance policy
    if !skip_governance {
        validate_governance(&config, shell)?;
    }

    // Step 3: Check for uncommitted changes
    check_clean_working_tree(shell)?;

    // Step 4: Check if tag already exists
    if tag_exists(&tag)? {
        return Err(CmodError::Other(format!(
            "tag '{}' already exists; bump the version in cmod.toml first",
            tag
        )));
    }

    // Step 5: Check publish include/exclude patterns
    if let Some(ref publish) = config.manifest.publish {
        if !publish.include.is_empty() {
            shell.verbose("Include", format!("{:?}", publish.include));
        }
        if !publish.exclude.is_empty() {
            shell.verbose("Exclude", format!("{:?}", publish.exclude));
        }
    }

    // Determine if we should sign
    let should_sign = if no_sign {
        false
    } else if sign {
        true
    } else {
        // Auto-detect from [security] config
        config
            .manifest
            .security
            .as_ref()
            .and_then(|s| s.signing_key.as_ref())
            .is_some()
    };

    // Resolve signing config if needed
    let signing_config = if should_sign {
        let sec = config.manifest.security.as_ref();
        let cfg = resolve_signing_config(
            sec.and_then(|s| s.signing_key.as_deref()),
            sec.and_then(|s| s.signing_backend.as_deref()),
        );
        if cfg.is_none() {
            return Err(CmodError::Other(
                "--sign requested but no signing key configured in [security]".to_string(),
            ));
        }
        cfg
    } else {
        None
    };

    // Run pre-publish hook
    super::build::run_hook(
        &config,
        "pre-publish",
        config
            .manifest
            .hooks
            .as_ref()
            .and_then(|h| h.pre_publish.as_deref()),
        shell,
    )?;

    if dry_run {
        shell.status("Dry run", format!("would create tag '{}'", tag));
        if should_sign {
            shell.status("Dry run", "tag would be signed");
        }
        if push {
            shell.status("Dry run", format!("would push tag '{}' to origin", tag));
        }
        shell.status("Finished", "publish dry run complete");
        return Ok(());
    }

    // Step 6: Create the tag (signed or unsigned)
    let message = format!("Release {} v{}", name, version);
    if let Some(ref cfg) = signing_config {
        create_signed_tag(&tag, &message, cfg, shell)?;
    } else {
        create_tag(&tag, &message)?;
    }
    shell.status("Created", format!("tag {}", tag));

    // Step 7: Push if requested
    if push {
        push_tag(&tag)?;
        shell.status("Pushed", format!("tag '{}' to origin", tag));
    } else {
        shell.note(format!("run `git push origin {}` to publish", tag));
    }

    // Step 8: Publish to registry if configured
    if let Some(ref publish) = config.manifest.publish {
        if let Some(ref registry_url) = publish.registry {
            publish_to_registry(name, version, &tag, &config, registry_url, shell);
        }
    }

    shell.status("Published", format!("{} v{}", name, version));
    Ok(())
}

/// Validate that the manifest has enough info for publishing.
fn validate_for_publish(config: &Config, shell: &Shell) -> Result<(), CmodError> {
    let manifest = &config.manifest;

    // Run standard validation
    manifest.validate()?;

    // Description is recommended
    if manifest.package.description.is_none() {
        shell.warn("package.description is not set");
    }

    // License is recommended
    if manifest.package.license.is_none() {
        shell.warn("package.license is not set");
    }

    // Check lockfile exists
    if !config.lockfile_path.exists() && !manifest.dependencies.is_empty() {
        return Err(CmodError::Other(
            "lockfile not found; run `cmod resolve` before publishing".to_string(),
        ));
    }

    Ok(())
}

/// Validate against governance policy.
fn validate_governance(config: &Config, shell: &Shell) -> Result<(), CmodError> {
    let policy = GovernancePolicy::default();
    let manifest = &config.manifest;

    let violations = validate_for_publishing(
        &manifest.package.name,
        &manifest.package.version,
        manifest.package.description.as_deref(),
        manifest.package.license.as_deref(),
        &policy,
    );

    if violations.is_empty() {
        shell.verbose("Governance", "policy validation passed");
        return Ok(());
    }

    for violation in &violations {
        shell.warn(format!("[governance] {}", violation));
    }

    Err(CmodError::Other(format!(
        "governance policy validation failed with {} issue(s); use --skip-governance to bypass",
        violations.len(),
    )))
}

/// Check that the Git working tree is clean.
fn check_clean_working_tree(shell: &Shell) -> Result<(), CmodError> {
    let output = Command::new("git")
        .args(["status", "--porcelain"])
        .output()
        .map_err(|e| CmodError::Other(format!("git not available: {}", e)))?;

    let status = String::from_utf8_lossy(&output.stdout);
    let dirty_lines: Vec<&str> = status.lines().filter(|l| !l.is_empty()).collect();

    if !dirty_lines.is_empty() {
        for line in &dirty_lines {
            shell.verbose("Dirty", *line);
        }
        return Err(CmodError::Other(format!(
            "working tree has {} uncommitted change(s); commit or stash first",
            dirty_lines.len()
        )));
    }

    Ok(())
}

/// Check if a Git tag already exists.
fn tag_exists(tag: &str) -> Result<bool, CmodError> {
    let output = Command::new("git")
        .args(["tag", "--list", tag])
        .output()
        .map_err(|e| CmodError::Other(format!("git not available: {}", e)))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    Ok(stdout.lines().any(|l| l.trim() == tag))
}

/// Create a Git tag.
fn create_tag(tag: &str, message: &str) -> Result<(), CmodError> {
    let status = Command::new("git")
        .args(["tag", "-a", tag, "-m", message])
        .status()
        .map_err(|e| CmodError::Other(format!("git not available: {}", e)))?;

    if !status.success() {
        return Err(CmodError::Other(format!("failed to create tag '{}'", tag)));
    }

    Ok(())
}

/// Create a signed Git tag using the configured signing backend.
fn create_signed_tag(
    tag: &str,
    message: &str,
    signing_config: &cmod_security::signing::SigningConfig,
    shell: &Shell,
) -> Result<(), CmodError> {
    // For PGP, use git tag -s which delegates to gpg
    match signing_config.backend {
        cmod_security::signing::SigningBackend::Pgp => {
            let mut cmd = Command::new("git");
            cmd.args(["tag", "-s", tag, "-m", message]);
            if let Some(ref key_id) = signing_config.key_id {
                cmd.args(["-u", key_id]);
            }
            let status = cmd
                .status()
                .map_err(|e| CmodError::Other(format!("git not available: {}", e)))?;
            if !status.success() {
                return Err(CmodError::Other(format!(
                    "failed to create signed tag '{}'",
                    tag
                )));
            }
            shell.verbose("Signed", format!("tag {} with PGP", tag));
        }
        _ => {
            // For SSH and Sigstore, create tag then sign the tag data
            create_tag(tag, message)?;
            let tag_data = format!("tag {}\nmessage {}", tag, message);
            match sign_data(signing_config, tag_data.as_bytes()) {
                Ok(result) => {
                    shell.verbose(
                        "Signed",
                        format!(
                            "tag {} with {} (signer: {})",
                            tag,
                            result.backend.as_str(),
                            result.signer,
                        ),
                    );
                    // Write signature alongside the tag
                    let sig_file = format!("{}.sig", tag);
                    if let Err(e) = std::fs::write(&sig_file, &result.signature) {
                        shell.warn(format!("could not write signature file: {}", e));
                    }
                }
                Err(e) => {
                    return Err(CmodError::Other(format!("failed to sign tag: {}", e)));
                }
            }
        }
    }

    Ok(())
}

/// Push a Git tag to origin.
fn push_tag(tag: &str) -> Result<(), CmodError> {
    let status = Command::new("git")
        .args(["push", "origin", tag])
        .status()
        .map_err(|e| CmodError::Other(format!("git not available: {}", e)))?;

    if !status.success() {
        return Err(CmodError::Other(format!(
            "failed to push tag '{}' to origin",
            tag
        )));
    }

    Ok(())
}

/// Publish module metadata to a registry after tagging.
fn publish_to_registry(
    name: &str,
    version: &str,
    tag: &str,
    config: &Config,
    registry_url: &str,
    shell: &Shell,
) {
    let cache_dir = dirs::cache_dir()
        .unwrap_or_else(|| std::path::PathBuf::from(".cache"))
        .join("cmod");
    let client = cmod_resolver::RegistryClient::new(registry_url, cache_dir);

    // Resolve the current commit hash for the tag
    let commit = resolve_head_commit().unwrap_or_else(|| "unknown".to_string());

    // Derive repository URL from module name (reverse-domain → Git URL)
    let repository = config
        .manifest
        .module
        .as_ref()
        .map(|m| format!("https://{}", m.name.replace('.', "/")))
        .unwrap_or_else(|| format!("https://{}", name.replace('.', "/")));

    let params = PublishModuleParams {
        name: name.to_string(),
        version: version.to_string(),
        tag: tag.to_string(),
        commit,
        description: config.manifest.package.description.clone(),
        license: config.manifest.package.license.clone(),
        repository,
    };

    match client.publish_module(&params) {
        Ok(()) => {
            shell.status(
                "Registry",
                format!("published {} v{} to registry", name, version),
            );
        }
        Err(e) => {
            shell.warn(format!("failed to publish to registry: {}", e));
            shell.note("the git tag was created successfully; registry publish can be retried");
        }
    }
}

/// Resolve HEAD commit hash.
fn resolve_head_commit() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;
    if output.status.success() {
        Some(String::from_utf8_lossy(&output.stdout).trim().to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tag_format() {
        let version = "1.2.3";
        let tag = format!("v{}", version);
        assert_eq!(tag, "v1.2.3");
    }

    #[test]
    fn test_validate_for_publish_minimal() {
        let config = Config {
            root: std::path::PathBuf::from("/tmp/nonexistent"),
            manifest_path: std::path::PathBuf::from("/tmp/nonexistent/cmod.toml"),
            manifest: cmod_core::manifest::default_manifest("test-pub"),
            lockfile_path: std::path::PathBuf::from("/tmp/nonexistent/cmod.lock"),
            profile: cmod_core::types::Profile::Debug,
            locked: false,
            offline: false,
            verbosity: cmod_core::shell::Verbosity::Normal,
            target: None,
            enabled_features: vec![],
            no_default_features: false,
            no_cache: false,
        };

        let shell = cmod_core::shell::Shell::new(cmod_core::shell::Verbosity::Normal);
        let result = validate_for_publish(&config, &shell);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_governance_passes() {
        let config = Config {
            root: std::path::PathBuf::from("/tmp/nonexistent"),
            manifest_path: std::path::PathBuf::from("/tmp/nonexistent/cmod.toml"),
            manifest: {
                let mut m = cmod_core::manifest::default_manifest("github.user.mylib");
                m.package.description = Some("A great library".to_string());
                m.package.license = Some("MIT".to_string());
                m
            },
            lockfile_path: std::path::PathBuf::from("/tmp/nonexistent/cmod.lock"),
            profile: cmod_core::types::Profile::Debug,
            locked: false,
            offline: false,
            verbosity: cmod_core::shell::Verbosity::Normal,
            target: None,
            enabled_features: vec![],
            no_default_features: false,
            no_cache: false,
        };

        let shell = cmod_core::shell::Shell::new(cmod_core::shell::Verbosity::Normal);
        let result = validate_governance(&config, &shell);
        assert!(result.is_ok());
    }

    #[test]
    fn test_resolve_head_commit_returns_hash() {
        // We're in a git repo, so this should return a commit hash
        let commit = resolve_head_commit();
        assert!(commit.is_some());
        let hash = commit.unwrap();
        assert!(hash.len() >= 7); // At least a short hash
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_validate_governance_fails_for_banned_name() {
        let config = Config {
            root: std::path::PathBuf::from("/tmp/nonexistent"),
            manifest_path: std::path::PathBuf::from("/tmp/nonexistent/cmod.toml"),
            manifest: {
                let mut m = cmod_core::manifest::default_manifest("std.io");
                m.package.description = Some("desc".to_string());
                m.package.license = Some("MIT".to_string());
                m
            },
            lockfile_path: std::path::PathBuf::from("/tmp/nonexistent/cmod.lock"),
            profile: cmod_core::types::Profile::Debug,
            locked: false,
            offline: false,
            verbosity: cmod_core::shell::Verbosity::Normal,
            target: None,
            enabled_features: vec![],
            no_default_features: false,
            no_cache: false,
        };

        let shell = cmod_core::shell::Shell::new(cmod_core::shell::Verbosity::Normal);
        let result = validate_governance(&config, &shell);
        assert!(result.is_err());
    }
}
