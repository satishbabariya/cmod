use std::path::{Path, PathBuf};

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

    let report = fmt_project(&config, check, shell)?;
    if check {
        if !report.unformatted.is_empty() {
            return Err(unformatted_error(&report.unformatted, &config.root, None));
        }
        if report.files > 0 {
            shell.status("Finished", "all files are properly formatted");
        }
    } else if report.files > 0 {
        shell.status("Formatted", format!("{} files", report.files));
    }
    Ok(())
}

/// What formatting (or checking) one package found.
struct FmtReport {
    /// Sources formatted or checked.
    files: usize,
    /// With `--check`, the sources clang-format would change.
    unformatted: Vec<PathBuf>,
}

/// Format, or with `check` only check, the sources of one package.
fn fmt_project(config: &Config, check: bool, shell: &Shell) -> Result<FmtReport, CmodError> {
    let src_dirs = config.format_dirs();
    let exclude = config.format_exclude();
    let sources = runner::discover_sources_multi(&src_dirs, &exclude)?;

    if sources.is_empty() {
        shell.warn("no source files found to format");
        return Ok(FmtReport {
            files: 0,
            unformatted: Vec::new(),
        });
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
        let shown = relative_to(source, &config.root);

        if check {
            let output = std::process::Command::new("clang-format")
                .arg("--dry-run")
                .arg("--Werror")
                .arg(source)
                .output()
                .map_err(|e| CmodError::Other(format!("failed to run clang-format: {}", e)))?;

            if !output.status.success() {
                unformatted.push(source.clone());
                shell.verbose("Unformatted", &shown);
            }
        } else {
            let status = std::process::Command::new("clang-format")
                .arg("-i")
                .arg(source)
                .status()
                .map_err(|e| CmodError::Other(format!("failed to run clang-format: {}", e)))?;

            if !status.success() {
                return Err(CmodError::Other(format!(
                    "clang-format failed for {}",
                    shown
                )));
            }

            shell.verbose("Formatted", &shown);
        }
    }

    Ok(FmtReport {
        files: sources.len(),
        unformatted,
    })
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

    let mut total_files = 0usize;
    let mut unformatted = Vec::new();

    for member in &members {
        let member_config = super::util::create_member_config(config, member)?;
        shell.status(if check { "Checking" } else { "Formatting" }, &member.name);

        let report = fmt_project(&member_config, check, shell)?;
        total_files += report.files;
        unformatted.extend(report.unformatted);
    }

    if !unformatted.is_empty() {
        return Err(unformatted_error(
            &unformatted,
            &config.root,
            Some(members.len()),
        ));
    }

    shell.status(
        "Finished",
        format!("{} files across {} member(s)", total_files, members.len()),
    );

    Ok(())
}

/// The `--check` failure naming the files to format, relative to `root`.
fn unformatted_error(files: &[PathBuf], root: &Path, members: Option<usize>) -> CmodError {
    let names: Vec<String> = files.iter().map(|f| relative_to(f, root)).collect();
    let scope = match members {
        Some(n) => format!(" across {} member(s)", n),
        None => String::new(),
    };
    CmodError::CheckFailed {
        reason: format!(
            "{} file(s) need formatting{}: {}",
            files.len(),
            scope,
            names.join(", ")
        ),
    }
}

/// `path` relative to `root` when it is inside it, with `/` separators.
fn relative_to(path: &Path, root: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
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
    use super::*;

    #[test]
    fn test_unformatted_error_names_paths_relative_to_the_root() {
        let root = Path::new("/ws");
        let files = vec![
            PathBuf::from("/ws/a/src/main.cpp"),
            PathBuf::from("/ws/b/src/main.cpp"),
        ];
        let err = unformatted_error(&files, root, Some(2));
        assert_eq!(
            err.to_string(),
            "2 file(s) need formatting across 2 member(s): a/src/main.cpp, b/src/main.cpp"
        );
        let err = unformatted_error(&files[..1], Path::new("/ws/a"), None);
        assert_eq!(err.to_string(), "1 file(s) need formatting: src/main.cpp");
        assert_eq!(err.exit_code(), cmod_core::error::EXIT_BUILD_FAILURE);
    }
}
