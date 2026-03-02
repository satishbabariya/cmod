use std::path::{Path, PathBuf};
use std::process::Command;

use cmod_core::error::CmodError;
use cmod_core::types::{Artifact, OptimizationLevel, Profile};

/// Abstraction over a C++ compiler backend.
///
/// The reference implementation targets Clang/LLVM. GCC and MSVC backends
/// are planned for future tiers.
pub trait CompilerBackend {
    /// Scan a source file for module dependencies.
    ///
    /// Returns a list of module names that the source imports.
    fn scan_deps(&self, source: &Path) -> Result<Vec<String>, CmodError>;

    /// Compile a module interface unit to produce a PCM (precompiled module)
    /// and an object file.
    fn compile_interface(
        &self,
        source: &Path,
        pcm_output: &Path,
        obj_output: &Path,
        dep_pcms: &[(&str, &Path)],
    ) -> Result<(), CmodError>;

    /// Compile a module implementation unit (or non-module TU) to an object file.
    fn compile_implementation(
        &self,
        source: &Path,
        obj_output: &Path,
        dep_pcms: &[(&str, &Path)],
    ) -> Result<(), CmodError>;

    /// Link object files into a final artifact.
    fn link(&self, objects: &[&Path], output: &Path, artifact: &Artifact) -> Result<(), CmodError>;
}

/// Clang/LLVM compiler backend.
pub struct ClangBackend {
    /// Path to the clang++ executable.
    pub clang_path: PathBuf,
    /// Path to clang-scan-deps executable.
    pub scan_deps_path: PathBuf,
    /// C++ standard (e.g., "20", "23").
    pub cxx_standard: String,
    /// Standard library (e.g., "libc++", "libstdc++").
    pub stdlib: Option<String>,
    /// Target triple.
    pub target: Option<String>,
    /// Build profile.
    pub profile: Profile,
    /// Additional flags.
    pub extra_flags: Vec<String>,
    /// Sysroot path for cross-compilation.
    pub sysroot: Option<PathBuf>,
    /// Enable LTO (link-time optimization).
    pub lto: bool,
    /// Explicit optimization level (overrides profile-based defaults).
    pub optimization: Option<OptimizationLevel>,
}

impl ClangBackend {
    /// Create a new Clang backend with default paths.
    pub fn new(cxx_standard: &str, profile: Profile) -> Self {
        ClangBackend {
            clang_path: std::env::var_os("CXX").map(PathBuf::from).unwrap_or_else(|| find_executable("clang++")),
            scan_deps_path: std::env::var_os("SCAN_DEPS").map(PathBuf::from).unwrap_or_else(|| find_executable("clang-scan-deps")),
            cxx_standard: cxx_standard.to_string(),
            stdlib: None,
            target: None,
            profile,
            extra_flags: Vec::new(),
            sysroot: None,
            lto: false,
            optimization: None,
        }
    }

    /// Common flags used for all compilations.
    pub fn common_flags(&self) -> Vec<String> {
        let mut flags = vec![format!("-std=c++{}", self.cxx_standard)];

        if let Some(ref stdlib) = self.stdlib {
            flags.push(format!("-stdlib={}", stdlib));
        }

        if let Some(ref target) = self.target {
            flags.push(format!("--target={}", target));
        }

        if let Some(ref sysroot) = self.sysroot {
            flags.push(format!("--sysroot={}", sysroot.display()));
        }

        // Use explicit optimization level if set, otherwise derive from profile
        match self.optimization {
            Some(OptimizationLevel::Debug) => {
                flags.push("-g".to_string());
                flags.push("-O0".to_string());
            }
            Some(OptimizationLevel::Release) => {
                flags.push("-O2".to_string());
                flags.push("-DNDEBUG".to_string());
            }
            Some(OptimizationLevel::Size) => {
                flags.push("-Os".to_string());
                flags.push("-DNDEBUG".to_string());
            }
            Some(OptimizationLevel::Speed) => {
                flags.push("-O3".to_string());
                flags.push("-DNDEBUG".to_string());
            }
            None => match self.profile {
                Profile::Debug => {
                    flags.push("-g".to_string());
                    flags.push("-O0".to_string());
                }
                Profile::Release => {
                    flags.push("-O2".to_string());
                    flags.push("-DNDEBUG".to_string());
                }
            },
        }

        if self.lto {
            flags.push("-flto=thin".to_string());
        }

        flags.extend(self.extra_flags.clone());
        flags
    }
}

