use cmod_build::runner;
use cmod_core::config::Config;
use cmod_core::error::CmodError;

/// Run `cmod lint` — static analysis and style checks on C++ module sources.
pub fn run(verbose: bool) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    eprintln!("  Linting {}", config.manifest.package.name);

    let src_dir = config.src_dir();
    let sources = runner::discover_sources(&src_dir)?;

    if sources.is_empty() {
        eprintln!("  No source files found.");
        return Ok(());
    }

    let mut warnings = Vec::new();

    for source in &sources {
        let content = std::fs::read_to_string(source)?;
        let filename = source
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("unknown");

        let file_warnings = lint_source(filename, &content);
        for w in &file_warnings {
            if verbose {
                eprintln!("  warning: {}:{}: {}", filename, w.line, w.message);
            }
        }
        warnings.extend(file_warnings);
    }

    if warnings.is_empty() {
        eprintln!("  No warnings found ({} files checked).", sources.len());
    } else {
        eprintln!(
            "  {} warning(s) in {} file(s).",
            warnings.len(),
            sources.len()
        );
    }

    Ok(())
}

/// A lint warning with location info.
#[derive(Debug)]
struct LintWarning {
    line: usize,
    message: String,
}

/// Check a source file for common issues.
fn lint_source(filename: &str, content: &str) -> Vec<LintWarning> {
    let mut warnings = Vec::new();

    for (i, line) in content.lines().enumerate() {
        let lineno = i + 1;

        // Check for trailing whitespace
        if line != line.trim_end() && !line.trim().is_empty() {
            warnings.push(LintWarning {
                line: lineno,
                message: "trailing whitespace".to_string(),
            });
        }

        // Check for tabs (prefer spaces in module sources)
        if line.contains('\t') {
            warnings.push(LintWarning {
                line: lineno,
                message: "tab character found (prefer spaces)".to_string(),
            });
        }

        // Check for very long lines
        if line.len() > 120 {
            warnings.push(LintWarning {
                line: lineno,
                message: format!("line too long ({} chars, max 120)", line.len()),
            });
        }

        // Check for `#pragma once` in module files (not needed for modules)
        if line.trim() == "#pragma once" && is_module_file(filename) {
            warnings.push(LintWarning {
                line: lineno,
                message: "#pragma once is unnecessary in module files".to_string(),
            });
        }

        // Check for `using namespace std;` at global scope
        if line.trim() == "using namespace std;" {
            warnings.push(LintWarning {
                line: lineno,
                message: "avoid 'using namespace std;' at global scope".to_string(),
            });
        }

        // Check for C-style casts in module files
        // Simple heuristic: (Type) pattern not preceded by common keywords
        if is_module_file(filename) && has_c_style_cast(line) {
            warnings.push(LintWarning {
                line: lineno,
                message: "possible C-style cast; prefer static_cast/dynamic_cast".to_string(),
            });
        }
    }

    // Check for missing newline at end of file
    if !content.is_empty() && !content.ends_with('\n') {
        warnings.push(LintWarning {
            line: content.lines().count(),
            message: "file does not end with a newline".to_string(),
        });
    }

    warnings
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

    // Look for patterns like (int), (double), (char*) that aren't function calls
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lint_clean_source() {
        let content = "export module mymod;\n\nvoid hello() {}\n";
        let warnings = lint_source("test.cppm", content);
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_lint_trailing_whitespace() {
        let content = "int x = 1;  \n";
        let warnings = lint_source("test.cpp", content);
        assert!(warnings
            .iter()
            .any(|w| w.message.contains("trailing whitespace")));
    }

    #[test]
    fn test_lint_tab_character() {
        let content = "\tint x = 1;\n";
        let warnings = lint_source("test.cpp", content);
        assert!(warnings.iter().any(|w| w.message.contains("tab character")));
    }

    #[test]
    fn test_lint_long_line() {
        let content = format!("{}\n", "x".repeat(130));
        let warnings = lint_source("test.cpp", &content);
        assert!(warnings.iter().any(|w| w.message.contains("line too long")));
    }

    #[test]
    fn test_lint_pragma_once_in_module() {
        let content = "#pragma once\nexport module mymod;\n";
        let warnings = lint_source("test.cppm", content);
        assert!(warnings.iter().any(|w| w.message.contains("#pragma once")));
    }

    #[test]
    fn test_lint_pragma_once_in_header_ok() {
        let content = "#pragma once\nint x = 1;\n";
        let warnings = lint_source("test.hpp", content);
        assert!(!warnings.iter().any(|w| w.message.contains("#pragma once")));
    }

    #[test]
    fn test_lint_using_namespace_std() {
        let content = "using namespace std;\nint main() {}\n";
        let warnings = lint_source("test.cpp", content);
        assert!(warnings
            .iter()
            .any(|w| w.message.contains("using namespace std")));
    }

    #[test]
    fn test_lint_no_trailing_newline() {
        let content = "int main() {}";
        let warnings = lint_source("test.cpp", content);
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
}
