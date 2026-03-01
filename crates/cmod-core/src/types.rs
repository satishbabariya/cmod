use serde::{Deserialize, Serialize};
use std::fmt;
use std::path::PathBuf;

/// A module identity derived from a Git URL in reverse-domain format.
/// Example: `github.fmtlib.fmt` from `https://github.com/fmtlib/fmt`
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ModuleId(pub String);

impl ModuleId {
    /// Construct a module ID from a Git URL.
    ///
    /// `https://github.com/fmtlib/fmt` → `github.fmtlib.fmt`
    /// `https://gitlab.com/org/infra/log` → `gitlab.org.infra.log`
    pub fn from_git_url(url: &str) -> Option<Self> {
        let url = url
            .trim_end_matches('/')
            .trim_end_matches(".git");

        // Strip protocol and normalize
        let path_str = if let Some(rest) = url.strip_prefix("https://") {
            rest.to_string()
        } else if let Some(rest) = url.strip_prefix("http://") {
            rest.to_string()
        } else if let Some(rest) = url.strip_prefix("ssh://git@") {
            rest.to_string()
        } else if let Some(rest) = url.strip_prefix("git@") {
            rest.replace(':', "/")
        } else {
            return None;
        };

        let path = path_str.replace(':', "/");

        let parts: Vec<&str> = path.split('/').filter(|s| !s.is_empty()).collect();
        if parts.len() < 2 {
            return None;
        }

        // First part is the host (e.g., github.com → github)
        let host = parts[0].split('.').next().unwrap_or(parts[0]);
        let rest: Vec<&str> = parts[1..].to_vec();

        let module_name = std::iter::once(host)
            .chain(rest.into_iter())
            .collect::<Vec<&str>>()
            .join(".");

        Some(ModuleId(module_name))
    }

    /// Check whether this module ID uses a reserved prefix (std.*, stdx.*).
    pub fn is_reserved(&self) -> bool {
        self.0.starts_with("std.") || self.0.starts_with("stdx.") || self.0 == "std" || self.0 == "stdx"
    }

    /// Check whether this is a local-only module.
    pub fn is_local(&self) -> bool {
        self.0.starts_with("local.")
    }
}

impl fmt::Display for ModuleId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The kind of C++ module unit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModuleUnitKind {
    /// `export module foo;` — primary module interface
    InterfaceUnit,
    /// Module implementation unit
    ImplementationUnit,
    /// `export module foo:bar;` — module partition
    PartitionUnit,
    /// Non-module translation unit (legacy headers)
    LegacyUnit,
}

/// Build output type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BuildType {
    Binary,
    StaticLib,
    SharedLib,
}

impl Default for BuildType {
    fn default() -> Self {
        BuildType::Binary
    }
}

/// Build optimization profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OptimizationLevel {
    Debug,
    Release,
    Size,
    Speed,
}

impl Default for OptimizationLevel {
    fn default() -> Self {
        OptimizationLevel::Debug
    }
}

/// Supported compiler backends.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Compiler {
    Clang,
    Gcc,
    Msvc,
}

impl Default for Compiler {
    fn default() -> Self {
        Compiler::Clang
    }
}

impl fmt::Display for Compiler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Compiler::Clang => write!(f, "clang"),
            Compiler::Gcc => write!(f, "gcc"),
            Compiler::Msvc => write!(f, "msvc"),
        }
    }
}

/// ABI variant.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Abi {
    Itanium,
    Msvc,
}

/// Build artifact produced by a build node.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum Artifact {
    Pcm { path: PathBuf },
    ObjectFile { path: PathBuf },
    StaticLib { path: PathBuf },
    SharedLib { path: PathBuf },
    Executable { path: PathBuf },
}

impl Artifact {
    pub fn path(&self) -> &PathBuf {
        match self {
            Artifact::Pcm { path }
            | Artifact::ObjectFile { path }
            | Artifact::StaticLib { path }
            | Artifact::SharedLib { path }
            | Artifact::Executable { path } => path,
        }
    }
}

/// Build node kind in the build plan IR.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    /// Compile module interface → PCM + object
    Interface,
    /// Compile module implementation → object
    Implementation,
    /// Compile non-module translation unit → object
    Object,
    /// Link objects → binary/library
    Link,
}

/// Build profile (debug/release).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Profile {
    Debug,
    Release,
}

impl Default for Profile {
    fn default() -> Self {
        Profile::Debug
    }
}

/// Resolved toolchain specification for build orchestration.
///
/// Constructed from the `[toolchain]` manifest section, CLI overrides,
/// and environment detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolchainSpec {
    /// Compiler backend (clang, gcc, msvc).
    pub compiler: Compiler,
    /// Compiler version string (e.g., "18.1.0").
    pub compiler_version: Option<String>,
    /// C++ standard (e.g., "20", "23").
    pub cxx_standard: String,
    /// Standard library (e.g., "libc++", "libstdc++").
    pub stdlib: Option<String>,
    /// ABI variant.
    pub abi: Option<Abi>,
    /// Target triple (e.g., "x86_64-unknown-linux-gnu").
    pub target: String,
    /// Sysroot path for cross-compilation.
    pub sysroot: Option<PathBuf>,
}

