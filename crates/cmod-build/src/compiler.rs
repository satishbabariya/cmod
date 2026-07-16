use std::path::{Path, PathBuf};
use std::process::Command;

use cmod_core::error::CmodError;
use cmod_core::types::{Artifact, OptimizationLevel, Profile};

/// Abstraction over a C++ compiler backend.
///
/// The reference implementation targets Clang/LLVM. GCC and MSVC backends
/// are planned for future tiers.
pub trait CompilerBackend: Send + Sync {
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

    /// Which compiler family this backend drives.
    fn kind(&self) -> cmod_core::types::Compiler;

    /// Path to the compiler executable (for compile_commands.json).
    fn compiler_path(&self) -> &Path;

    /// Detected compiler version string (e.g. `"18.1.8"`). Seeds cache keys.
    fn version(&self) -> String;

    /// The configured C++ standard (e.g. `"20"`).
    fn cxx_standard(&self) -> &str;

    /// The configured target triple, if any.
    fn target(&self) -> Option<&str>;

    /// Flags common to all compilations (compile_commands.json, diagnostics).
    fn common_flags(&self) -> Vec<String>;

    /// Deterministic description of every configuration input that affects
    /// codegen. Hashed into incremental-build state and cache keys.
    fn fingerprint(&self) -> String;

    /// File extension of this compiler's BMI artifacts (without the dot).
    /// Clang emits `.pcm`; MSVC emits `.ifc`; GCC emits CMI files under a
    /// module-mapper directory. `BuildPlan` currently hardcodes `pcm` paths —
    /// wiring this in is part of full non-Clang backend support (#48).
    fn bmi_extension(&self) -> &'static str {
        "pcm"
    }
}

/// LTO mode for link-time optimization.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum LtoMode {
    /// ThinLTO — faster link times, good optimization.
    #[default]
    Thin,
    /// Full LTO — slower but maximum optimization.
    Full,
}

/// Compiler-agnostic configuration for constructing a backend.
///
/// Carries every knob that affects codegen so backends can be built through
/// [`make_backend`] without poking concrete fields afterwards.
#[derive(Debug, Clone, Default)]
pub struct BackendConfig {
    /// C++ standard (e.g., "20", "23").
    pub cxx_standard: String,
    /// Build profile.
    pub profile: Profile,
    /// Standard library (e.g., "libc++", "libstdc++").
    pub stdlib: Option<String>,
    /// Sysroot path for cross-compilation.
    pub sysroot: Option<PathBuf>,
    /// Target triple.
    pub target: Option<String>,
    /// Additional flags (includes, defines, user extra_flags).
    pub extra_flags: Vec<String>,
    /// Enable LTO.
    pub lto: bool,
    /// LTO mode.
    pub lto_mode: LtoMode,
    /// Explicit optimization level (overrides profile-based defaults).
    pub optimization: Option<OptimizationLevel>,
}

/// Construct a compiler backend for the requested compiler family.
///
/// Clang is fully supported. GCC and MSVC return a clear error until their
/// backends land (#47 groundwork / #48 skeleton).
pub fn make_backend(
    kind: cmod_core::types::Compiler,
    config: &BackendConfig,
) -> Result<Box<dyn CompilerBackend>, CmodError> {
    match kind {
        cmod_core::types::Compiler::Clang => Ok(Box::new(ClangBackend::from_config(config))),
        other => Err(CmodError::BuildFailed {
            reason: format!(
                "the {} backend is not yet implemented; set [toolchain] compiler = \"clang\" \
                 (or remove the line) — see issues #47/#48",
                other
            ),
        }),
    }
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
    /// LTO mode: "thin" (default) or "full".
    pub lto_mode: LtoMode,
    /// Explicit optimization level (overrides profile-based defaults).
    pub optimization: Option<OptimizationLevel>,
}

