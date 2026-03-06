use cmod_build::runner;
use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::shell::Shell;

/// Run `cmod fmt` — format C++ module sources using clang-format.
pub fn run(check: bool, shell: &Shell) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    let src_dir = config.src_dir();
    let sources = runner::discover_sources(&src_dir)?;

    if sources.is_empty() {
        shell.warn("no source files found to format");
        return Ok(());
    }

    // Check if clang-format is available
    if !is_clang_format_available() {
        return Err(CmodError::CompilerNotFound {
            compiler: "clang-format (install LLVM toolchain)".to_string(),
        });
    }

    shell.status(
        if check { "Checking" } else { "Formatting" },
        format!("{} source files", sources.len()),
    );

    let mut unformatted = Vec::new();

    for source in &sources {
        let filename = source
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("unknown");

        if check {
            // --dry-run mode: check if file needs formatting
            let output = std::process::Command::new("clang-format")
                .arg("--dry-run")
                .arg("--Werror")
                .arg(source)
                .output()
                .map_err(|e| CmodError::BuildFailed {
                    reason: format!("failed to run clang-format: {}", e),
                })?;

            if !output.status.success() {
                unformatted.push(filename.to_string());
                shell.verbose("Unformatted", filename);
            }
        } else {
            // In-place formatting
            let status = std::process::Command::new("clang-format")
                .arg("-i")
                .arg(source)
                .status()
                .map_err(|e| CmodError::BuildFailed {
                    reason: format!("failed to run clang-format: {}", e),
                })?;

            if !status.success() {
                return Err(CmodError::BuildFailed {
                    reason: format!("clang-format failed for {}", filename),
                });
            }

            shell.verbose("Formatted", filename);
        }
    }

    if check {
        if unformatted.is_empty() {
            shell.status("Finished", "all files are properly formatted");
        } else {
            return Err(CmodError::BuildFailed {
                reason: format!(
                    "{} file(s) need formatting: {}",
                    unformatted.len(),
                    unformatted.join(", ")
                ),
            });
        }
    } else {
        shell.status("Formatted", format!("{} files", sources.len()));
    }

    Ok(())
}

/// Check if `clang-format` is available on PATH.
fn is_clang_format_available() -> bool {
    std::process::Command::new("clang-format")
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_clang_format_check_concept() {
        // Verifies the module compiles and basic types work.
        // Actual clang-format invocation tested in integration tests.
        let check = true;
        let label = if check { "Checking" } else { "Formatting" };
        assert_eq!(label, "Checking");
    }
}
