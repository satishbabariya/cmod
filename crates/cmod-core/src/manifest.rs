use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::error::CmodError;
use crate::types::{Abi, BuildType, Compiler, OptimizationLevel};

/// Top-level cmod.toml manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub package: Package,

    #[serde(default)]
    pub module: Option<Module>,

    #[serde(default)]
    pub dependencies: BTreeMap<String, Dependency>,

    #[serde(default, rename = "dev-dependencies")]
    pub dev_dependencies: BTreeMap<String, Dependency>,

    #[serde(default, rename = "build-dependencies")]
    pub build_dependencies: BTreeMap<String, Dependency>,

    #[serde(default)]
    pub features: BTreeMap<String, Vec<String>>,

    #[serde(default)]
    pub compat: Option<Compat>,

    #[serde(default)]
    pub toolchain: Option<Toolchain>,

    #[serde(default)]
    pub build: Option<Build>,

    #[serde(default)]
    pub test: Option<Test>,

    #[serde(default)]
    pub workspace: Option<Workspace>,

    #[serde(default)]
    pub cache: Option<Cache>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub edition: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub authors: Vec<String>,
    #[serde(default)]
    pub license: Option<String>,
    #[serde(default)]
    pub repository: Option<String>,
    #[serde(default)]
    pub homepage: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Module {
    pub name: String,
    pub root: PathBuf,
}

/// A dependency can be specified as a simple version string or an expanded table.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Dependency {
    /// Simple version string: `"^1.2"`
    Simple(String),
    /// Expanded dependency specification.
    Detailed(DetailedDependency),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DetailedDependency {
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub git: Option<String>,
    #[serde(default)]
    pub branch: Option<String>,
    #[serde(default)]
    pub rev: Option<String>,
    #[serde(default)]
    pub tag: Option<String>,
    #[serde(default)]
    pub path: Option<PathBuf>,
    #[serde(default)]
    pub features: Vec<String>,
    #[serde(default)]
    pub optional: bool,
    /// Inherit from workspace dependencies.
    #[serde(default)]
    pub workspace: bool,
}

impl Dependency {
    /// Extract the version constraint string, if any.
    pub fn version_req(&self) -> Option<&str> {
        match self {
            Dependency::Simple(v) => Some(v.as_str()),
            Dependency::Detailed(d) => d.version.as_deref(),
        }
    }

    /// Extract the Git URL. For simple dependencies, the key itself is the URL.
    pub fn git_url(&self) -> Option<&str> {
        match self {
            Dependency::Simple(_) => None,
            Dependency::Detailed(d) => d.git.as_deref(),
        }
    }

    /// Check whether this is a path dependency.
    pub fn is_path(&self) -> bool {
        matches!(self, Dependency::Detailed(d) if d.path.is_some())
    }

