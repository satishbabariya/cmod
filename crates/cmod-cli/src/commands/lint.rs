use cmod_build::runner;
use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::shell::Shell;
use cmod_workspace::WorkspaceManager;

/// Run `cmod lint` — static analysis and style checks on C++ module sources.
pub fn run(deny_warnings: bool, package: Option<String>, shell: &Shell) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    if config.manifest.is_workspace() {
        return lint_workspace(&config, deny_warnings, package, shell);
    }

    lint_project(&config, deny_warnings, shell)?;
    Ok(())
}

/// Lint a single (non-workspace) project. Returns the number of warnings found.
fn lint_project(config: &Config, deny_warnings: bool, shell: &Shell) -> Result<usize, CmodError> {
    shell.status("Linting", &config.manifest.package.name);

    let src_dirs = config.lint_dirs();
    let exclude = config.lint_exclude();
    let sources = runner::discover_sources_multi(&src_dirs, &exclude)?;

    if sources.is_empty() {
        shell.warn("no source files found");
        return Ok(0);
    }

    let max_line_length = config
        .manifest
        .lint
        .as_ref()
        .and_then(|l| l.max_line_length)
        .unwrap_or(120);

    let clang_tidy_enabled = config
        .manifest
        .lint
        .as_ref()
        .and_then(|l| l.clang_tidy)
        .unwrap_or(false);

    let mut warnings = Vec::new();

    for source in &sources {
        let content = std::fs::read_to_string(source)?;
        let filename = source
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("unknown");

        let file_warnings = lint_source(filename, &content, max_line_length);
        for w in &file_warnings {
            shell.warn(format!("{}:{}: {}", w.filename, w.line, w.message));
        }
        warnings.extend(file_warnings);

        // Run clang-tidy if enabled
        if clang_tidy_enabled {
            let tidy_warnings = run_clang_tidy(source, shell)?;
            warnings.extend(tidy_warnings);
        }
    }

    let warning_count = warnings.len();

    if warnings.is_empty() {
        shell.status(
            "Finished",
            format!("no warnings ({} files checked)", sources.len()),
        );
    } else {
        shell.status(
            "Finished",
            format!("{} warning(s) in {} file(s)", warning_count, sources.len()),
        );
    }

    if deny_warnings && warning_count > 0 {
        return Err(CmodError::BuildFailed {
            reason: format!(
                "lint failed: {} warning(s) found (--deny-warnings)",
                warning_count
            ),
        });
    }

    Ok(warning_count)
}

/// Lint all workspace members (or a specific `--package`).
fn lint_workspace(
    config: &Config,
    deny_warnings: bool,
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

    let mut total_warnings = 0usize;
    let mut total_files = 0usize;

    for member in &members {
        let member_config = super::util::create_member_config(config, member)?;

        let src_dirs = member_config.lint_dirs();
        let exclude = member_config.lint_exclude();
        let sources = runner::discover_sources_multi(&src_dirs, &exclude).unwrap_or_default();
        total_files += sources.len();

        let count = lint_project(&member_config, false, shell)?;
        total_warnings += count;
    }

    shell.status(
        "Finished",
        format!("{} file(s) across {} member(s)", total_files, members.len()),
    );

    if deny_warnings && total_warnings > 0 {
        return Err(CmodError::BuildFailed {
            reason: format!(
                "lint failed: {} warning(s) across workspace (--deny-warnings)",
                total_warnings
            ),
        });
    }

    Ok(())
}

/// A lint warning with location info.
#[derive(Debug)]
struct LintWarning {
    filename: String,
    line: usize,
    message: String,
}