impl ToolchainSpec {
    /// Detect the host target triple.
    pub fn host_target() -> String {
        let arch = std::env::consts::ARCH;
        let os = std::env::consts::OS;
        match (arch, os) {
            ("x86_64", "linux") => "x86_64-unknown-linux-gnu".to_string(),
            ("x86_64", "macos") => "x86_64-apple-darwin".to_string(),
            ("aarch64", "linux") => "aarch64-unknown-linux-gnu".to_string(),
            ("aarch64", "macos") => "arm64-apple-darwin".to_string(),
            ("x86_64", "windows") => "x86_64-pc-windows-msvc".to_string(),
            _ => format!("{}-unknown-{}", arch, os),
        }
    }

    /// Whether this spec targets a different platform than the host.
    pub fn is_cross_compiling(&self) -> bool {
        self.target != Self::host_target()
    }

    /// Validate that the toolchain is available.
    pub fn validate(&self) -> Result<(), crate::error::CmodError> {
        let cmd = match self.compiler {
            Compiler::Clang => "clang++",
            Compiler::Gcc => "g++",
            Compiler::Msvc => "cl",
        };

        match std::process::Command::new(cmd)
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
        {
            Ok(status) if status.success() => Ok(()),
            _ => Err(crate::error::CmodError::CompilerNotFound {
                compiler: cmd.to_string(),
            }),
        }
    }

    /// Build a compact key string for cache isolation.
    pub fn cache_key_tuple(&self) -> String {
        format!(
            "{}-{}-std{}-{}-{}",
            self.compiler,
            self.compiler_version.as_deref().unwrap_or("unknown"),
            self.cxx_standard,
            self.stdlib.as_deref().unwrap_or("default"),
            self.target,
        )
    }
}

impl Default for ToolchainSpec {
    fn default() -> Self {
        ToolchainSpec {
            compiler: Compiler::default(),
            compiler_version: None,
            cxx_standard: "20".to_string(),
            stdlib: None,
            abi: None,
            target: Self::host_target(),
            sysroot: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── ModuleId ───────────────────────────────────────────────

    #[test]
    fn test_module_id_from_https_github() {
        let id = ModuleId::from_git_url("https://github.com/fmtlib/fmt").unwrap();
        assert_eq!(id.0, "github.fmtlib.fmt");
    }

    #[test]
    fn test_module_id_from_https_gitlab_deep() {
        let id = ModuleId::from_git_url("https://gitlab.com/org/infra/log").unwrap();
        assert_eq!(id.0, "gitlab.org.infra.log");
    }

    #[test]
    fn test_module_id_strips_dotgit() {
        let id = ModuleId::from_git_url("https://github.com/user/repo.git").unwrap();
        assert_eq!(id.0, "github.user.repo");
    }

    #[test]
    fn test_module_id_strips_trailing_slash() {
        let id = ModuleId::from_git_url("https://github.com/user/repo/").unwrap();
        assert_eq!(id.0, "github.user.repo");
    }

    #[test]
    fn test_module_id_from_ssh() {
        let id = ModuleId::from_git_url("ssh://git@github.com/user/repo").unwrap();
        assert_eq!(id.0, "github.user.repo");
    }

    #[test]
    fn test_module_id_from_git_at() {
        let id = ModuleId::from_git_url("git@github.com:fmtlib/fmt.git").unwrap();
        assert_eq!(id.0, "github.fmtlib.fmt");
    }

    #[test]
    fn test_module_id_from_http() {
        let id = ModuleId::from_git_url("http://example.com/org/proj").unwrap();
        assert_eq!(id.0, "example.org.proj");
    }

    #[test]
    fn test_module_id_invalid_no_protocol() {
        assert!(ModuleId::from_git_url("just-a-name").is_none());
    }

    #[test]
    fn test_module_id_invalid_too_short() {
        assert!(ModuleId::from_git_url("https://github.com").is_none());
    }

    #[test]
    fn test_module_id_is_reserved_std() {
        assert!(ModuleId("std".to_string()).is_reserved());
        assert!(ModuleId("std.io".to_string()).is_reserved());
        assert!(ModuleId("stdx".to_string()).is_reserved());
        assert!(ModuleId("stdx.ranges".to_string()).is_reserved());
    }

    #[test]
    fn test_module_id_is_not_reserved() {
        assert!(!ModuleId("github.fmtlib.fmt".to_string()).is_reserved());
        assert!(!ModuleId("stdlib_compat".to_string()).is_reserved());
    }

    #[test]
    fn test_module_id_is_local() {
        assert!(ModuleId("local.utils".to_string()).is_local());
        assert!(ModuleId("local.my_project".to_string()).is_local());
    }

    #[test]
    fn test_module_id_not_local() {
        assert!(!ModuleId("github.fmtlib.fmt".to_string()).is_local());
    }

    #[test]
    fn test_module_id_display() {
        let id = ModuleId("github.fmtlib.fmt".to_string());
        assert_eq!(format!("{}", id), "github.fmtlib.fmt");
    }

    // ─── Defaults ───────────────────────────────────────────────

    #[test]
    fn test_build_type_default() {
        assert_eq!(BuildType::default(), BuildType::Binary);
    }

    #[test]
    fn test_optimization_level_default() {
        assert_eq!(OptimizationLevel::default(), OptimizationLevel::Debug);
    }

    #[test]
    fn test_compiler_default() {
        assert_eq!(Compiler::default(), Compiler::Clang);
    }

    #[test]
    fn test_profile_default() {
        assert_eq!(Profile::default(), Profile::Debug);
    }

    // ─── Compiler Display ───────────────────────────────────────

    #[test]
    fn test_compiler_display() {
        assert_eq!(format!("{}", Compiler::Clang), "clang");
        assert_eq!(format!("{}", Compiler::Gcc), "gcc");
        assert_eq!(format!("{}", Compiler::Msvc), "msvc");
    }

    // ─── Artifact ───────────────────────────────────────────────

    #[test]
    fn test_artifact_path() {
        let a = Artifact::Pcm {
            path: PathBuf::from("/tmp/mod.pcm"),
        };
        assert_eq!(a.path(), &PathBuf::from("/tmp/mod.pcm"));

        let b = Artifact::Executable {
            path: PathBuf::from("/tmp/app"),
        };
        assert_eq!(b.path(), &PathBuf::from("/tmp/app"));
    }

    // ─── Serde roundtrip ────────────────────────────────────────

    #[test]
    fn test_module_unit_kind_serde() {
        let json = serde_json::to_string(&ModuleUnitKind::InterfaceUnit).unwrap();
        assert_eq!(json, "\"interface_unit\"");

        let parsed: ModuleUnitKind = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, ModuleUnitKind::InterfaceUnit);
    }

    #[test]
    fn test_build_type_serde() {
        let json = serde_json::to_string(&BuildType::SharedLib).unwrap();
        assert_eq!(json, "\"shared-lib\"");

        let parsed: BuildType = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, BuildType::SharedLib);
    }

