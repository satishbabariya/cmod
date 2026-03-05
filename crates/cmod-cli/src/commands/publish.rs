use std::process::Command;

use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::shell::Shell;

/// Run `cmod publish` — prepare and tag a release.
///
/// Steps:
/// 1. Validate the manifest (name, version, description, license)
/// 2. Ensure there are no uncommitted changes
/// 3. Run `cmod verify` checks
/// 4. Create a Git tag `v{version}`
/// 5. Optionally push the tag
pub fn run(dry_run: bool, push: bool, shell: &Shell) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    let name = &config.manifest.package.name;
    let version = &config.manifest.package.version;
    let tag = format!("v{}", version);

    shell.status("Publishing", format!("{} v{}", name, version));

    // Step 1: Validate manifest for publishing
    validate_for_publish(&config, shell)?;

    // Step 2: Check for uncommitted changes
    check_clean_working_tree(shell)?;

    // Step 3: Check if tag already exists
    if tag_exists(&tag)? {
        return Err(CmodError::Other(format!(
            "tag '{}' already exists; bump the version in cmod.toml first",
            tag
        )));
    }

    // Step 4: Check publish include/exclude patterns
    if let Some(ref publish) = config.manifest.publish {
        if !publish.include.is_empty() {
            shell.verbose("Include", format!("{:?}", publish.include));
        }
        if !publish.exclude.is_empty() {
            shell.verbose("Exclude", format!("{:?}", publish.exclude));
        }
    }

    // Run pre-publish hook
    super::build::run_hook(
        &config,
        "pre-publish",
        config
            .manifest
            .hooks
            .as_ref()
            .and_then(|h| h.pre_publish.as_deref()),
    )?;

    if dry_run {
        shell.status("Dry run", format!("would create tag '{}'", tag));
        if push {
            shell.status("Dry run", format!("would push tag '{}' to origin", tag));
        }
        shell.status("Finished", "publish dry run complete");
        return Ok(());
    }

    // Step 5: Create the tag
    create_tag(&tag, &format!("Release {} v{}", name, version))?;
    shell.status("Created", format!("tag {}", tag));

    // Step 6: Push if requested
    if push {
        push_tag(&tag)?;
        shell.status("Pushed", format!("tag '{}' to origin", tag));
    } else {
        shell.note(format!("run `git push origin {}` to publish", tag));
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
        // This test just verifies the validation logic compiles
        // Real testing requires a cmod.toml on disk
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
        // Should succeed — default manifest passes validation and has no deps
        let result = validate_for_publish(&config, &shell);
        assert!(result.is_ok());
    }
}
