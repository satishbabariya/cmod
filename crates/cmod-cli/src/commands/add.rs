use std::path::PathBuf;

use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::lockfile::Lockfile;
use cmod_core::manifest::{Dependency, DetailedDependency};
use cmod_resolver::Resolver;

/// Run `cmod add <dep>` — add a dependency.
pub fn run(
    dep: String,
    git: Option<String>,
    branch: Option<String>,
    rev: Option<String>,
    path: Option<String>,
    features: Vec<String>,
    _locked: bool,
    _offline: bool,
) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let mut config = Config::load(&cwd)?;

    // Parse the dep specifier: "github.com/fmtlib/fmt@^10.2" or "github.com/fmtlib/fmt"
    let (dep_key, version_constraint) = parse_dep_specifier(&dep);

    // Build the Dependency object
    let dependency = if path.is_some() || git.is_some() || branch.is_some() || rev.is_some() || !features.is_empty() {
        Dependency::Detailed(DetailedDependency {
            version: version_constraint,
            git,
            branch,
            rev,
            tag: None,
            path: path.map(PathBuf::from),
            features,
            optional: false,
            default_features: true,
            workspace: false,
        })
    } else if let Some(ver) = version_constraint {
        Dependency::Simple(ver)
    } else {
        Dependency::Simple("*".to_string())
    };

    // Load existing lockfile if present
    let existing_lock = Lockfile::load(&config.lockfile_path).ok();

    // Add dependency and resolve
    let mut resolver = Resolver::new(config.deps_dir());
    let lockfile = resolver.add_dependency(
        &mut config.manifest,
        dep_key.clone(),
        dependency,
        existing_lock.as_ref(),
    )?;

    // Save updated manifest and lockfile
    config.manifest.save(&config.manifest_path)?;
    lockfile.save(&config.lockfile_path)?;

    if let Some(pkg) = lockfile.find_package(&dep_key) {
        eprintln!("  Added dependency '{}' v{}", dep_key, pkg.version);
    } else {
        eprintln!("  Added dependency '{}'", dep_key);
    }

    Ok(())
}

/// Parse a dependency specifier like `github.com/fmtlib/fmt@^10.2`.
fn parse_dep_specifier(spec: &str) -> (String, Option<String>) {
    if let Some(idx) = spec.find('@') {
        let key = spec[..idx].to_string();
        let version = spec[idx + 1..].to_string();
        (key, Some(version))
    } else {
        (spec.to_string(), None)
    }
}