/// Check a source file for common issues.
fn lint_source(filename: &str, content: &str, max_line_length: usize) -> Vec<LintWarning> {
    let mut warnings = Vec::new();

    for (i, line) in content.lines().enumerate() {
        let lineno = i + 1;

        // Check for trailing whitespace
        if line != line.trim_end() && !line.trim().is_empty() {
            warnings.push(LintWarning {
                filename: filename.to_string(),
                line: lineno,
                message: "trailing whitespace".to_string(),
            });
        }

        // Check for tabs (prefer spaces in module sources)
        if line.contains('\t') {
            warnings.push(LintWarning {
                filename: filename.to_string(),
                line: lineno,
                message: "tab character found (prefer spaces)".to_string(),
            });
        }

        // Check for very long lines
        if line.len() > max_line_length {
            warnings.push(LintWarning {
                filename: filename.to_string(),
                line: lineno,
                message: format!(
                    "line too long ({} chars, max {})",
                    line.len(),
                    max_line_length
                ),
            });
        }

        // Check for `#pragma once` in module files (not needed for modules)
        if line.trim() == "#pragma once" && is_module_file(filename) {
            warnings.push(LintWarning {
                filename: filename.to_string(),
                line: lineno,
                message: "#pragma once is unnecessary in module files".to_string(),
            });
        }

        // Check for `using namespace std;` at global scope
        if line.trim() == "using namespace std;" {
            warnings.push(LintWarning {
                filename: filename.to_string(),
                line: lineno,
                message: "avoid 'using namespace std;' at global scope".to_string(),
            });
        }

        // Check for C-style casts in module files
        if is_module_file(filename) && has_c_style_cast(line) {
            warnings.push(LintWarning {
                filename: filename.to_string(),
                line: lineno,
                message: "possible C-style cast; prefer static_cast/dynamic_cast".to_string(),
            });
        }
    }

    // Check for missing newline at end of file
    if !content.is_empty() && !content.ends_with('\n') {
        warnings.push(LintWarning {
            filename: filename.to_string(),
            line: content.lines().count(),
            message: "file does not end with a newline".to_string(),
        });
    }

    warnings
}

/// Run clang-tidy on a single source file and collect warnings.
fn run_clang_tidy(source: &std::path::Path, shell: &Shell) -> Result<Vec<LintWarning>, CmodError> {
    if !is_clang_tidy_available() {
        shell.warn("clang-tidy not found; skipping clang-tidy checks");
        return Ok(Vec::new());
    }

    let output = std::process::Command::new("clang-tidy")
        .arg(source)
        .arg("--")
        .output()
        .map_err(|e| CmodError::BuildFailed {
            reason: format!("failed to run clang-tidy: {}", e),
        })?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{}{}", stdout, stderr);

    let filename = source
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or("unknown");

    let mut warnings = Vec::new();

    // Parse clang-tidy output: "file:line:col: warning: message [check-name]"
    for line in combined.lines() {
        if let Some(warning_idx) = line.find(": warning:") {
            let location = &line[..warning_idx];
            let message = line[warning_idx + ": warning: ".len()..].trim();

            let lineno = location
                .rsplit(':')
                .nth(1)
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(0);

            shell.warn(format!("{}:{}: [clang-tidy] {}", filename, lineno, message));

            warnings.push(LintWarning {
                filename: filename.to_string(),
                line: lineno,
                message: format!("[clang-tidy] {}", message),
            });
        }
    }

    Ok(warnings)
}

/// Check if a filename represents a C++ module file.
fn is_module_file(filename: &str) -> bool {
    filename.ends_with(".cppm") || filename.ends_with(".ixx") || filename.ends_with(".mpp")
}

/// Simple heuristic for C-style casts: (int), (float), (char*), etc.
fn has_c_style_cast(line: &str) -> bool {
    let trimmed = line.trim();
    // Skip preprocessor lines, includes, and comments
    if trimmed.starts_with('#') || trimmed.starts_with("//") || trimmed.starts_with("/*") {
        return false;
    }

    let cast_types = [
        "int", "float", "double", "char", "long", "short", "unsigned", "void",
    ];
    for cast_type in &cast_types {
        let pattern = format!("({})", cast_type);
        if trimmed.contains(&pattern) {
            return true;
        }
        let ptr_pattern = format!("({}*)", cast_type);
        if trimmed.contains(&ptr_pattern) {
            return true;
        }
    }
    false
}