    #[test]
    fn test_node_kind_serde() {
        let json = serde_json::to_string(&NodeKind::Interface).unwrap();
        assert_eq!(json, "\"interface\"");

        let parsed: NodeKind = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed, NodeKind::Interface);
    }

    // ─── ToolchainSpec ─────────────────────────────────────────

    #[test]
    fn test_toolchain_spec_default() {
        let spec = ToolchainSpec::default();
        assert_eq!(spec.compiler, Compiler::Clang);
        assert_eq!(spec.cxx_standard, "20");
        assert!(spec.compiler_version.is_none());
        assert!(spec.stdlib.is_none());
        assert!(spec.sysroot.is_none());
        assert!(!spec.target.is_empty());
    }

    #[test]
    fn test_toolchain_spec_host_target() {
        let target = ToolchainSpec::host_target();
        assert!(!target.is_empty());
        assert!(target.contains('-'));
    }

    #[test]
    fn test_toolchain_spec_is_cross_compiling() {
        let mut spec = ToolchainSpec::default();
        assert!(!spec.is_cross_compiling());

        spec.target = "aarch64-unknown-none".to_string();
        // Only cross-compiling if target != host
        if ToolchainSpec::host_target() != "aarch64-unknown-none" {
            assert!(spec.is_cross_compiling());
        }
    }

    #[test]
    fn test_toolchain_spec_cache_key_tuple() {
        let spec = ToolchainSpec {
            compiler: Compiler::Clang,
            compiler_version: Some("18.1.0".to_string()),
            cxx_standard: "23".to_string(),
            stdlib: Some("libc++".to_string()),
            abi: None,
            target: "x86_64-unknown-linux-gnu".to_string(),
            sysroot: None,
        };
        let key = spec.cache_key_tuple();
        assert!(key.contains("clang"));
        assert!(key.contains("18.1.0"));
        assert!(key.contains("std23"));
        assert!(key.contains("libc++"));
        assert!(key.contains("x86_64"));
    }

    #[test]
    fn test_toolchain_spec_cache_key_defaults() {
        let spec = ToolchainSpec::default();
        let key = spec.cache_key_tuple();
        assert!(key.contains("unknown"));
        assert!(key.contains("default"));
    }
}
