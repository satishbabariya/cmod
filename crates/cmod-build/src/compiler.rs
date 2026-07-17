use std::path::{Path, PathBuf};
use std::process::Command;

use cmod_core::error::CmodError;
use cmod_core::types::{Artifact, OptimizationLevel, Profile};

/// Abstraction over a C++ compiler backend.
///
/// Implemented by [`ClangBackend`] (reference), [`GccBackend`], and
/// [`MsvcBackend`].
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
    /// Clang emits `.pcm`, MSVC `.ifc`, GCC `.gcm`. `BuildPlan::from_graph`
    /// consumes this to shape interface/partition output paths.
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
pub fn make_backend(
    kind: cmod_core::types::Compiler,
    config: &BackendConfig,
) -> Result<Box<dyn CompilerBackend>, CmodError> {
    match kind {
        cmod_core::types::Compiler::Clang => Ok(Box::new(ClangBackend::from_config(config))),
        cmod_core::types::Compiler::Gcc => Ok(Box::new(GccBackend::from_config(config))),
        cmod_core::types::Compiler::Msvc => Ok(Box::new(MsvcBackend::from_config(config))),
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
/// GCC compiler backend (GCC 14+).
///
/// Drives GCC's `-fmodules-ts` module model. CMIs (`.gcm`) are placed at the
/// plan's BMI paths via a module-mapper file written next to the primary
/// output; dependency scanning uses `g++ -fdeps-format=p1689r5`, whose JSON
/// output feeds the same P1689 parser as `clang-scan-deps`.
///
/// Configuration notes:
/// - `[toolchain] stdlib` is ignored — GCC drives libstdc++ only.
/// - `target` is ignored for flag purposes — GCC cross-compiles via prefixed
///   toolchains (set `CXX=aarch64-linux-gnu-g++`), not a `--target` flag.
/// - LTO maps to `-flto` for both modes (ThinLTO is a Clang concept).
pub struct GccBackend {
    /// Path to the g++ executable.
    pub gxx_path: PathBuf,
    config: BackendConfig,
}

impl GccBackend {
    /// Create a GCC backend from a full [`BackendConfig`].
    pub fn from_config(config: &BackendConfig) -> Self {
        GccBackend {
            gxx_path: std::env::var_os("CXX")
                .map(PathBuf::from)
                .unwrap_or_else(|| find_executable("g++")),
            config: config.clone(),
        }
    }

    /// Write a module-mapper file next to `primary_output` and return its path.
    fn write_mapper<S: AsRef<str>>(
        &self,
        primary_output: &Path,
        entries: &[(S, PathBuf)],
    ) -> Result<PathBuf, CmodError> {
        let mut mapper_path = primary_output.as_os_str().to_owned();
        mapper_path.push(".map");
        let mapper_path = PathBuf::from(mapper_path);
        if let Some(parent) = mapper_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&mapper_path, gcc_module_mapper(entries))?;
        Ok(mapper_path)
    }

    fn run_compile(&self, cmd: &mut Command, what: &Path) -> Result<(), CmodError> {
        let status = cmd.status().map_err(|e| CmodError::BuildFailed {
            reason: format!("failed to run g++ at {}: {}", self.gxx_path.display(), e),
        })?;
        if !status.success() {
            return Err(CmodError::BuildFailed {
                reason: format!("g++ failed to compile: {}", what.display()),
            });
        }
        Ok(())
    }
}

/// Render module-mapper file content: one `<module-name> <cmi-path>` line
/// per entry.
fn gcc_module_mapper<S: AsRef<str>>(entries: &[(S, PathBuf)]) -> String {
    let mut out = String::new();
    for (name, path) in entries {
        out.push_str(name.as_ref());
        out.push(' ');
        out.push_str(&path.display().to_string());
        out.push('\n');
    }
    out
}

impl CompilerBackend for GccBackend {
    fn scan_deps(&self, source: &Path) -> Result<Vec<String>, CmodError> {
        let deps_file = std::env::temp_dir().join(format!(
            "cmod-gcc-deps-{}-{}.json",
            std::process::id(),
            source.file_stem().and_then(|s| s.to_str()).unwrap_or("src")
        ));

        let output = Command::new(&self.gxx_path)
            .args(self.config_flags())
            .arg("-fdeps-format=p1689r5")
            .arg(format!("-fdeps-file={}", deps_file.display()))
            .arg("-fdeps-target=scan.o")
            .arg("-E")
            .arg("-x")
            .arg("c++")
            .arg(source)
            .output()
            .map_err(|e| CmodError::ModuleScanFailed {
                reason: format!("failed to run g++ at {}: {}", self.gxx_path.display(), e),
            })?;

        if !output.status.success() {
            let _ = std::fs::remove_file(&deps_file);
            return Err(CmodError::ModuleScanFailed {
                reason: format!(
                    "g++ dependency scan failed: {}",
                    String::from_utf8_lossy(&output.stderr)
                ),
            });
        }

        let json =
            std::fs::read_to_string(&deps_file).map_err(|e| CmodError::ModuleScanFailed {
                reason: format!("failed to read g++ deps file: {}", e),
            })?;
        let _ = std::fs::remove_file(&deps_file);
        parse_p1689_imports(&json)
    }

    fn compile_interface(
        &self,
        source: &Path,
        pcm_output: &Path,
        obj_output: &Path,
        dep_pcms: &[(&str, &Path)],
    ) -> Result<(), CmodError> {
        // GCC emits the CMI and the object in one pass; the mapper routes the
        // CMI to the plan's path and resolves imported modules' CMIs.
        // The mapper keys on module names as written in source. Map the
        // sanitized file stem and, when recoverable, the real declared module
        // name — GCC accepts multiple mapper lines pointing at the same CMI.
        let module_stem = pcm_output
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("module")
            .to_string();
        let mut entries: Vec<(String, PathBuf)> = vec![(module_stem, pcm_output.to_path_buf())];
        entries.extend(
            dep_pcms
                .iter()
                .map(|(n, p)| (n.to_string(), p.to_path_buf())),
        );
        if let Ok(content) = std::fs::read_to_string(source) {
            if let Ok(Some(real_name)) = crate::runner::extract_module_name_from_content(&content) {
                entries.push((real_name, pcm_output.to_path_buf()));
            }
        }

        let mapper = self.write_mapper(pcm_output, &entries)?;

        let mut cmd = Command::new(&self.gxx_path);
        cmd.args(self.config_flags())
            .arg(format!("-fmodule-mapper={}", mapper.display()))
            .arg("-x")
            .arg("c++")
            .arg("-c")
            .arg("-o")
            .arg(obj_output)
            .arg(source);
        self.run_compile(&mut cmd, source)
    }

    fn compile_implementation(
        &self,
        source: &Path,
        obj_output: &Path,
        dep_pcms: &[(&str, &Path)],
    ) -> Result<(), CmodError> {
        let entries: Vec<(String, PathBuf)> = dep_pcms
            .iter()
            .map(|(n, p)| (n.to_string(), p.to_path_buf()))
            .collect();
        let mapper = self.write_mapper(obj_output, &entries)?;

        let mut cmd = Command::new(&self.gxx_path);
        cmd.args(self.config_flags())
            .arg(format!("-fmodule-mapper={}", mapper.display()))
            .arg("-c")
            .arg("-o")
            .arg(obj_output)
            .arg(source);
        self.run_compile(&mut cmd, source)
    }

    fn link(&self, objects: &[&Path], output: &Path, artifact: &Artifact) -> Result<(), CmodError> {
        match artifact {
            Artifact::StaticLib { .. } => {
                let obj_only: Vec<&&Path> = objects
                    .iter()
                    .filter(|p| p.extension().and_then(|e| e.to_str()) != Some("a"))
                    .collect();
                if obj_only.is_empty() {
                    return Ok(());
                }
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
                Ok(())
            }
            other => {
                let mut cmd = Command::new(&self.gxx_path);
                cmd.args(self.config_flags());
                if matches!(other, Artifact::SharedLib { .. }) {
                    cmd.arg("-shared");
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
    }

    fn kind(&self) -> cmod_core::types::Compiler {
        cmod_core::types::Compiler::Gcc
    }

    fn compiler_path(&self) -> &Path {
        &self.gxx_path
    }

    fn version(&self) -> String {
        let out = Command::new(&self.gxx_path).arg("--version").output();
        let stdout = match out {
            Ok(o) if o.status.success() => String::from_utf8_lossy(&o.stdout).into_owned(),
            _ => return String::new(),
        };
        stdout
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
        self.config_flags()
    }

    fn fingerprint(&self) -> String {
        format!(
            "gcc|std={}|sysroot={}|profile={:?}|lto={}|opt={:?}|flags={}",
            self.config.cxx_standard,
            self.config
                .sysroot
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_default(),
            self.config.profile,
            self.config.lto,
            self.config.optimization,
            self.config.extra_flags.join(" "),
        )
    }

    fn bmi_extension(&self) -> &'static str {
        "gcm"
    }
}

impl GccBackend {
    /// Flags shared by every g++ invocation.
    fn config_flags(&self) -> Vec<String> {
        let mut flags = vec![
            format!("-std=c++{}", self.config.cxx_standard),
            "-fmodules-ts".to_string(),
        ];
        // stdlib and target deliberately ignored — see type-level docs.
        if let Some(ref sysroot) = self.config.sysroot {
            flags.push(format!("--sysroot={}", sysroot.display()));
        }
        match self.config.optimization {
            Some(OptimizationLevel::Debug) => flags.extend(["-g".into(), "-O0".into()]),
            Some(OptimizationLevel::Release) => flags.extend(["-O2".into(), "-DNDEBUG".into()]),
            Some(OptimizationLevel::Size) => flags.extend(["-Os".into(), "-DNDEBUG".into()]),
            Some(OptimizationLevel::Speed) => flags.extend(["-O3".into(), "-DNDEBUG".into()]),
            None => match self.config.profile {
                Profile::Debug => flags.extend(["-g".into(), "-O0".into()]),
                Profile::Release => flags.extend(["-O2".into(), "-DNDEBUG".into()]),
            },
        }
        if self.config.lto {
            flags.push("-flto".to_string());
        }
        flags.extend(self.config.extra_flags.clone());
        flags
    }
}

/// MSVC compiler backend (VS 2022 17.6+).
///
/// Drives `cl.exe`'s C++20 modules model: `/interface /TP` interface
/// compilation with `/ifcOutput` BMI placement, `/reference name=path.ifc`
/// per dependency, `cl /scanDependencies` P1689 scanning (shared parser
/// with the clang path), `lib.exe`/`link.exe` archiving/linking. Requires
/// the VS developer environment (vcvars) so `cl`/`link`/`lib` resolve.
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

    /// Arguments for compiling a module interface/partition unit.
    /// `/interface /TP` forces interface-unit semantics for any extension
    /// (cmod scaffolds `.cppm`, MSVC convention is `.ixx` — both work).
    pub fn interface_args(
        &self,
        source: &Path,
        ifc_output: &Path,
        obj_output: &Path,
        dep_ifcs: &[(&str, &Path)],
    ) -> Vec<String> {
        let mut args = CompilerBackend::common_flags(self);
        args.push("/c".to_string());
        args.push("/interface".to_string());
        args.push("/TP".to_string());
        args.push("/ifcOutput".to_string());
        args.push(ifc_output.display().to_string());
        args.push(format!("/Fo{}", obj_output.display()));
        for (name, path) in dep_ifcs {
            args.push("/reference".to_string());
            args.push(format!("{}={}", name, path.display()));
        }
        args.push(source.display().to_string());
        args
    }

    /// Arguments for compiling an implementation or legacy unit.
    pub fn implementation_args(
        &self,
        source: &Path,
        obj_output: &Path,
        dep_ifcs: &[(&str, &Path)],
    ) -> Vec<String> {
        let mut args = CompilerBackend::common_flags(self);
        args.push("/c".to_string());
        args.push(format!("/Fo{}", obj_output.display()));
        for (name, path) in dep_ifcs {
            args.push("/reference".to_string());
            args.push(format!("{}={}", name, path.display()));
        }
        args.push(source.display().to_string());
        args
    }

    /// Arguments for P1689 dependency scanning (`cl /scanDependencies`).
    pub fn scan_args(&self, source: &Path, deps_file: &Path) -> Vec<String> {
        let mut args = CompilerBackend::common_flags(self);
        args.push("/scanDependencies".to_string());
        args.push(deps_file.display().to_string());
        args.push(source.display().to_string());
        args
    }

    fn run_cl(&self, args: &[String], what: &Path) -> Result<(), CmodError> {
        let output = Command::new(&self.cl_path)
            .args(args)
            .output()
            .map_err(|e| CmodError::BuildFailed {
                reason: format!(
                    "failed to run cl at {}: {} (MSVC builds need the VS developer \
                     environment — run from a Developer Prompt or after vcvars)",
                    self.cl_path.display(),
                    e
                ),
            })?;
        if !output.status.success() {
            return Err(CmodError::BuildFailed {
                reason: format!(
                    "cl failed to compile {}: {}",
                    what.display(),
                    String::from_utf8_lossy(&output.stdout)
                ),
            });
        }
        Ok(())
    }
}

impl CompilerBackend for MsvcBackend {
    fn scan_deps(&self, source: &Path) -> Result<Vec<String>, CmodError> {
        let deps_file = std::env::temp_dir().join(format!(
            "cmod-msvc-deps-{}-{}.json",
            std::process::id(),
            source.file_stem().and_then(|s| s.to_str()).unwrap_or("src")
        ));
        let mut args = self.scan_args(source, &deps_file);
        args.push("/c".to_string());
        let output = Command::new(&self.cl_path)
            .args(&args)
            .output()
            .map_err(|e| CmodError::ModuleScanFailed {
                reason: format!("failed to run cl at {}: {}", self.cl_path.display(), e),
            })?;
        if !output.status.success() {
            let _ = std::fs::remove_file(&deps_file);
            return Err(CmodError::ModuleScanFailed {
                reason: format!(
                    "cl dependency scan failed: {}",
                    String::from_utf8_lossy(&output.stdout)
                ),
            });
        }
        let json =
            std::fs::read_to_string(&deps_file).map_err(|e| CmodError::ModuleScanFailed {
                reason: format!("failed to read cl deps file: {}", e),
            })?;
        let _ = std::fs::remove_file(&deps_file);
        parse_p1689_imports(&json)
    }

    fn compile_interface(
        &self,
        source: &Path,
        pcm_output: &Path,
        obj_output: &Path,
        dep_pcms: &[(&str, &Path)],
    ) -> Result<(), CmodError> {
        if let Some(parent) = pcm_output.parent() {
            std::fs::create_dir_all(parent)?;
        }
        if let Some(parent) = obj_output.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let args = self.interface_args(source, pcm_output, obj_output, dep_pcms);
        self.run_cl(&args, source)
    }

    fn compile_implementation(
        &self,
        source: &Path,
        obj_output: &Path,
        dep_pcms: &[(&str, &Path)],
    ) -> Result<(), CmodError> {
        if let Some(parent) = obj_output.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let args = self.implementation_args(source, obj_output, dep_pcms);
        self.run_cl(&args, source)
    }

    fn link(&self, objects: &[&Path], output: &Path, artifact: &Artifact) -> Result<(), CmodError> {
        let obj_only: Vec<&&Path> = objects
            .iter()
            .filter(|p| {
                !matches!(
                    p.extension().and_then(|e| e.to_str()),
                    Some("a") | Some("lib")
                )
            })
            .collect();
        let (tool, mut args): (&str, Vec<String>) = match artifact {
            Artifact::StaticLib { .. } => {
                if obj_only.is_empty() {
                    return Ok(());
                }
                let _ = std::fs::remove_file(output);
                (
                    "lib",
                    vec!["/nologo".to_string(), format!("/OUT:{}", output.display())],
                )
            }
            Artifact::SharedLib { .. } => (
                "link",
                vec![
                    "/nologo".to_string(),
                    "/DLL".to_string(),
                    format!("/OUT:{}", output.display()),
                ],
            ),
            _ => (
                "link",
                vec!["/nologo".to_string(), format!("/OUT:{}", output.display())],
            ),
        };
        for obj in &obj_only {
            args.push(obj.display().to_string());
        }
        let status =
            Command::new(tool)
                .args(&args)
                .status()
                .map_err(|e| CmodError::BuildFailed {
                    reason: format!("failed to run {}: {}", tool, e),
                })?;
        if !status.success() {
            return Err(CmodError::BuildFailed {
                reason: format!("{} failed to produce {}", tool, output.display()),
            });
        }
        Ok(())
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

    // --- MSVC backend (#48 skeleton -> #77 implementation) ---

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
    fn test_make_backend_gcc_constructs() {
        let cfg = BackendConfig {
            cxx_standard: "20".to_string(),
            ..Default::default()
        };
        let backend = make_backend(cmod_core::types::Compiler::Gcc, &cfg).unwrap();
        assert_eq!(backend.kind(), cmod_core::types::Compiler::Gcc);
        assert_eq!(backend.bmi_extension(), "gcm");
    }

    #[test]
    fn test_gcc_backend_flags() {
        let cfg = BackendConfig {
            cxx_standard: "20".to_string(),
            stdlib: Some("libc++".to_string()), // GCC drives libstdc++ only — must be ignored
            extra_flags: vec!["-DBAR=2".to_string()],
            ..Default::default()
        };
        let backend = GccBackend::from_config(&cfg);
        let flags = CompilerBackend::common_flags(&backend);
        assert!(flags.contains(&"-std=c++20".to_string()));
        assert!(flags.contains(&"-fmodules-ts".to_string()));
        assert!(flags.contains(&"-DBAR=2".to_string()));
        assert!(
            !flags.iter().any(|f| f.contains("stdlib")),
            "gcc must not receive -stdlib, got: {:?}",
            flags
        );
    }

    #[test]
    fn test_gcc_module_mapper_content() {
        let entries = [
            ("local.app", PathBuf::from("/b/pcm/local_app.gcm")),
            ("local.dep", PathBuf::from("/b/pcm/local_dep.gcm")),
        ];
        let mapper = gcc_module_mapper(&entries);
        assert_eq!(
            mapper,
            "local.app /b/pcm/local_app.gcm\nlocal.dep /b/pcm/local_dep.gcm\n"
        );
    }

    #[test]
    fn test_gcc_fingerprint_deterministic_and_distinct() {
        let cfg = BackendConfig {
            cxx_standard: "20".to_string(),
            ..Default::default()
        };
        let a = GccBackend::from_config(&cfg);
        let b = GccBackend::from_config(&cfg);
        assert_eq!(
            CompilerBackend::fingerprint(&a),
            CompilerBackend::fingerprint(&b)
        );
        let clang = ClangBackend::from_config(&cfg);
        assert_ne!(
            CompilerBackend::fingerprint(&a),
            CompilerBackend::fingerprint(&clang)
        );
    }

    #[test]
    fn test_make_backend_msvc_constructs() {
        let cfg = BackendConfig {
            cxx_standard: "20".to_string(),
            ..Default::default()
        };
        let backend = make_backend(cmod_core::types::Compiler::Msvc, &cfg).unwrap();
        assert_eq!(backend.kind(), cmod_core::types::Compiler::Msvc);
        assert_eq!(backend.bmi_extension(), "ifc");
    }

    #[test]
    fn test_msvc_interface_args_shape() {
        let cfg = BackendConfig {
            cxx_standard: "20".to_string(),
            ..Default::default()
        };
        let backend = MsvcBackend::from_config(&cfg);
        let args = backend.interface_args(
            Path::new("src/m.cppm"),
            Path::new("build/pcm/m.ifc"),
            Path::new("build/obj/m.o"),
            &[("dep", Path::new("build/pcm/dep.ifc"))],
        );
        assert!(args.contains(&"/c".to_string()));
        assert!(args.contains(&"/interface".to_string()));
        assert!(args.contains(&"/TP".to_string()));
        assert!(args.contains(&"/ifcOutput".to_string()));
        assert!(args.contains(&"build/pcm/m.ifc".to_string()));
        assert!(args.iter().any(|a| a.starts_with("/Fo")));
        assert!(args.iter().any(|a| a == "/reference"));
        assert!(args
            .iter()
            .any(|a| a.contains("dep=") && a.contains("dep.ifc")));
        assert!(args.contains(&"src/m.cppm".to_string()));
    }

    #[test]
    fn test_msvc_implementation_args_shape() {
        let cfg = BackendConfig {
            cxx_standard: "20".to_string(),
            ..Default::default()
        };
        let backend = MsvcBackend::from_config(&cfg);
        let args = backend.implementation_args(
            Path::new("src/main.cpp"),
            Path::new("build/obj/main.o"),
            &[("dep", Path::new("build/pcm/dep.ifc"))],
        );
        assert!(args.contains(&"/c".to_string()));
        assert!(!args.contains(&"/interface".to_string()));
        assert!(args
            .iter()
            .any(|a| a.contains("dep=") && a.contains("dep.ifc")));
        assert!(args.contains(&"src/main.cpp".to_string()));
    }

    #[test]
    fn test_msvc_scan_args_shape() {
        let cfg = BackendConfig {
            cxx_standard: "20".to_string(),
            ..Default::default()
        };
        let backend = MsvcBackend::from_config(&cfg);
        let args = backend.scan_args(Path::new("src/m.cppm"), Path::new("deps.json"));
        assert!(args.contains(&"/scanDependencies".to_string()));
        assert!(args.contains(&"deps.json".to_string()));
        assert!(args.contains(&"src/m.cppm".to_string()));
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
