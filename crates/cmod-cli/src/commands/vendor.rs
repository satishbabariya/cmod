use std::path::Path;

use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::lockfile::Lockfile;

/// Run `cmod vendor` — vendor dependencies for offline builds.
pub fn run(sync: bool, verbose: bool) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    let lockfile = Lockfile::load(&config.lockfile_path)?;

    let vendor_dir = config.root.join("vendor");

    if sync {
        eprintln!("  Re-synchronizing vendor directory...");
        // Remove stale entries
        if vendor_dir.exists() {
            remove_stale_entries(&vendor_dir, &lockfile)?;
        }
    }

    std::fs::create_dir_all(&vendor_dir)?;

    let mut vendored = 0;

    for pkg in &lockfile.packages {
        let pkg_dir = vendor_dir.join(&pkg.name);

        // Skip if already vendored and not syncing
        if pkg_dir.exists() && !sync {
            if verbose {
                eprintln!("    Already vendored: {}", pkg.name);
            }
            vendored += 1;
            continue;
        }

        // Vendor from the resolved dependency source
        let source = pkg.source.as_deref().unwrap_or("git");
        match source {
            "git" => {
                vendor_git_dep(&config, pkg, &pkg_dir, verbose)?;
            }
            "path" => {
                vendor_path_dep(pkg, &pkg_dir, verbose)?;
            }
            _ => {
                eprintln!("    Skipping {} (unknown source: {})", pkg.name, source);
                continue;
            }
        }

        vendored += 1;
    }

    // Generate vendor config
    generate_vendor_config(&vendor_dir, &lockfile)?;

    eprintln!(
        "  Vendored {} dependencies into {}",
        vendored,
        vendor_dir.display()
    );

    Ok(())
}

/// Vendor a Git-sourced dependency by copying from the deps checkout.
fn vendor_git_dep(
    config: &Config,
    pkg: &cmod_core::lockfile::LockedPackage,
    dest: &Path,
    verbose: bool,
) -> Result<(), CmodError> {
    let deps_dir = config.deps_dir();
    let checkout = deps_dir.join(&pkg.name);

    if checkout.exists() {
        if verbose {
            eprintln!("    Copying {} from deps checkout...", pkg.name);
        }
        copy_dir_recursive(&checkout, dest)?;
    } else if let Some(ref repo_url) = pkg.repo {
        if verbose {
            eprintln!("    Cloning {} for vendor...", pkg.name);
        }
        // Clone the repo at the pinned commit
        let repo = git2::Repository::clone(repo_url, dest).map_err(|e| CmodError::GitError {
            reason: format!("failed to clone {}: {}", repo_url, e),
        })?;

        // Checkout the specific commit
        if let Some(ref commit_hash) = pkg.commit {
            let oid =
                git2::Oid::from_str(commit_hash).map_err(|e| CmodError::GitError {
                    reason: format!("invalid commit hash: {}", e),
                })?;
            let commit = repo.find_commit(oid).map_err(|e| CmodError::GitError {
                reason: format!("commit not found: {}", e),
            })?;
            repo.checkout_tree(commit.as_object(), None)
                .map_err(|e| CmodError::GitError {
                    reason: format!("checkout failed: {}", e),
                })?;
            repo.set_head_detached(oid)
                .map_err(|e| CmodError::GitError {
                    reason: format!("detach head failed: {}", e),
                })?;
        }
    } else {
        eprintln!("    Warning: no source for {}, skipping", pkg.name);
    }

    Ok(())
}

/// Vendor a path-sourced dependency by symlinking or copying.
fn vendor_path_dep(
    pkg: &cmod_core::lockfile::LockedPackage,
    dest: &Path,
    verbose: bool,
) -> Result<(), CmodError> {
    if verbose {
        eprintln!("    Symlinking {} (path dep)", pkg.name);
    }
    // For path deps, we record the mapping in vendor config
    // but don't copy — they're already local
    std::fs::create_dir_all(dest)?;
    std::fs::write(
        dest.join(".cmod-path-dep"),
        format!("source = path\nname = {}\n", pkg.name),
    )?;
    Ok(())
}