impl CompilerBackend for ClangBackend {
    fn scan_deps(&self, source: &Path) -> Result<Vec<String>, CmodError> {
        let output = Command::new(&self.scan_deps_path)
            .arg("--format=p1689")
            .arg("--")
            .args(self.common_flags())
            .arg(source)
            .output()
            .map_err(|e| CmodError::ModuleScanFailed {
                reason: format!(
                    "failed to run clang-scan-deps at {}: {}",
                    self.scan_deps_path.display(),
                    e
                ),
            })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(CmodError::ModuleScanFailed {
                reason: format!("clang-scan-deps failed: {}", stderr),
            });
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        parse_p1689_imports(&stdout)
    }

    fn compile_interface(
        &self,
        source: &Path,
        pcm_output: &Path,
        obj_output: &Path,
        dep_pcms: &[(&str, &Path)],
    ) -> Result<(), CmodError> {
        let mut cmd = Command::new(&self.clang_path);
        cmd.args(self.common_flags());

        // Add dependency PCM references
        for (name, pcm_path) in dep_pcms {
            cmd.arg(format!("-fmodule-file={}={}", name, pcm_path.display()));
        }

        // First pass: compile to PCM
        let pcm_status = Command::new(&self.clang_path)
            .args(self.common_flags())
            .args(
                dep_pcms
                    .iter()
                    .map(|(name, path)| format!("-fmodule-file={}={}", name, path.display())),
            )
            .arg("--precompile")
            .arg("-o")
            .arg(pcm_output)
            .arg(source)
            .status()
            .map_err(|e| CmodError::BuildFailed {
                reason: format!("failed to run clang++: {}", e),
            })?;

        if !pcm_status.success() {
            return Err(CmodError::BuildFailed {
                reason: format!("failed to compile module interface: {}", source.display()),
            });
        }

        // Second pass: PCM to object file
        // Dependency PCMs are still needed for modules that import other modules
        let obj_status = Command::new(&self.clang_path)
            .args(self.common_flags())
            .args(
                dep_pcms
                    .iter()
                    .map(|(name, path)| format!("-fmodule-file={}={}", name, path.display())),
            )
            .arg("-c")
            .arg("-o")
            .arg(obj_output)
            .arg(pcm_output)
            .status()
            .map_err(|e| CmodError::BuildFailed {
                reason: format!("failed to run clang++: {}", e),
            })?;

        if !obj_status.success() {
            return Err(CmodError::BuildFailed {
                reason: format!("failed to compile PCM to object: {}", pcm_output.display()),
            });
        }

        Ok(())
    }

    fn compile_implementation(
        &self,
        source: &Path,
        obj_output: &Path,
        dep_pcms: &[(&str, &Path)],
    ) -> Result<(), CmodError> {
        let status = Command::new(&self.clang_path)
            .args(self.common_flags())
            .args(
                dep_pcms
                    .iter()
                    .map(|(name, path)| format!("-fmodule-file={}={}", name, path.display())),
            )
            .arg("-c")
            .arg("-o")
            .arg(obj_output)
            .arg(source)
            .status()
            .map_err(|e| CmodError::BuildFailed {
                reason: format!("failed to run clang++: {}", e),
            })?;

        if !status.success() {
            return Err(CmodError::BuildFailed {
                reason: format!("failed to compile: {}", source.display()),
            });
        }

        Ok(())
    }