    /// Check whether this is a workspace dependency reference.
    pub fn is_workspace(&self) -> bool {
        matches!(self, Dependency::Detailed(d) if d.workspace)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Compat {
    #[serde(default)]
    pub cpp: Option<String>,
    #[serde(default)]
    pub llvm: Option<String>,
    #[serde(default)]
    pub abi: Option<Abi>,
    #[serde(default)]
    pub platforms: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Toolchain {
    #[serde(default)]
    pub compiler: Option<Compiler>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub cxx_standard: Option<String>,
    #[serde(default)]
    pub stdlib: Option<String>,
    #[serde(default)]
    pub target: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Build {
    #[serde(default, rename = "type")]
    pub build_type: Option<BuildType>,
    #[serde(default)]
    pub optimization: Option<OptimizationLevel>,
    #[serde(default)]
    pub lto: Option<bool>,
    #[serde(default)]
    pub parallel: Option<bool>,
    #[serde(default)]
    pub incremental: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Test {
    #[serde(default)]
    pub framework: Option<String>,
    #[serde(default)]
    pub test_patterns: Vec<String>,
    #[serde(default)]
    pub exclude_patterns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Workspace {
    #[serde(default)]
    pub name: Option<String>,
    #[serde(default)]
    pub members: Vec<String>,
    #[serde(default)]
    pub exclude: Vec<String>,
    #[serde(default)]
    pub dependencies: BTreeMap<String, Dependency>,
    #[serde(default)]
    pub resolver: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Cache {
    #[serde(default)]
    pub local_path: Option<PathBuf>,
    #[serde(default)]
    pub shared_url: Option<String>,
    #[serde(default)]
    pub ttl: Option<String>,
}

impl Manifest {
    /// Load a manifest from a `cmod.toml` file path.
    pub fn load(path: &Path) -> Result<Self, CmodError> {
        let content = std::fs::read_to_string(path).map_err(|_| CmodError::ManifestNotFound {
            path: path.display().to_string(),
        })?;
        Self::from_str(&content)
    }

    /// Parse a manifest from a TOML string.
    pub fn from_str(content: &str) -> Result<Self, CmodError> {
        toml::from_str(content).map_err(|e| CmodError::InvalidManifest {
            reason: e.to_string(),
        })
    }

    /// Serialize manifest back to TOML.
    pub fn to_toml_string(&self) -> Result<String, CmodError> {
        toml::to_string_pretty(self).map_err(|e| CmodError::InvalidManifest {
            reason: e.to_string(),
        })
    }

    /// Write manifest to a file.
    pub fn save(&self, path: &Path) -> Result<(), CmodError> {
        let content = self.to_toml_string()?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Find the manifest file by searching upward from the given directory.
    pub fn find(start_dir: &Path) -> Option<PathBuf> {
        let mut dir = start_dir.to_path_buf();
        loop {
            let candidate = dir.join("cmod.toml");
            if candidate.exists() {
                return Some(candidate);
            }
            if !dir.pop() {
                return None;
            }
        }
    }

    /// Check if this manifest defines a workspace.
    pub fn is_workspace(&self) -> bool {
        self.workspace.is_some()
    }

    /// Resolve a dependency key to a Git URL.
    ///
    /// For simple deps where the key is a Git path like `github.com/fmtlib/fmt`,
    /// construct the full https URL. For detailed deps with an explicit `git` field,
    /// use that directly.
    pub fn resolve_dep_url(key: &str, dep: &Dependency) -> String {
        match dep {
            Dependency::Simple(_) => {
                format!("https://{}", key)
            }
            Dependency::Detailed(d) => {
                if let Some(git) = &d.git {
                    git.clone()
                } else {
                    format!("https://{}", key)
                }
            }
        }
    }
}

/// Create a minimal default manifest for `cmod init`.
pub fn default_manifest(name: &str) -> Manifest {
    Manifest {
        package: Package {
            name: name.to_string(),
            version: "0.1.0".to_string(),
            edition: Some("2023".to_string()),
            description: None,
            authors: vec![],
            license: None,
            repository: None,
            homepage: None,
        },
        module: Some(Module {
            name: format!("local.{}", name),
            root: PathBuf::from("src/lib.cppm"),
        }),
        dependencies: BTreeMap::new(),
        dev_dependencies: BTreeMap::new(),
        build_dependencies: BTreeMap::new(),
        features: BTreeMap::new(),
        compat: Some(Compat {
            cpp: Some(">=20".to_string()),
            llvm: None,
            abi: None,
            platforms: vec![],
        }),
        toolchain: Some(Toolchain {
            compiler: Some(Compiler::Clang),
            version: None,
            cxx_standard: Some("20".to_string()),
            stdlib: None,
            target: None,
        }),
        build: Some(Build {
            build_type: Some(BuildType::Binary),
            optimization: Some(OptimizationLevel::Debug),
            lto: Some(false),
            parallel: Some(true),
            incremental: Some(true),
        }),
        test: None,
        workspace: None,
        cache: None,
    }
}

/// Create a workspace manifest for `cmod init --workspace`.
pub fn default_workspace_manifest(name: &str) -> Manifest {
    Manifest {
        package: Package {
            name: name.to_string(),
            version: "0.1.0".to_string(),
            edition: Some("2023".to_string()),
            description: None,
            authors: vec![],
            license: None,
            repository: None,
            homepage: None,
        },
        module: None,
        dependencies: BTreeMap::new(),
        dev_dependencies: BTreeMap::new(),
        build_dependencies: BTreeMap::new(),
        features: BTreeMap::new(),
        compat: None,
        toolchain: None,
        build: None,
        test: None,
        workspace: Some(Workspace {
            name: Some(name.to_string()),
            members: vec![],
            exclude: vec![],
            dependencies: BTreeMap::new(),
            resolver: Some("2".to_string()),
        }),
        cache: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_manifest() {
        let toml_str = r#"
[package]
name = "my_math"
version = "1.0.0"

[module]
name = "github.user.my_math"
root = "src/lib.cppm"
"#;
        let manifest = Manifest::from_str(toml_str).unwrap();
        assert_eq!(manifest.package.name, "my_math");
        assert_eq!(manifest.package.version, "1.0.0");
        assert_eq!(manifest.module.as_ref().unwrap().name, "github.user.my_math");
    }

    #[test]
    fn test_parse_full_manifest() {
        let toml_str = r#"
[package]
name = "my_math"
version = "1.4.2"
edition = "2023"
description = "Math utilities"
authors = ["Jane Doe <jane@example.com>"]
license = "MIT"

[module]
name = "com.github.user.my_math"
root = "src/lib.cppm"

[dependencies]
"github.com/fmtlib/fmt" = "^10.2"
"github.com/acme/math" = { version = ">=1.0.0", features = ["simd"] }
local_utils = { path = "./utils", version = "0.1.0" }

[dev-dependencies]
"github.com/catchorg/Catch2" = "^3.4"

[toolchain]
compiler = "clang"
version = "18.1.0"
cxx_standard = "23"

[build]
type = "binary"
optimization = "release"
lto = true
parallel = true
incremental = true
"#;
        let manifest = Manifest::from_str(toml_str).unwrap();
        assert_eq!(manifest.dependencies.len(), 3);
        assert_eq!(manifest.dev_dependencies.len(), 1);
        assert!(manifest.toolchain.is_some());
    }

    #[test]
    fn test_parse_workspace_manifest() {
        let toml_str = r#"
[package]
name = "engine"
version = "0.1.0"

[workspace]
name = "github.com/acme/engine"
members = ["core", "math", "render"]
exclude = ["experimental/*"]

[workspace.dependencies]
"github.com/fmtlib/fmt" = "^10.2"
"#;
        let manifest = Manifest::from_str(toml_str).unwrap();
        assert!(manifest.is_workspace());
        let ws = manifest.workspace.unwrap();
        assert_eq!(ws.members.len(), 3);
        assert_eq!(ws.dependencies.len(), 1);
    }

    #[test]
    fn test_resolve_dep_url() {
        let dep = Dependency::Simple("^10.2".to_string());
        assert_eq!(
            Manifest::resolve_dep_url("github.com/fmtlib/fmt", &dep),
            "https://github.com/fmtlib/fmt"
        );

        let dep = Dependency::Detailed(DetailedDependency {
            version: Some("^1.0".to_string()),
            git: Some("https://github.com/acme/math.git".to_string()),
            branch: None,
            rev: None,
            tag: None,
            path: None,
            features: vec![],
            optional: false,
            workspace: false,
        });
        assert_eq!(
            Manifest::resolve_dep_url("math", &dep),
            "https://github.com/acme/math.git"
        );
    }

    #[test]
    fn test_default_manifest() {
        let manifest = default_manifest("hello");
        assert_eq!(manifest.package.name, "hello");
        assert_eq!(manifest.package.version, "0.1.0");
        let module = manifest.module.unwrap();
        assert_eq!(module.name, "local.hello");
    }
}