/// Remove vendored entries that are no longer in the lockfile.
fn remove_stale_entries(vendor_dir: &Path, lockfile: &Lockfile) -> Result<(), CmodError> {
    let locked_names: std::collections::HashSet<&str> =
        lockfile.packages.iter().map(|p| p.name.as_str()).collect();

    if let Ok(entries) = std::fs::read_dir(vendor_dir) {
        for entry in entries.flatten() {
            if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                let name = entry.file_name();
                let name_str = name.to_string_lossy();
                if name_str != "config.toml" && !locked_names.contains(name_str.as_ref()) {
                    let _ = std::fs::remove_dir_all(entry.path());
                }
            }
        }
    }

    Ok(())
}

/// Generate vendor/config.toml mapping deps to local paths.
fn generate_vendor_config(vendor_dir: &Path, lockfile: &Lockfile) -> Result<(), CmodError> {
    let mut config = String::from("# Auto-generated by `cmod vendor`\n\n");

    for pkg in &lockfile.packages {
        config.push_str(&format!(
            "[source.\"{}\"]\npath = \"{}/{}\"\n",
            pkg.name,
            vendor_dir.display(),
            pkg.name,
        ));
        if let Some(ref commit) = pkg.commit {
            config.push_str(&format!("commit = \"{}\"\n", commit));
        }
        config.push('\n');
    }

    std::fs::write(vendor_dir.join("config.toml"), config)?;
    Ok(())
}

/// Recursively copy a directory.
fn copy_dir_recursive(src: &Path, dest: &Path) -> Result<(), CmodError> {
    std::fs::create_dir_all(dest)?;

    for entry in walkdir::WalkDir::new(src)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let relative = entry.path().strip_prefix(src).unwrap_or(entry.path());
        let target = dest.join(relative);

        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&target)?;
        } else {
            if let Some(parent) = target.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::copy(entry.path(), &target)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_copy_dir_recursive() {
        let src = TempDir::new().unwrap();
        let dest = TempDir::new().unwrap();

        std::fs::write(src.path().join("a.txt"), "hello").unwrap();
        let sub = src.path().join("sub");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::write(sub.join("b.txt"), "world").unwrap();

        let dest_path = dest.path().join("out");
        copy_dir_recursive(src.path(), &dest_path).unwrap();

        assert!(dest_path.join("a.txt").exists());
        assert!(dest_path.join("sub/b.txt").exists());
        assert_eq!(
            std::fs::read_to_string(dest_path.join("a.txt")).unwrap(),
            "hello"
        );
    }

    #[test]
    fn test_generate_vendor_config() {
        let tmp = TempDir::new().unwrap();
        let lockfile = Lockfile {
            version: 1,
            packages: vec![cmod_core::lockfile::LockedPackage {
                name: "fmt".to_string(),
                version: "10.2.0".to_string(),
                source: Some("git".to_string()),
                repo: Some("https://github.com/fmtlib/fmt".to_string()),
                commit: Some("abc123".to_string()),
                hash: None,
                toolchain: None,
                targets: std::collections::BTreeMap::new(),
                deps: vec![],
                features: vec![],
            }],
        };

        generate_vendor_config(tmp.path(), &lockfile).unwrap();
        let content = std::fs::read_to_string(tmp.path().join("config.toml")).unwrap();
        assert!(content.contains("[source.\"fmt\"]"));
        assert!(content.contains("commit = \"abc123\""));
    }

    #[test]
    fn test_remove_stale_entries() {
        let tmp = TempDir::new().unwrap();
        let stale = tmp.path().join("old_dep");
        std::fs::create_dir_all(&stale).unwrap();
        std::fs::write(stale.join("file.txt"), "x").unwrap();

        let lockfile = Lockfile {
            version: 1,
            packages: vec![],
        };

        remove_stale_entries(tmp.path(), &lockfile).unwrap();
        assert!(!stale.exists());
    }
}
