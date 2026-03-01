use cmod_core::config::Config;
use cmod_core::error::CmodError;

/// Run `cmod run` — build and execute the project binary.
pub fn run(
    release: bool,
    args: Vec<String>,
    verbose: bool,
) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    // Build first
    super::build::run(release, false, false, verbose, None, 0, false, None)?;

    // Find the built binary
    let build_dir = config.build_dir();
    let binary_name = &config.manifest.package.name;
    let binary_path = find_binary(&build_dir, binary_name)?;

    if verbose {
        eprintln!("  Running: {} {}", binary_path.display(), args.join(" "));
    }

    // Execute the binary, forwarding args
    let status = std::process::Command::new(&binary_path)
        .args(&args)
        .status()
        .map_err(|e| CmodError::BuildFailed {
            reason: format!("failed to execute {}: {}", binary_path.display(), e),
        })?;

    if !status.success() {
        let code = status.code().unwrap_or(1);
        std::process::exit(code);
    }

    Ok(())
}

/// Find the built binary in the build directory.
fn find_binary(
    build_dir: &std::path::Path,
    name: &str,
) -> Result<std::path::PathBuf, CmodError> {
    // Try common binary locations / names
    let candidates = [
        build_dir.join(name),
        build_dir.join(format!("{}.exe", name)),
        build_dir.join("a.out"),
    ];

    for candidate in &candidates {
        if candidate.exists() && candidate.is_file() {
            return Ok(candidate.clone());
        }
    }

    Err(CmodError::BuildFailed {
        reason: format!(
            "no binary found in {} (expected '{}'). Is the project configured as an executable?",
            build_dir.display(),
            name
        ),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_find_binary_exists() {
        let tmp = TempDir::new().unwrap();
        let binary = tmp.path().join("myapp");
        std::fs::write(&binary, "#!/bin/sh\n").unwrap();

        let result = find_binary(tmp.path(), "myapp");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), binary);
    }

    #[test]
    fn test_find_binary_exe_suffix() {
        let tmp = TempDir::new().unwrap();
        let binary = tmp.path().join("myapp.exe");
        std::fs::write(&binary, "MZ").unwrap();

        let result = find_binary(tmp.path(), "myapp");
        assert!(result.is_ok());
    }

    #[test]
    fn test_find_binary_not_found() {
        let tmp = TempDir::new().unwrap();
        let result = find_binary(tmp.path(), "nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_find_binary_a_out_fallback() {
        let tmp = TempDir::new().unwrap();
        let binary = tmp.path().join("a.out");
        std::fs::write(&binary, "ELF").unwrap();

        let result = find_binary(tmp.path(), "anyname");
        assert!(result.is_ok());
        assert!(result.unwrap().ends_with("a.out"));
    }
}
