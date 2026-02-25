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