    fn link(&self, objects: &[&Path], output: &Path, artifact: &Artifact) -> Result<(), CmodError> {
        let mut cmd = Command::new(&self.clang_path);
        cmd.args(self.common_flags());

        match artifact {
            Artifact::StaticLib { .. } => {
                // Use ar for static libs
                let status = Command::new("ar")
                    .arg("rcs")
                    .arg(output)
                    .args(objects)
                    .status()
                    .map_err(|e| CmodError::BuildFailed {
                        reason: format!("failed to run ar: {}", e),
                    })?;

                if !status.success() {
                    return Err(CmodError::BuildFailed {
                        reason: "ar failed to create static library".to_string(),
                    });
                }
                return Ok(());
            }
            Artifact::SharedLib { .. } => {
                cmd.arg("-shared");
            }
            _ => {}
        }

        cmd.arg("-o").arg(output);
        for obj in objects {
            cmd.arg(obj);
        }

        let status = cmd.status().map_err(|e| CmodError::BuildFailed {
            reason: format!("linker failed: {}", e),
        })?;

        if !status.success() {
            return Err(CmodError::BuildFailed {
                reason: "linking failed".to_string(),
            });
        }

        Ok(())
    }
}

/// Parse the P1689 JSON format from clang-scan-deps output to extract imports.
fn parse_p1689_imports(output: &str) -> Result<Vec<String>, CmodError> {
    // P1689 format: JSON with "rules" array, each rule has "requires" array
    let value: serde_json::Value =
        serde_json::from_str(output).map_err(|e| CmodError::ModuleScanFailed {
            reason: format!("failed to parse scan-deps output: {}", e),
        })?;

    let mut imports = Vec::new();

    if let Some(rules) = value.get("rules").and_then(|v| v.as_array()) {
        for rule in rules {
            if let Some(requires) = rule.get("requires").and_then(|v| v.as_array()) {
                for req in requires {
                    if let Some(name) = req.get("logical-name").and_then(|v| v.as_str()) {
                        imports.push(name.to_string());
                    }
                }
            }
        }
    }

    // Also try the "version" 1 format with top-level "requires"
    if imports.is_empty() {
        if let Some(requires) = value.get("requires").and_then(|v| v.as_array()) {
            for req in requires {
                if let Some(name) = req.get("logical-name").and_then(|v| v.as_str()) {
                    imports.push(name.to_string());
                }
            }
        }
    }

    Ok(imports)
}

/// Find an executable on PATH, falling back to the name itself.
fn find_executable(name: &str) -> PathBuf {
    which(name).unwrap_or_else(|| PathBuf::from(name))
}

