use std::fs;
use std::path::Path;

use cmod_core::config::Config;
use cmod_core::error::CmodError;

/// Run `cmod clean` — remove build artifacts.
///
/// Removes:
/// - `build/` directory (compiled objects, PCMs, linked outputs)
/// - `.cmod-build-state.json` (incremental build state)
///
/// Does NOT remove:
/// - `vendor/` (vendored dependencies)
/// - `cmod.lock` (lockfile)
/// - Source files
pub fn run(verbose: bool) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    eprintln!("  Cleaning {}", config.manifest.package.name);

    let build_root = config.root.join("build");
    let mut total_freed: u64 = 0;

    // Remove build directory
    if build_root.exists() {
        let size = dir_size(&build_root);
        if verbose {
            eprintln!("  Removing {}", build_root.display());
        }
        fs::remove_dir_all(&build_root)?;
        total_freed += size;
    }

    // Remove incremental build state files
    let state_patterns = [".cmod-build-state.json"];
    for pattern in &state_patterns {
        let path = config.root.join(pattern);
        if path.exists() {
            if verbose {
                eprintln!("  Removing {}", path.display());
            }
            fs::remove_file(&path)?;
        }
    }

    eprintln!("  Cleaned, freed {}", format_bytes(total_freed));
    Ok(())
}

/// Compute the total size of a directory tree.
fn dir_size(path: &Path) -> u64 {
    walkdir::WalkDir::new(path)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
        .map(|e| e.metadata().map(|m| m.len()).unwrap_or(0))
        .sum()
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(100), "100 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1_048_576), "1.00 MB");
        assert_eq!(format_bytes(1_073_741_824), "1.00 GB");
    }

    #[test]
    fn test_dir_size_empty() {
        let tmp = tempfile::TempDir::new().unwrap();
        assert_eq!(dir_size(tmp.path()), 0);
    }

    #[test]
    fn test_dir_size_with_files() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("a.txt"), "hello").unwrap();
        std::fs::write(tmp.path().join("b.txt"), "world!").unwrap();
        assert_eq!(dir_size(tmp.path()), 11); // 5 + 6
    }
}