/// Check if `clang-tidy` is available on PATH.
fn is_clang_tidy_available() -> bool {
    std::process::Command::new("clang-tidy")
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
    fn test_lint_clean_source() {
        let content = "export module mymod;\n\nvoid hello() {}\n";
        let warnings = lint_source("test.cppm", content, 120);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_lint_trailing_whitespace() {
        let content = "int x = 1;  \n";
        let warnings = lint_source("test.cpp", content, 120);
        assert!(warnings
            .iter()
            .any(|w| w.message.contains("trailing whitespace")));
    }

    #[test]
    fn test_lint_tab_character() {
        let content = "\tint x = 1;\n";
        let warnings = lint_source("test.cpp", content, 120);
        assert!(warnings.iter().any(|w| w.message.contains("tab character")));
    }

    #[test]
    fn test_lint_long_line() {
        let content = format!("{}\n", "x".repeat(130));
        let warnings = lint_source("test.cpp", &content, 120);
        assert!(warnings.iter().any(|w| w.message.contains("line too long")));
    }

    #[test]
    fn test_lint_long_line_custom_max() {
        let content = format!("{}\n", "x".repeat(90));
        let warnings = lint_source("test.cpp", &content, 80);
        assert!(warnings
            .iter()
            .any(|w| w.message.contains("line too long") && w.message.contains("max 80")));
    }

    #[test]
    fn test_lint_long_line_custom_max_ok() {
        let content = format!("{}\n", "x".repeat(90));
        let warnings = lint_source("test.cpp", &content, 200);
        assert!(!warnings.iter().any(|w| w.message.contains("line too long")));
    }

    #[test]
    fn test_lint_pragma_once_in_module() {
        let content = "#pragma once\nexport module mymod;\n";
        let warnings = lint_source("test.cppm", content, 120);
        assert!(warnings.iter().any(|w| w.message.contains("#pragma once")));
    }

    #[test]
    fn test_lint_pragma_once_in_header_ok() {
        let content = "#pragma once\nint x = 1;\n";
        let warnings = lint_source("test.hpp", content, 120);
        assert!(!warnings.iter().any(|w| w.message.contains("#pragma once")));
    }

    #[test]
    fn test_lint_using_namespace_std() {
        let content = "using namespace std;\nint main() {}\n";
        let warnings = lint_source("test.cpp", content, 120);
        assert!(warnings
            .iter()
            .any(|w| w.message.contains("using namespace std")));
    }

    #[test]
    fn test_lint_no_trailing_newline() {
        let content = "int main() {}";
        let warnings = lint_source("test.cpp", content, 120);
        assert!(warnings.iter().any(|w| w.message.contains("newline")));
    }

    #[test]
    fn test_is_module_file() {
        assert!(is_module_file("lib.cppm"));
        assert!(is_module_file("mod.ixx"));
        assert!(is_module_file("mod.mpp"));
        assert!(!is_module_file("main.cpp"));
        assert!(!is_module_file("header.hpp"));
    }

    #[test]
    fn test_c_style_cast_detection() {
        assert!(has_c_style_cast("int y = (int)x;"));
        assert!(has_c_style_cast("char* p = (char*)buf;"));
        assert!(!has_c_style_cast("auto y = static_cast<int>(x);"));
        assert!(!has_c_style_cast("// (int) comment"));
        assert!(!has_c_style_cast("#define X (int)"));
    }

    #[test]
    fn test_lint_warning_has_filename() {
        let content = "int x = 1;  \n";
        let warnings = lint_source("myfile.cpp", content, 120);
        assert!(warnings.iter().all(|w| w.filename == "myfile.cpp"));
    }
}