/// Simple which implementation.
fn which(name: &str) -> Option<PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).find_map(|dir| {
            let full = dir.join(name);
            if full.is_file() {
                Some(full)
            } else {
                None
            }
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_p1689_imports() {
        let json = r#"{
            "rules": [
                {
                    "primary-output": "test.o",
                    "requires": [
                        { "logical-name": "std" },
                        { "logical-name": "github.fmtlib.fmt" }
                    ]
                }
            ]
        }"#;
        let imports = parse_p1689_imports(json).unwrap();
        assert_eq!(imports, vec!["std", "github.fmtlib.fmt"]);
    }

    #[test]
    fn test_parse_p1689_empty() {
        let json = r#"{ "rules": [] }"#;
        let imports = parse_p1689_imports(json).unwrap();
        assert!(imports.is_empty());
    }

    #[test]
    fn test_common_flags_debug() {
        let backend = ClangBackend::new("20", Profile::Debug);
        let flags = backend.common_flags();
        assert!(flags.contains(&"-std=c++20".to_string()));
        assert!(flags.contains(&"-g".to_string()));
        assert!(flags.contains(&"-O0".to_string()));
    }

    #[test]
    fn test_common_flags_release() {
        let backend = ClangBackend::new("23", Profile::Release);
        let flags = backend.common_flags();
        assert!(flags.contains(&"-std=c++23".to_string()));
        assert!(flags.contains(&"-O2".to_string()));
        assert!(flags.contains(&"-DNDEBUG".to_string()));
    }

    #[test]
    fn test_common_flags_with_target() {
        let mut backend = ClangBackend::new("20", Profile::Debug);
        backend.target = Some("x86_64-unknown-linux-gnu".to_string());
        let flags = backend.common_flags();
        assert!(flags.contains(&"--target=x86_64-unknown-linux-gnu".to_string()));
    }

    #[test]
    fn test_common_flags_with_stdlib() {
        let mut backend = ClangBackend::new("20", Profile::Debug);
        backend.stdlib = Some("libc++".to_string());
        let flags = backend.common_flags();
        assert!(flags.contains(&"-stdlib=libc++".to_string()));
    }

    #[test]
    fn test_common_flags_with_extra_flags() {
        let mut backend = ClangBackend::new("20", Profile::Debug);
        backend.extra_flags = vec!["-fsanitize=address".to_string(), "-Wall".to_string()];
        let flags = backend.common_flags();
        assert!(flags.contains(&"-fsanitize=address".to_string()));
        assert!(flags.contains(&"-Wall".to_string()));
    }

    #[test]
    fn test_parse_p1689_multiple_rules() {
        let json = r#"{
            "rules": [
                {
                    "primary-output": "a.o",
                    "requires": [
                        { "logical-name": "base" }
                    ]
                },
                {
                    "primary-output": "b.o",
                    "requires": [
                        { "logical-name": "base" },
                        { "logical-name": "utils" }
                    ]
                }
            ]
        }"#;
        let imports = parse_p1689_imports(json).unwrap();
        assert_eq!(imports, vec!["base", "base", "utils"]);
    }

    #[test]
    fn test_parse_p1689_no_requires() {
        let json = r#"{
            "rules": [
                {
                    "primary-output": "standalone.o",
                    "provides": [
                        { "logical-name": "mymod", "is-interface": true }
                    ]
                }
            ]
        }"#;
        let imports = parse_p1689_imports(json).unwrap();
        assert!(imports.is_empty());
    }

    #[test]
    fn test_parse_p1689_invalid_json() {
        let result = parse_p1689_imports("not json");
        assert!(result.is_err());
    }

    #[test]
    fn test_find_executable_fallback() {
        // A nonexistent executable should fall back to the name itself
        let path = find_executable("definitely_not_a_real_executable_12345");
        assert_eq!(
            path,
            PathBuf::from("definitely_not_a_real_executable_12345")
        );
    }

    #[test]
    fn test_clang_backend_defaults() {
        let backend = ClangBackend::new("20", Profile::Debug);
        assert_eq!(backend.cxx_standard, "20");
        assert!(backend.stdlib.is_none());
        assert!(backend.target.is_none());
        assert!(backend.extra_flags.is_empty());
        assert!(matches!(backend.profile, Profile::Debug));
        assert!(backend.optimization.is_none());
    }

    #[test]
    fn test_common_flags_optimization_size() {
        let mut backend = ClangBackend::new("20", Profile::Release);
        backend.optimization = Some(OptimizationLevel::Size);
        let flags = backend.common_flags();
        assert!(flags.contains(&"-Os".to_string()));
        assert!(flags.contains(&"-DNDEBUG".to_string()));
        assert!(!flags.contains(&"-O2".to_string()));
    }

    #[test]
    fn test_common_flags_optimization_speed() {
        let mut backend = ClangBackend::new("20", Profile::Release);
        backend.optimization = Some(OptimizationLevel::Speed);
        let flags = backend.common_flags();
        assert!(flags.contains(&"-O3".to_string()));
        assert!(flags.contains(&"-DNDEBUG".to_string()));
        assert!(!flags.contains(&"-O2".to_string()));
    }

    #[test]
    fn test_common_flags_optimization_overrides_profile() {
        let mut backend = ClangBackend::new("20", Profile::Debug);
        // Even though profile is Debug, optimization level overrides it
        backend.optimization = Some(OptimizationLevel::Speed);
        let flags = backend.common_flags();
        assert!(flags.contains(&"-O3".to_string()));
        assert!(!flags.contains(&"-g".to_string()));
        assert!(!flags.contains(&"-O0".to_string()));
    }
}
