use std::collections::BTreeMap;
use std::path::Path;

use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::lockfile::Lockfile;
use cmod_core::shell::Shell;
use cmod_core::types::{is_acceptable_package_name, sanitize_package_name_for_path};

use super::common::{sha256_hex, verify_vendored, VendorChecksum, VENDOR_CHECKSUM_FILE};

/// Run `cmod vendor` — vendor dependencies for offline builds.
pub fn run(sync: bool, offline: bool, shell: &Shell) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let mut config = Config::load(&cwd)?;
    config.offline = offline;

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

        // Already vendored at the locked commit, and unchanged.
        if !sync
            && pkg_dir.join(VENDOR_CHECKSUM_FILE).exists()
            && verify_vendored(pkg, &pkg_dir).is_ok()
        {
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
    // Vendored packages build into their own build/ directories.
    let gitignore = vendor_dir.join(".gitignore");
    if !gitignore.exists() {
        std::fs::write(
            &gitignore,
            "# Written by `cmod vendor`: build outputs of vendored packages\n*/build/\n",
        )?;
    }

    shell.status(
        "Vendored",
        format!("{} dependencies into {}", vendored, vendor_dir.display()),
    );

    Ok(())
}

/// Vendor a Git-sourced dependency: the files of its locked commit, taken
/// from the build's checkout when it has the commit or else from a fresh
/// clone, without the repository itself (a `.git` inside `vendor/` would be
/// committed as an embedded repository, not as files), and a
/// [`VENDOR_CHECKSUM_FILE`] recording the commit and every file's SHA-256.
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
    let Some(commit) = pkg.commit.as_deref() else {
        shell.warn(format!("no locked commit for {}, skipping", pkg.name));
        return Ok(());
    };
    let oid = git2::Oid::from_str(commit).map_err(|e| CmodError::GitError {
        reason: format!("invalid commit hash '{}': {}", commit, e),
    })?;

    let checkout = config
        .deps_dir()
        .join(sanitize_package_name_for_path(&pkg.name));
    let local = git2::Repository::open(&checkout)
        .ok()
        .filter(|repo| repo.find_commit(oid).is_ok());
    // Declared before the clone so it outlives the repository opened in it.
    let clone_dir;
    let repo = match local {
        Some(repo) => {
            shell.verbose("Exporting", format!("{} from the deps checkout", pkg.name));
            repo
        }
        None => {
            let Some(url) = pkg.repo.as_deref() else {
                shell.warn(format!("no source for {}, skipping", pkg.name));
                return Ok(());
            };
            if config.offline {
                return Err(CmodError::GitError {
                    reason: format!(
                        "{} is not checked out at commit {}, and --offline does not fetch",
                        pkg.name, commit
                    ),
                });
            }
            shell.verbose("Cloning", format!("{} for vendor...", pkg.name));
            clone_dir = tempfile::TempDir::new()?;
            git2::Repository::clone(url, clone_dir.path()).map_err(|e| CmodError::GitError {
                reason: format!("failed to clone {}: {}", url, e),
            })?
        }
    };

    export_commit(&repo, oid, dest, &pkg.name, shell)
}

/// Write the files of commit `oid` to `dest`, replacing what is there, and
/// the [`VENDOR_CHECKSUM_FILE`] listing them.
fn export_commit(
    repo: &git2::Repository,
    oid: git2::Oid,
    dest: &Path,
    name: &str,
    shell: &Shell,
) -> Result<(), CmodError> {
    let git_err = |e: git2::Error| CmodError::GitError {
        reason: format!("{}: {}", name, e),
    };
    let tree = repo
        .find_commit(oid)
        .map_err(git_err)?
        .tree()
        .map_err(git_err)?;

    if dest.exists() {
        std::fs::remove_dir_all(dest)?;
    }
    std::fs::create_dir_all(dest)?;

    let mut files = BTreeMap::new();
    let mut failure = None;
    tree.walk(git2::TreeWalkMode::PreOrder, |dir, entry| {
        let Some(entry_name) = entry.name() else {
            return git2::TreeWalkResult::Ok;
        };
        let rel = format!("{}{}", dir, entry_name);
        match entry.kind() {
            Some(git2::ObjectType::Blob) => {
                let result = repo
                    .find_blob(entry.id())
                    .map_err(git_err)
                    .and_then(|blob| write_blob(dest, &rel, entry.filemode(), blob.content()));
                match result {
                    Ok(Some(sum)) => {
                        files.insert(rel, sum);
                    }
                    Ok(None) => {}
                    Err(e) => {
                        failure = Some(e);
                        return git2::TreeWalkResult::Abort;
                    }
                }
            }
            Some(git2::ObjectType::Commit) => {
                shell.warn(format!("{}: submodule {} is not vendored", name, rel));
            }
            _ => {}
        }
        git2::TreeWalkResult::Ok
    })
    .map_err(|e| failure.take().unwrap_or_else(|| git_err(e)))?;

    let checksum = VendorChecksum {
        commit: oid.to_string(),
        files,
    };
    let json = serde_json::to_string_pretty(&checksum)
        .map_err(|e| CmodError::Other(format!("{}: {}", name, e)))?;
    std::fs::write(dest.join(VENDOR_CHECKSUM_FILE), json + "\n")?;
    Ok(())
}

