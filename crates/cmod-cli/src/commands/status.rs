use cmod_build::runner;
use cmod_cache::ArtifactCache;
use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::lockfile::Lockfile;
use cmod_workspace::WorkspaceManager;

/// Run `cmod status` — show project state overview.
pub fn run(verbose: bool) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    println!("Project: {} v{}", config.manifest.package.name, config.manifest.package.version);
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
            println!("  Cache: {} entries, {}", status.entry_count, format_size(status.total_size));
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
        println!("  Build dir: {}", format_size(size));
    } else {
        println!("  Build dir: not yet created");
    }

    println!();
    Ok(())
}

/// Format a byte count into human-readable form.
fn format_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{} B", bytes)
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
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
    fn test_format_size_bytes() {
        assert_eq!(format_size(0), "0 B");
        assert_eq!(format_size(512), "512 B");
    }

    #[test]
    fn test_format_size_kb() {
        assert_eq!(format_size(1024), "1.0 KB");
        assert_eq!(format_size(2048), "2.0 KB");
    }

    #[test]
    fn test_format_size_mb() {
        assert_eq!(format_size(1024 * 1024), "1.0 MB");
    }

    #[test]
    fn test_format_size_gb() {
        assert_eq!(format_size(1024 * 1024 * 1024), "1.0 GB");
    }

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
        assert_eq!(size, 11); // "hello" (5) + "world!" (6)
    }
}
