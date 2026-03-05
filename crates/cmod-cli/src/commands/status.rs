use cmod_build::runner;
use cmod_cache::ArtifactCache;
use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::lockfile::Lockfile;
use cmod_core::shell::{format_bytes, Shell, Verbosity};
use cmod_workspace::WorkspaceManager;

/// Run `cmod status` — show project state overview.
pub fn run(shell: &Shell) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;
    let verbose = shell.verbosity() == Verbosity::Verbose;

    println!(
        "Project: {} v{}",
        config.manifest.package.name, config.manifest.package.version
    );
    println!();

    // Module info
    if let Some(ref module) = config.manifest.module {
        println!("  Module: {}", module.name);
        println!("  Root:   {}", module.root.display());
    }

    // Profile
    println!("  Profile: {:?}", config.profile);

    // Source files
    let src_dir = config.src_dir();
    if src_dir.exists() {
        match runner::discover_sources(&src_dir) {
            Ok(sources) => {
                println!("  Sources: {} files", sources.len());
                if verbose {
                    for s in &sources {
                        let kind = runner::classify_source(s)
                            .map(|k| format!("{:?}", k))
                            .unwrap_or_else(|_| "unknown".to_string());
                        println!("    {} ({})", s.display(), kind);
                    }
                }
            }
            Err(_) => println!("  Sources: error scanning"),
        }
    } else {
        println!("  Sources: no src/ directory");
    }

    // Dependencies
    let dep_count = config.manifest.dependencies.len();
    let dev_dep_count = config.manifest.dev_dependencies.len();
    println!("  Dependencies: {} (+ {} dev)", dep_count, dev_dep_count);

    // Lockfile status
    if config.lockfile_path.exists() {
        match Lockfile::load(&config.lockfile_path) {
            Ok(lockfile) => {
                println!("  Lockfile: {} locked packages", lockfile.packages.len());
            }
            Err(_) => println!("  Lockfile: corrupt or unreadable"),
        }
    } else if dep_count > 0 {
        println!("  Lockfile: missing (run `cmod resolve`)");
    } else {
        println!("  Lockfile: not needed (no dependencies)");
    }

    // Cache status
    let cache = ArtifactCache::new(config.cache_dir());
    match cache.status() {
        Ok(status) => {
            println!(
                "  Cache: {} entries, {}",
                status.entry_count,
                format_bytes(status.total_size)
            );
        }
        Err(_) => println!("  Cache: not initialized"),
    }

    // Workspace info
    if config.manifest.is_workspace() {
        match WorkspaceManager::load(&config.root) {
            Ok(ws) => {
                println!("  Workspace: {} members", ws.members.len());
                if verbose {
                    for m in &ws.members {
                        println!("    - {}", m.name);
                    }
                }
            }
            Err(_) => println!("  Workspace: error loading"),
        }
    }

    // Build directory status
    let build_dir = config.build_dir();
    if build_dir.exists() {
        let size = dir_size(&build_dir);
        println!("  Build dir: {}", format_bytes(size));
    } else {
        println!("  Build dir: not yet created");
    }

    println!();
    Ok(())
}

/// Calculate total size of a directory tree.
fn dir_size(path: &std::path::Path) -> u64 {
    walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .map(|m| m.len())
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dir_size_nonexistent() {
        let size = dir_size(std::path::Path::new("/nonexistent_dir_xyz"));
        assert_eq!(size, 0);
    }

    #[test]
    fn test_dir_size_real() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("a.txt"), "hello").unwrap();
        std::fs::write(tmp.path().join("b.txt"), "world!").unwrap();
        let size = dir_size(tmp.path());
        assert_eq!(size, 11);
    }
}
