use cmod_build::runner;
use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::shell::Shell;
use cmod_workspace::WorkspaceManager;

/// Run `cmod fmt` — format C++ module sources using clang-format.
pub fn run(check: bool, package: Option<String>, shell: &Shell) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    if config.manifest.is_workspace() {
        return fmt_workspace(&config, check, package, shell);
    }

    fmt_project(&config, check, shell)
}

/// Format a single (non-workspace) project.
fn fmt_project(config: &Config, check: bool, shell: &Shell) -> Result<(), CmodError> {
    let src_dirs = config.format_dirs();
    let exclude = config.format_exclude();
    let sources = runner::discover_sources_multi(&src_dirs, &exclude)?;

    if sources.is_empty() {
        shell.warn("no source files found to format");
        return Ok(());
    }

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

/// Format all workspace members (or a specific `--package`).
fn fmt_workspace(
    config: &Config,
    check: bool,
    package: Option<String>,
    shell: &Shell,
) -> Result<(), CmodError> {
    let ws = WorkspaceManager::load(&config.root)?;

    let members: Vec<_> = if let Some(ref name) = package {
        let m =
            ws.members.iter().find(|m| m.name == *name).ok_or_else(|| {
                CmodError::Other(format!("workspace member '{}' not found", name))
            })?;
        vec![m]
    } else {
        ws.members.iter().collect()
    };

    let mut total_checked = 0usize;
    let mut total_unformatted = 0usize;
    let mut any_error = false;

    for member in &members {
        let member_config = super::util::create_member_config(config, member)?;
        shell.status(if check { "Checking" } else { "Formatting" }, &member.name);

        match fmt_project(&member_config, check, shell) {
            Ok(()) => {}
            Err(CmodError::BuildFailed { ref reason }) if check => {
                // Parse out the count from the error message
                if let Some(count_str) = reason.split(' ').next() {
                    if let Ok(n) = count_str.parse::<usize>() {
                        total_unformatted += n;
                    }
                }
                any_error = true;
            }
            Err(e) => return Err(e),
        }

        let src_dirs = member_config.format_dirs();
        let exclude = member_config.format_exclude();
        let sources = runner::discover_sources_multi(&src_dirs, &exclude).unwrap_or_default();
        total_checked += sources.len();
    }

    if check && any_error {
        return Err(CmodError::BuildFailed {
            reason: format!(
                "{} file(s) need formatting across {} member(s)",
                total_unformatted,
                members.len()
            ),
        });
    }

    shell.status(
        "Finished",
        format!("{} files across {} member(s)", total_checked, members.len()),
    );

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
        let check = true;
        let label = if check { "Checking" } else { "Formatting" };
        assert_eq!(label, "Checking");
    }
}
