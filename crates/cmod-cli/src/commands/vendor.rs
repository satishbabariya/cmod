use std::path::Path;

use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::lockfile::Lockfile;
use cmod_core::shell::Shell;
use cmod_core::types::{is_acceptable_package_name, sanitize_package_name_for_path};

/// Run `cmod vendor` — vendor dependencies for offline builds.
pub fn run(sync: bool, shell: &Shell) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    let lockfile = Lockfile::load(&config.lockfile_path)?;

    let vendor_dir = config.root.join("vendor");

    if sync {
        shell.status("Syncing", "vendor directory...");
        if vendor_dir.exists() {
            remove_stale_entries(&vendor_dir, &lockfile)?;
        }
    }

    std::fs::create_dir_all(&vendor_dir)?;

    let mut vendored = 0;

    for pkg in &lockfile.packages {
        // Validate package name — reject traversal sequences, nulls, etc.
        // Slashes are permitted (Git-URL naming) and encoded on-disk below.
        if !is_acceptable_package_name(&pkg.name) {
            return Err(CmodError::SecurityViolation {
                reason: format!(
                    "unsafe package name in lockfile: '{}' contains path traversal or invalid characters",
                    pkg.name
                ),
            });
        }

        let safe_component = sanitize_package_name_for_path(&pkg.name);
        let pkg_dir = vendor_dir.join(&safe_component);

        if pkg_dir.exists() && !sync {
            shell.verbose("Vendored", format!("{} (already)", pkg.name));
            vendored += 1;
            continue;
        }

        let source = pkg.source.as_deref().unwrap_or("git");
        match source {
            "git" => {
                vendor_git_dep(&config, pkg, &pkg_dir, shell)?;
            }
            "path" => {
                vendor_path_dep(pkg, &pkg_dir, shell)?;
            }
            _ => {
                shell.warn(format!(
                    "skipping {} (unknown source: {})",
                    pkg.name, source
                ));
                continue;
            }
        }

        vendored += 1;
    }

    generate_vendor_config(&vendor_dir, &lockfile)?;

    shell.status(
        "Vendored",
        format!("{} dependencies into {}", vendored, vendor_dir.display()),
    );

    Ok(())
}

/// Vendor a Git-sourced dependency by copying from the deps checkout.
fn vendor_git_dep(
    config: &Config,
    pkg: &cmod_core::lockfile::LockedPackage,
    dest: &Path,
    shell: &Shell,
) -> Result<(), CmodError> {
    // Package name is already validated in run(), but double-check for safety
    if !is_acceptable_package_name(&pkg.name) {
        return Err(CmodError::SecurityViolation {
            reason: format!("unsafe package name: '{}'", pkg.name),
        });
    }

    let deps_dir = config.deps_dir();
    let checkout = deps_dir.join(sanitize_package_name_for_path(&pkg.name));

    if checkout.exists() {
        shell.verbose("Copying", format!("{} from deps checkout...", pkg.name));
        copy_dir_recursive(&checkout, dest)?;
    } else if let Some(ref repo_url) = pkg.repo {
        shell.verbose("Cloning", format!("{} for vendor...", pkg.name));
        let repo = open_or_clone(repo_url, dest)?;

        if let Some(ref commit_hash) = pkg.commit {
            let oid = git2::Oid::from_str(commit_hash).map_err(|e| CmodError::GitError {
                reason: format!("invalid commit hash: {}", e),
            })?;
            cmod_resolver::git::checkout_commit(&repo, oid)?;
        }
    } else {
        shell.warn(format!("no source for {}, skipping", pkg.name));
    }

    Ok(())
}

/// Open an existing vendored clone or clone fresh.
///
/// A previous `vendor --sync` may have left a non-empty directory here, and
/// git2 refuses to clone into one (#38). Reuse the directory when it is a
/// valid repo (fetching so a newly locked commit is available); otherwise
/// clear it and clone from scratch. Fetch failures are tolerated so offline
/// re-syncs still work when the locked commit is already present.
fn open_or_clone(url: &str, dest: &Path) -> Result<git2::Repository, CmodError> {
    if dest.join(".git").exists() {
        let repo = git2::Repository::open(dest).map_err(|e| CmodError::GitError {
            reason: format!("failed to open vendored repo at {}: {}", dest.display(), e),
        })?;
        if let Ok(mut remote) = repo.find_remote("origin") {
            let _ = remote.fetch(&[] as &[&str], None, None);
        }
        return Ok(repo);
    }

    if dest.exists() {
        std::fs::remove_dir_all(dest)?;
    }
    git2::Repository::clone(url, dest).map_err(|e| CmodError::GitError {
        reason: format!("failed to clone {}: {}", url, e),
    })
}

