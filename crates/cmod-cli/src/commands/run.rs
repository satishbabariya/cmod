use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_workspace::WorkspaceManager;

/// Run `cmod run` — build and execute the project binary.
pub fn run(
    release: bool,
    package: Option<String>,
    args: Vec<String>,
    verbose: bool,
    no_cache: bool,
) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    // Build first
    super::build::run(
        release,
        false,
        false,
        verbose,
        None,
        0,
        false,
        None,
        false,
        false,
        false,
        &[],
        false,
        no_cache,
    )?;

    // For workspace projects, resolve the target member binary
    if config.manifest.is_workspace() {
        return run_workspace_member(&config, release, package, args, verbose);
    }

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

/// Run a workspace member binary.
fn run_workspace_member(
    config: &Config,
    release: bool,
    package: Option<String>,
    args: Vec<String>,
    verbose: bool,
) -> Result<(), CmodError> {
    let ws = WorkspaceManager::load(&config.root)?;

    // Determine which member to run
    let member_name = match package {
        Some(name) => {
            // Validate the member exists
            if !ws.members.iter().any(|m| m.name == name) {
                return Err(CmodError::BuildFailed {
                    reason: format!(
                        "workspace member '{}' not found. Available members: {}",
                        name,
                        ws.members
                            .iter()
                            .map(|m| m.name.as_str())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                });
            }
            name
        }
        None => {
            // Find the first member with build type = binary
            let binary_members: Vec<_> = ws
                .members
                .iter()
                .filter(|m| {
                    m.manifest
                        .build
                        .as_ref()
                        .and_then(|b| b.build_type)
                        .map(|bt| bt == cmod_core::types::BuildType::Binary)
                        .unwrap_or(false)
                })
                .collect();

            match binary_members.len() {
                0 => {
                    return Err(CmodError::BuildFailed {
                        reason: "no workspace member configured as a binary. Use `--package <name>` to specify one.".to_string(),
                    });
                }
                1 => binary_members[0].name.clone(),
                _ => {
                    return Err(CmodError::BuildFailed {
                        reason: format!(
                            "multiple binary members found: {}. Use `--package <name>` to specify one.",
                            binary_members
                                .iter()
                                .map(|m| m.name.as_str())
                                .collect::<Vec<_>>()
                                .join(", ")
                        ),
                    });
                }
            }
        }
    };

    let profile_dir = if release { "release" } else { "debug" };
    let member_build_dir = config
        .root
        .join("build")
        .join(profile_dir)
        .join(&member_name);

    let binary_path = find_binary(&member_build_dir, &member_name)?;

    if verbose {
        eprintln!("  Running: {} {}", binary_path.display(), args.join(" "));
    }

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
///
/// Searches both the build root and profile subdirectories (debug/release)
/// for the named executable or any executable file.
fn find_binary(build_dir: &std::path::Path, name: &str) -> Result<std::path::PathBuf, CmodError> {
    // Search in both the build root and profile subdirectories
    let search_dirs = [
        build_dir.to_path_buf(),
        build_dir.join("debug"),
        build_dir.join("release"),
    ];

    for dir in &search_dirs {
        // Try exact name matches first, then common linker output names
        let candidates = [
            dir.join(name),
            dir.join(format!("{}.exe", name)),
            dir.join("main"),
            dir.join("main.exe"),
            dir.join("a.out"),
        ];

        for candidate in &candidates {
            if candidate.exists() && candidate.is_file() {
                return Ok(candidate.clone());
            }
        }
    }

    // Fallback: find any executable file in the build directories
    // (the linker output name may differ from the package name)
    for dir in &search_dirs {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    // Skip object files, PCMs, metadata, and test binaries
                    let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                    if matches!(ext, "o" | "pcm" | "json" | "a" | "so" | "dylib") {
                        continue;
                    }
                    let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
                    if stem.starts_with("test_") || stem.ends_with("_test") {
                        continue;
                    }
                    // Check if it's an executable (on Unix, check permissions)
                    #[cfg(unix)]
                    {
                        use std::os::unix::fs::PermissionsExt;
                        if let Ok(meta) = std::fs::metadata(&path) {
                            if meta.permissions().mode() & 0o111 != 0 {
                                return Ok(path);
                            }
                        }
                    }
                    #[cfg(not(unix))]
                    {
                        if ext == "exe" || ext.is_empty() {
                            return Ok(path);
                        }
                    }
                }
            }
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
