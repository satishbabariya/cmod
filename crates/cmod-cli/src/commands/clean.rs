use std::fs;
use std::path::Path;

use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::shell::{format_bytes, Shell};

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
pub fn run(shell: &Shell) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    shell.status("Cleaning", &config.manifest.package.name);

    let build_root = config.root.join("build");
    let mut total_freed: u64 = 0;

    // Remove build directory
    if build_root.exists() {
        let size = dir_size(&build_root);
        shell.verbose("Removing", build_root.display());
        fs::remove_dir_all(&build_root)?;
        total_freed += size;
    }

    // Remove incremental build state files
    let state_patterns = [".cmod-build-state.json"];
    for pattern in &state_patterns {
        let path = config.root.join(pattern);
        if path.exists() {
            shell.verbose("Removing", path.display());
            fs::remove_file(&path)?;
        }
    }

    shell.status("Cleaned", format!("freed {}", format_bytes(total_freed)));
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

#[cfg(test)]
mod tests {
    use super::*;

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