/// Vendor a path-sourced dependency by symlinking or copying.
fn vendor_path_dep(
    pkg: &cmod_core::lockfile::LockedPackage,
    dest: &Path,
    shell: &Shell,
) -> Result<(), CmodError> {
    shell.verbose("Linking", format!("{} (path dep)", pkg.name));
    std::fs::create_dir_all(dest)?;
    std::fs::write(
        dest.join(".cmod-path-dep"),
        format!("source = path\nname = {}\n", pkg.name),
    )?;
    Ok(())
}

/// Remove vendored entries that are no longer in the lockfile.
fn remove_stale_entries(vendor_dir: &Path, lockfile: &Lockfile) -> Result<(), CmodError> {
    let locked_names: std::collections::HashSet<String> = lockfile
        .packages
        .iter()
        .map(|p| sanitize_package_name_for_path(&p.name))
        .collect();

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
        let safe = sanitize_package_name_for_path(&pkg.name);
        config.push_str(&format!(
            "[source.\"{}\"]\npath = \"{}/{}\"\n",
            pkg.name,
            vendor_dir.display(),
            safe,
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
    use cmod_core::lockfile::LockedPackage;
    use cmod_core::shell::Verbosity;
    use tempfile::TempDir;

    /// Create a git repo with one committed file; return the commit hash.
    fn init_fixture_repo(dir: &Path) -> String {
        std::fs::create_dir_all(dir).unwrap();
        let repo = git2::Repository::init(dir).unwrap();
        std::fs::write(dir.join("lib.cppm"), "export module fixture;").unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("lib.cppm")).unwrap();
        index.write().unwrap();
        let tree_id = index.write_tree().unwrap();
        let tree = repo.find_tree(tree_id).unwrap();
        let sig = git2::Signature::now("test", "test@example.com").unwrap();
        let oid = repo
            .commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .unwrap();
        oid.to_string()
    }

    fn make_locked_pkg(name: &str, repo_url: &str, commit: &str) -> LockedPackage {
        LockedPackage {
            name: name.to_string(),
            version: "1.0.0".to_string(),
            source: Some("git".to_string()),
            repo: Some(repo_url.to_string()),
            commit: Some(commit.to_string()),
            hash: None,
            toolchain: None,
            targets: Default::default(),
            deps: vec![],
            features: vec![],
        }
    }

    fn setup_project(tmp: &TempDir) -> Config {
        let project = tmp.path().join("project");
        std::fs::create_dir_all(&project).unwrap();
        std::fs::write(
            project.join("cmod.toml"),
            "[package]\nname = \"test_project\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();
        Config::load(&project).unwrap()
    }

    fn quiet_shell() -> Shell {
        Shell::from_write(Box::new(std::io::sink()), Verbosity::Quiet)
    }

    /// Regression test for #38: a second `vendor --sync` must not fail when
    /// the destination directory already exists with stale (non-repo) content.
    #[test]
    fn test_vendor_git_dep_over_existing_non_repo_dest() {
        let tmp = TempDir::new().unwrap();
        let upstream = tmp.path().join("upstream");
        let commit = init_fixture_repo(&upstream);
        let config = setup_project(&tmp);

        // Simulate leftovers from a previous vendor run: non-empty, not a repo.
        let dest = config.root.join("vendor").join("dep");
        std::fs::create_dir_all(&dest).unwrap();
        std::fs::write(dest.join("stale.txt"), "old").unwrap();

        let pkg = make_locked_pkg("dep", upstream.to_str().unwrap(), &commit);
        vendor_git_dep(&config, &pkg, &dest, &quiet_shell()).unwrap();

        assert!(dest.join("lib.cppm").exists());
        assert!(!dest.join("stale.txt").exists());
    }

    /// Regression test for #38: re-syncing over a previously vendored clone
    /// must reuse the repo and hard-reset to the locked commit.
    #[test]
    fn test_vendor_git_dep_over_existing_clone() {
        let tmp = TempDir::new().unwrap();
        let upstream = tmp.path().join("upstream");
        let commit = init_fixture_repo(&upstream);
        let config = setup_project(&tmp);

        let dest = config.root.join("vendor").join("dep");
        let pkg = make_locked_pkg("dep", upstream.to_str().unwrap(), &commit);
        let shell = quiet_shell();

        // First vendor clones; second must succeed over the existing clone,
        // discarding local modifications (hard reset to the locked commit).
        vendor_git_dep(&config, &pkg, &dest, &shell).unwrap();
        std::fs::write(dest.join("lib.cppm"), "local modification").unwrap();
        vendor_git_dep(&config, &pkg, &dest, &shell).unwrap();

        assert_eq!(
            std::fs::read_to_string(dest.join("lib.cppm")).unwrap(),
            "export module fixture;"
        );
    }

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
            integrity: None,
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
            integrity: None,
            packages: vec![],
        };

        remove_stale_entries(tmp.path(), &lockfile).unwrap();
        assert!(!stale.exists());
    }
}