impl ClangBackend {
    /// Create a new Clang backend with default paths.
    pub fn new(cxx_standard: &str, profile: Profile) -> Self {
        ClangBackend {
            clang_path: std::env::var_os("CXX")
                .map(PathBuf::from)
                .unwrap_or_else(|| find_executable("clang++")),
            scan_deps_path: std::env::var_os("SCAN_DEPS")
                .map(PathBuf::from)
                .unwrap_or_else(|| find_executable("clang-scan-deps")),
            cxx_standard: cxx_standard.to_string(),
            stdlib: None,
            target: None,
            profile,
            extra_flags: Vec::new(),
            sysroot: None,
            lto: false,
            lto_mode: LtoMode::default(),
            optimization: None,
        }
    }

    /// Create a Clang backend from a full [`BackendConfig`].
    pub fn from_config(config: &BackendConfig) -> Self {
        let mut backend = ClangBackend::new(&config.cxx_standard, config.profile);
        backend.stdlib = config.stdlib.clone();
        backend.sysroot = config.sysroot.clone();
        backend.target = config.target.clone();
        backend.extra_flags = config.extra_flags.clone();
        backend.lto = config.lto;
        backend.lto_mode = config.lto_mode;
        backend.optimization = config.optimization;
        backend
    }

    /// Query the compiler for its version string (e.g. `"18.1.8"`).
    ///
    /// Used to seed the cache key so that PCMs produced by different Clang
    /// majors don't collide in the on-disk cache (PCM format is not
    /// compatible across versions).
    pub fn detect_version(&self) -> String {
        let out = std::process::Command::new(&self.clang_path)
            .arg("--version")
            .output();
        let stdout = match out {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
            _ => return String::new(),
        };
        // First line is like: "Homebrew clang version 18.1.8" or
        // "Apple clang version 21.0.0 (clang-2100.0.123.102)".
        for token in stdout.split_whitespace() {
            if token.chars().next().is_some_and(|c| c.is_ascii_digit()) && token.contains('.') {
                return token.to_string();
            }
        }
        String::new()
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
            match self.lto_mode {
                LtoMode::Thin => flags.push("-flto=thin".to_string()),
                LtoMode::Full => flags.push("-flto=full".to_string()),
            }
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
        // For .cc/.cpp/.cxx files, clang doesn't auto-detect module interface —
        // we must explicitly specify the language with -x c++-module.
        let needs_lang_override =
            !matches!(source.extension().and_then(|e| e.to_str()), Some("cppm"));

        let mut pcm_cmd = Command::new(&self.clang_path);
        pcm_cmd.args(self.common_flags()).args(
            dep_pcms
                .iter()
                .map(|(name, path)| format!("-fmodule-file={}={}", name, path.display())),
        );
        if needs_lang_override {
            pcm_cmd.args(["-x", "c++-module"]);
        }
        let pcm_status = pcm_cmd
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
        let is_c_file = source
            .extension()
            .and_then(|e| e.to_str())
            .map(|e| e == "c")
            .unwrap_or(false);

        let mut cmd = Command::new(&self.clang_path);

        if is_c_file {
            // For plain C files, use C-compatible flags instead of C++ flags.
            // Filter out -std=c++XX and module-related flags; add -std=c17.
            let flags = self.common_flags();
            for flag in &flags {
                if flag.starts_with("-std=c++") || flag.starts_with("-fmodule") {
                    continue;
                }
                cmd.arg(flag);
            }
            cmd.arg("-std=c17");
            // Force C language for unambiguous compilation
            cmd.arg("-x").arg("c");
        } else {
            cmd.args(self.common_flags());
            cmd.args(
                dep_pcms
                    .iter()
                    .map(|(name, path)| format!("-fmodule-file={}={}", name, path.display())),
            );
        }

        let status = cmd
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

    fn kind(&self) -> cmod_core::types::Compiler {
        cmod_core::types::Compiler::Clang
    }

    fn compiler_path(&self) -> &Path {
        &self.clang_path
    }

    fn version(&self) -> String {
        self.detect_version()
    }

    fn cxx_standard(&self) -> &str {
        &self.cxx_standard
    }

    fn target(&self) -> Option<&str> {
        self.target.as_deref()
    }

    fn common_flags(&self) -> Vec<String> {
        ClangBackend::common_flags(self)
    }

    fn fingerprint(&self) -> String {
        format!(
            "clang|std={}|stdlib={}|target={}|sysroot={}|profile={:?}|lto={}:{:?}|opt={:?}|flags={}",
            self.cxx_standard,
            self.stdlib.as_deref().unwrap_or(""),
            self.target.as_deref().unwrap_or(""),
            self.sysroot
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            self.profile,
            self.lto,
            self.lto_mode,
            self.optimization,
            self.extra_flags.join(" "),
        )
    }

    fn link(&self, objects: &[&Path], output: &Path, artifact: &Artifact) -> Result<(), CmodError> {
        let mut cmd = Command::new(&self.clang_path);
        cmd.args(self.common_flags());

        match artifact {
            Artifact::StaticLib { .. } => {
                // Use ar for static libs — filter out .a archives to prevent nesting
                let obj_only: Vec<&&Path> = objects
                    .iter()
                    .filter(|p| p.extension().and_then(|e| e.to_str()) != Some("a"))
                    .collect();
                // Skip ar for header-only packages with no object files
                if obj_only.is_empty() {
                    return Ok(());
                }
                // Remove existing archive to avoid stale objects from prior builds
                let _ = std::fs::remove_file(output);
                let status = Command::new("ar")
                    .arg("rcs")
                    .arg(output)
                    .args(obj_only)
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
/// MSVC compiler backend — **skeleton only** (#48).
///
/// Validates the `CompilerBackend` trait shape against MSVC's model. Flag
/// mapping is real (`/std:c++NN`, `/EHsc`, `/interface`, `/ifcOutput`,
/// `/reference name=path.ifc`); the compile/link/scan entry points return a
/// clear skeleton error until the full implementation lands.
///
/// Shape findings recorded for the full implementation:
/// - BMIs are `.ifc`, not `.pcm` — see [`CompilerBackend::bmi_extension`];
///   `BuildPlan` path generation must consult it.
/// - Dependency scanning has no `clang-scan-deps` equivalent; MSVC uses
///   `cl /scanDependencies` emitting the same P1689 JSON format.
/// - Interface units conventionally use `.ixx`, already accepted by source
///   discovery.
pub struct MsvcBackend {
    /// Path to cl.exe.
    pub cl_path: PathBuf,
    config: BackendConfig,
}

impl MsvcBackend {
    /// Create an MSVC backend from a full [`BackendConfig`].
    pub fn from_config(config: &BackendConfig) -> Self {
        MsvcBackend {
            cl_path: std::env::var_os("CXX")
                .map(PathBuf::from)
                .unwrap_or_else(|| find_executable("cl")),
            config: config.clone(),
        }
    }

    fn not_implemented(&self, what: &str) -> CmodError {
        CmodError::BuildFailed {
            reason: format!(
                "MSVC backend is a skeleton; {} is not yet implemented (see issue #48)",
                what
            ),
        }
    }
}

impl CompilerBackend for MsvcBackend {
    fn scan_deps(&self, _source: &Path) -> Result<Vec<String>, CmodError> {
        // Full implementation: `cl /scanDependencies` (P1689 JSON output).
        Err(self.not_implemented("dependency scanning"))
    }

    fn compile_interface(
        &self,
        _source: &Path,
        _pcm_output: &Path,
        _obj_output: &Path,
        _dep_pcms: &[(&str, &Path)],
    ) -> Result<(), CmodError> {
        // Full implementation: `cl /c /interface /ifcOutput <out.ifc>` with
        // `/reference <name>=<path.ifc>` per dependency.
        Err(self.not_implemented("interface compilation"))
    }

    fn compile_implementation(
        &self,
        _source: &Path,
        _obj_output: &Path,
        _dep_pcms: &[(&str, &Path)],
    ) -> Result<(), CmodError> {
        Err(self.not_implemented("implementation compilation"))
    }

    fn link(
        &self,
        _objects: &[&Path],
        _output: &Path,
        _artifact: &Artifact,
    ) -> Result<(), CmodError> {
        // Full implementation: `link.exe` / `lib.exe` per artifact kind.
        Err(self.not_implemented("linking"))
    }

    fn kind(&self) -> cmod_core::types::Compiler {
        cmod_core::types::Compiler::Msvc
    }

    fn compiler_path(&self) -> &Path {
        &self.cl_path
    }

    fn version(&self) -> String {
        // cl.exe prints its version banner to stderr with no arguments.
        let out = Command::new(&self.cl_path).output();
        let banner = match out {
            Ok(o) => String::from_utf8_lossy(&o.stderr).into_owned(),
            _ => return String::new(),
        };
        banner
            .split_whitespace()
            .find(|t| t.chars().next().is_some_and(|c| c.is_ascii_digit()) && t.contains('.'))
            .unwrap_or_default()
            .to_string()
    }

    fn cxx_standard(&self) -> &str {
        &self.config.cxx_standard
    }

    fn target(&self) -> Option<&str> {
        self.config.target.as_deref()
    }

    fn common_flags(&self) -> Vec<String> {
        let mut flags = vec![
            format!("/std:c++{}", self.config.cxx_standard),
            "/EHsc".to_string(),
            "/nologo".to_string(),
        ];
        match self.config.profile {
            Profile::Debug => flags.push("/Od".to_string()),
            Profile::Release => flags.push("/O2".to_string()),
        }
        flags.extend(self.config.extra_flags.clone());
        flags
    }

    fn fingerprint(&self) -> String {
        format!(
            "msvc|std={}|target={}|profile={:?}|opt={:?}|flags={}",
            self.config.cxx_standard,
            self.config.target.as_deref().unwrap_or(""),
            self.config.profile,
            self.config.optimization,
            self.config.extra_flags.join(" "),
        )
    }

    fn bmi_extension(&self) -> &'static str {
        "ifc"
    }
}

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

    // --- MSVC skeleton: trait-shape validation (#48) ---

    #[test]
    fn test_msvc_backend_kind_and_flags() {
        let cfg = BackendConfig {
            cxx_standard: "20".to_string(),
            extra_flags: vec!["/DFOO=1".to_string()],
            ..Default::default()
        };
        let backend = MsvcBackend::from_config(&cfg);
        assert_eq!(backend.kind(), cmod_core::types::Compiler::Msvc);
        let flags = CompilerBackend::common_flags(&backend);
        assert!(flags.contains(&"/std:c++20".to_string()));
        assert!(flags.contains(&"/EHsc".to_string()));
        assert!(flags.contains(&"/DFOO=1".to_string()));
    }

    #[test]
    fn test_msvc_backend_bmi_extension() {
        let cfg = BackendConfig::default();
        // MSVC produces .ifc BMIs; clang produces .pcm
        assert_eq!(MsvcBackend::from_config(&cfg).bmi_extension(), "ifc");
        assert_eq!(
            ClangBackend::new("20", Profile::Debug).bmi_extension(),
            "pcm"
        );
    }

    #[test]
    fn test_msvc_backend_compile_is_not_implemented() {
        let cfg = BackendConfig::default();
        let backend = MsvcBackend::from_config(&cfg);
        let err = backend
            .compile_interface(
                Path::new("a.ixx"),
                Path::new("a.ifc"),
                Path::new("a.obj"),
                &[],
            )
            .unwrap_err();
        assert!(
            err.to_string().contains("skeleton"),
            "compile must fail with a skeleton notice, got: {}",
            err
        );
        assert!(backend.scan_deps(Path::new("a.ixx")).is_err());
        assert!(backend
            .link(
                &[],
                Path::new("out.exe"),
                &Artifact::Executable {
                    path: PathBuf::from("out.exe")
                }
            )
            .is_err());
    }

    #[test]
    fn test_msvc_fingerprint_deterministic_and_distinct_from_clang() {
        let cfg = BackendConfig {
            cxx_standard: "20".to_string(),
            ..Default::default()
        };
        let msvc = MsvcBackend::from_config(&cfg);
        let msvc2 = MsvcBackend::from_config(&cfg);
        assert_eq!(msvc.fingerprint(), msvc2.fingerprint());
        let clang = ClangBackend::from_config(&cfg);
        assert_ne!(
            msvc.fingerprint(),
            CompilerBackend::fingerprint(&clang),
            "fingerprints must differ across compiler families"
        );
    }

    // --- backend factory and trait-object groundwork (#47) ---

    #[test]
    fn test_make_backend_clang() {
        let cfg = BackendConfig {
            cxx_standard: "20".to_string(),
            ..Default::default()
        };
        let backend = make_backend(cmod_core::types::Compiler::Clang, &cfg).unwrap();
        assert_eq!(backend.kind(), cmod_core::types::Compiler::Clang);
        assert_eq!(backend.cxx_standard(), "20");
    }

    #[test]
    fn test_make_backend_gcc_not_implemented() {
        let cfg = BackendConfig::default();
        let Err(err) = make_backend(cmod_core::types::Compiler::Gcc, &cfg) else {
            panic!("gcc backend must not construct yet");
        };
        assert!(
            err.to_string().contains("not yet implemented"),
            "gcc should error clearly, got: {}",
            err
        );
    }

    #[test]
    fn test_make_backend_msvc_not_implemented() {
        let cfg = BackendConfig::default();
        let Err(err) = make_backend(cmod_core::types::Compiler::Msvc, &cfg) else {
            panic!("msvc backend must not construct yet");
        };
        assert!(err.to_string().contains("not yet implemented"));
    }

    #[test]
    fn test_backend_config_carries_all_knobs() {
        let cfg = BackendConfig {
            cxx_standard: "23".to_string(),
            profile: Profile::Release,
            stdlib: Some("libc++".to_string()),
            sysroot: Some(PathBuf::from("/sdk")),
            target: Some("x86_64-unknown-linux-gnu".to_string()),
            extra_flags: vec!["-DFOO=1".to_string()],
            lto: true,
            lto_mode: LtoMode::Thin,
            optimization: None,
        };
        let backend = make_backend(cmod_core::types::Compiler::Clang, &cfg).unwrap();
        let flags = backend.common_flags();
        assert!(flags.contains(&"-std=c++23".to_string()));
        assert!(flags.contains(&"-stdlib=libc++".to_string()));
        assert!(flags.contains(&"-DFOO=1".to_string()));
        assert!(flags.iter().any(|f| f.contains("x86_64-unknown-linux")));
    }

    #[test]
    fn test_fingerprint_changes_with_config() {
        let base = BackendConfig {
            cxx_standard: "20".to_string(),
            ..Default::default()
        };
        let a = make_backend(cmod_core::types::Compiler::Clang, &base).unwrap();
        let b = make_backend(
            cmod_core::types::Compiler::Clang,
            &BackendConfig {
                cxx_standard: "23".to_string(),
                ..base.clone()
            },
        )
        .unwrap();
        assert_ne!(a.fingerprint(), b.fingerprint());
        // Deterministic for identical config
        let a2 = make_backend(cmod_core::types::Compiler::Clang, &base).unwrap();
        assert_eq!(a.fingerprint(), a2.fingerprint());
    }

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
    fn test_common_flags_lto_thin() {
        let mut backend = ClangBackend::new("20", Profile::Release);
        backend.lto = true;
        backend.lto_mode = LtoMode::Thin;
        let flags = backend.common_flags();
        assert!(flags.contains(&"-flto=thin".to_string()));
        assert!(!flags.contains(&"-flto=full".to_string()));
    }

    #[test]
    fn test_common_flags_lto_full() {
        let mut backend = ClangBackend::new("20", Profile::Release);
        backend.lto = true;
        backend.lto_mode = LtoMode::Full;
        let flags = backend.common_flags();
        assert!(flags.contains(&"-flto=full".to_string()));
        assert!(!flags.contains(&"-flto=thin".to_string()));
    }

    #[test]
    fn test_common_flags_lto_disabled() {
        let backend = ClangBackend::new("20", Profile::Release);
        let flags = backend.common_flags();
        assert!(!flags.iter().any(|f| f.starts_with("-flto")));
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