/// Write one blob of the tree to `dest/rel`: a file (executable when git
/// says so) or, on Unix, a symbolic link. Returns the file's SHA-256, or
/// `None` for a link, which the checksums leave out.
fn write_blob(
    dest: &Path,
    rel: &str,
    filemode: i32,
    content: &[u8],
) -> Result<Option<String>, CmodError> {
    let path = dest.join(rel);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if filemode == 0o120000 && write_symlink(&path, content)? {
        return Ok(None);
    }
    std::fs::write(&path, content)?;
    if filemode == 0o100755 {
        make_executable(&path)?;
    }
    Ok(Some(sha256_hex(content)))
}

/// Create the symbolic link `path` pointing at `target`; false where links
/// are not created (Windows), which then gets a file holding the target.
#[cfg(unix)]
fn write_symlink(path: &Path, target: &[u8]) -> std::io::Result<bool> {
    use std::os::unix::ffi::OsStrExt;
    std::os::unix::fs::symlink(std::ffi::OsStr::from_bytes(target), path)?;
    Ok(true)
}

#[cfg(not(unix))]
fn write_symlink(_path: &Path, _target: &[u8]) -> std::io::Result<bool> {
    Ok(false)
}

/// Mark `path` executable, as git records the file.
#[cfg(unix)]
fn make_executable(path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
}

#[cfg(not(unix))]
fn make_executable(_path: &Path) -> std::io::Result<()> {
    Ok(())
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
    let mut config =
        String::from("# Auto-generated by `cmod vendor`; paths are relative to this directory\n\n");

    for pkg in &lockfile.packages {
        let safe = sanitize_package_name_for_path(&pkg.name);
        config.push_str(&format!("[source.\"{}\"]\npath = \"{}\"\n", pkg.name, safe,));
        if let Some(ref commit) = pkg.commit {
            config.push_str(&format!("commit = \"{}\"\n", commit));
        }
        config.push('\n');
    }

    std::fs::write(vendor_dir.join("config.toml"), config)?;
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

    /// The vendored copy is the commit's files with their checksums, no
    /// repository: a `.git` in `vendor/` would be committed as an embedded
    /// repository, not as files.
    #[test]
    fn test_vendor_git_dep_exports_files_with_checksums() {
        let tmp = TempDir::new().unwrap();
        let upstream = tmp.path().join("upstream");
        let commit = init_fixture_repo(&upstream);
        let config = setup_project(&tmp);
        let dest = config.root.join("vendor").join("dep");
        let pkg = make_locked_pkg("dep", upstream.to_str().unwrap(), &commit);

        vendor_git_dep(&config, &pkg, &dest, &quiet_shell()).unwrap();

        assert!(!dest.join(".git").exists());
        let checksum: VendorChecksum = serde_json::from_str(
            &std::fs::read_to_string(dest.join(VENDOR_CHECKSUM_FILE)).unwrap(),
        )
        .unwrap();
        assert_eq!(checksum.commit, commit);
        assert_eq!(
            checksum.files.get("lib.cppm"),
            Some(&sha256_hex(b"export module fixture;"))
        );
        assert!(verify_vendored(&pkg, &dest).is_ok());
    }

    /// Exported from the build's checkout when it has the locked commit, so
    /// vendoring works offline after a build.
    #[test]
    fn test_vendor_git_dep_uses_the_deps_checkout() {
        let tmp = TempDir::new().unwrap();
        let mut config = setup_project(&tmp);
        let checkout = config.deps_dir().join("github.com_acme_dep");
        let commit = init_fixture_repo(&checkout);
        config.offline = true;
        let pkg = make_locked_pkg(
            "github.com/acme/dep",
            "https://invalid.example/dep",
            &commit,
        );
        let dest = config.root.join("vendor").join("github.com_acme_dep");

        vendor_git_dep(&config, &pkg, &dest, &quiet_shell()).unwrap();
        assert_eq!(
            std::fs::read_to_string(dest.join("lib.cppm")).unwrap(),
            "export module fixture;"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_export_commit_keeps_executable_bits_and_links() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = TempDir::new().unwrap();
        let repo_dir = tmp.path().join("repo");
        std::fs::create_dir_all(&repo_dir).unwrap();
        let repo = git2::Repository::init(&repo_dir).unwrap();
        std::fs::write(repo_dir.join("run.sh"), "#!/bin/sh\n").unwrap();
        std::fs::set_permissions(
            repo_dir.join("run.sh"),
            std::fs::Permissions::from_mode(0o755),
        )
        .unwrap();
        std::os::unix::fs::symlink("run.sh", repo_dir.join("link.sh")).unwrap();
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("run.sh")).unwrap();
        index.add_path(Path::new("link.sh")).unwrap();
        index.write().unwrap();
        let tree = repo.find_tree(index.write_tree().unwrap()).unwrap();
        let sig = git2::Signature::now("t", "t@example.com").unwrap();
        let oid = repo
            .commit(Some("HEAD"), &sig, &sig, "init", &tree, &[])
            .unwrap();

        let dest = tmp.path().join("out");
        export_commit(&repo, oid, &dest, "x", &quiet_shell()).unwrap();
        let mode = std::fs::metadata(dest.join("run.sh"))
            .unwrap()
            .permissions()
            .mode();
        assert_eq!(mode & 0o111, 0o111);
        assert_eq!(
            std::fs::read_link(dest.join("link.sh")).unwrap(),
            Path::new("run.sh")
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
        // Relative, so the vendor directory works wherever it is checked out.
        assert!(content.contains("path = \"fmt\"\n"), "{}", content);
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
