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

    #[serde(default)]
    pub metadata: Option<Metadata>,

    #[serde(default)]
    pub security: Option<Security>,

    #[serde(default)]
    pub publish: Option<Publish>,

    #[serde(default)]
    pub hooks: Option<Hooks>,

    /// Target-specific dependencies, keyed by cfg expression string.
    ///
    /// In `cmod.toml` these appear as:
    /// ```toml
    /// [target.'cfg(target_os = "linux")'.dependencies]
    /// liburing = "^2.0"
    /// ```
    #[serde(default)]
    pub target: BTreeMap<String, TargetSpec>,
}

/// Target-specific configuration block.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TargetSpec {
    #[serde(default)]
    pub dependencies: BTreeMap<String, Dependency>,
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
    /// Whether to use default features (defaults to true).
    #[serde(default = "default_true")]
    pub default_features: bool,
    /// Inherit from workspace dependencies.
    #[serde(default)]
    pub workspace: bool,
}

fn default_true() -> bool {
    true
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
    #[serde(default)]
    pub sysroot: Option<PathBuf>,
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
    /// Unified version for all workspace members (optional).
    #[serde(default)]
    pub version: Option<String>,
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
    /// Time-to-live for cache entries (e.g., "7d", "24h", "30m").
    #[serde(default)]
    pub ttl: Option<String>,
    /// Maximum total cache size in human-readable form (e.g., "1G", "500M").
    #[serde(default)]
    pub max_size: Option<String>,
}

/// Project metadata for discoverability and documentation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Metadata {
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub keywords: Vec<String>,
    #[serde(default)]
    pub links: BTreeMap<String, String>,
    #[serde(default)]
    pub documentation: Option<String>,
}

/// Security configuration for the project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Security {
    /// Signing configuration (GPG key ID, SSH key path, etc.).
    #[serde(default)]
    pub signing_key: Option<String>,
    /// Whether to verify content hashes on dependency fetch.
    #[serde(default)]
    pub verify_checksums: Option<bool>,
    /// Trusted source URLs/patterns.
    #[serde(default)]
    pub trusted_sources: Vec<String>,
    /// Required signature policy: "none", "warn", "require".
    #[serde(default)]
    pub signature_policy: Option<String>,
}

/// Publish configuration for distributing the module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Publish {
    /// Target registry (Git URL or custom server).
    #[serde(default)]
    pub registry: Option<String>,
    /// File patterns to include in published package.
    #[serde(default)]
    pub include: Vec<String>,
    /// File patterns to exclude from published package.
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Tags for the release.
    #[serde(default)]
    pub tags: Vec<String>,
}

/// Build lifecycle hooks.
///
/// Shell commands executed at specific points in the build lifecycle.
/// Hooks run in the project root directory and fail the build on non-zero exit.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Hooks {
    /// Command to run before building starts.
    #[serde(default, rename = "pre-build")]
    pub pre_build: Option<String>,
    /// Command to run after a successful build.
    #[serde(default, rename = "post-build")]
    pub post_build: Option<String>,
    /// Command to run before publishing.
    #[serde(default, rename = "pre-publish")]
    pub pre_publish: Option<String>,
    /// Command to run before testing.
    #[serde(default, rename = "pre-test")]
    pub pre_test: Option<String>,
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

    /// Validate the manifest for common issues.
    pub fn validate(&self) -> Result<(), CmodError> {
        // Package name must be non-empty
        if self.package.name.is_empty() {
            return Err(CmodError::InvalidManifest {
                reason: "package.name must not be empty".to_string(),
            });
        }

        // Package name must be a valid identifier
        if !self.package.name.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-') {
            return Err(CmodError::InvalidManifest {
                reason: format!(
                    "package.name '{}' contains invalid characters (only alphanumeric, _, -)",
                    self.package.name
                ),
            });
        }

        // Version must parse as semver
        if semver::Version::parse(&self.package.version).is_err() {
            return Err(CmodError::InvalidManifest {
                reason: format!(
                    "package.version '{}' is not valid semver",
                    self.package.version
                ),
            });
        }

        // Module name, if specified, should match reverse-domain format
        if let Some(ref module) = self.module {
            if module.name.is_empty() {
                return Err(CmodError::InvalidManifest {
                    reason: "module.name must not be empty".to_string(),
                });
            }
        }

        // Check for duplicate dependency keys
        for (name, dep) in &self.dependencies {
            if let Dependency::Detailed(d) = dep {
                // A dep can't be both git and path
                if d.git.is_some() && d.path.is_some() {
                    return Err(CmodError::InvalidManifest {
                        reason: format!(
                            "dependency '{}' specifies both `git` and `path`",
                            name
                        ),
                    });
                }
            }
        }

        // Security policy must be valid if specified
        if let Some(ref sec) = self.security {
            if let Some(ref policy) = sec.signature_policy {
                if !["none", "warn", "require"].contains(&policy.as_str()) {
                    return Err(CmodError::InvalidManifest {
                        reason: format!(
                            "security.signature_policy '{}' must be 'none', 'warn', or 'require'",
                            policy
                        ),
                    });
                }
            }
        }

        Ok(())
    }

    /// Get the effective set of dependencies for a given target triple.
    ///
    /// Merges the base `[dependencies]` with any matching `[target.'cfg(...)'.dependencies]`.
    pub fn effective_dependencies(&self, target_triple: &str) -> BTreeMap<String, Dependency> {
        let mut deps = self.dependencies.clone();

        for (cfg_expr, spec) in &self.target {
            if eval_cfg(cfg_expr, target_triple) {
                for (name, dep) in &spec.dependencies {
                    deps.entry(name.clone()).or_insert_with(|| dep.clone());
                }
            }
        }

        deps
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

/// Evaluate a `cfg(...)` expression against a target triple.
///
/// Supports:
/// - `cfg(target_os = "linux")` — matches the OS portion of the triple
/// - `cfg(target_arch = "x86_64")` — matches the arch portion
/// - `cfg(target_family = "unix")` — unix = linux/macos/freebsd; windows = windows
/// - `cfg(unix)` — shorthand for unix family
/// - `cfg(windows)` — shorthand for windows family
/// - `cfg(all(...))` — all conditions must match
/// - `cfg(any(...))` — at least one condition must match
/// - `cfg(not(...))` — negation
/// - Plain triple matching: `x86_64-unknown-linux-gnu` (literal target key)
pub fn eval_cfg(expr: &str, target_triple: &str) -> bool {
    let trimmed = expr.trim();

    // If it looks like a cfg() expression, parse it
    if let Some(inner) = trimmed.strip_prefix("cfg(").and_then(|s| s.strip_suffix(')')) {
        return eval_cfg_inner(inner.trim(), target_triple);
    }

    // Otherwise, treat as a literal target triple match
    trimmed == target_triple
}

fn eval_cfg_inner(expr: &str, target: &str) -> bool {
    let expr = expr.trim();

    // all(...)
    if let Some(inner) = expr.strip_prefix("all(").and_then(|s| s.strip_suffix(')')) {
        return split_cfg_args(inner)
            .iter()
            .all(|arg| eval_cfg_inner(arg, target));
    }

    // any(...)
    if let Some(inner) = expr.strip_prefix("any(").and_then(|s| s.strip_suffix(')')) {
        return split_cfg_args(inner)
            .iter()
            .any(|arg| eval_cfg_inner(arg, target));
    }

    // not(...)
    if let Some(inner) = expr.strip_prefix("not(").and_then(|s| s.strip_suffix(')')) {
        return !eval_cfg_inner(inner.trim(), target);
    }

    // Shorthand: `unix` / `windows`
    if expr == "unix" {
        return target_family(target) == "unix";
    }
    if expr == "windows" {
        return target_family(target) == "windows";
    }

    // key = "value" form
    if let Some((key, value)) = parse_cfg_kv(expr) {
        return match key {
            "target_os" => target_os(target) == value,
            "target_arch" => target_arch(target) == value,
            "target_family" => target_family(target) == value,
            "target_env" => target_env(target) == value,
            _ => false,
        };
    }

    false
}

/// Split cfg arguments at the top level (respecting nested parentheses).
fn split_cfg_args(s: &str) -> Vec<&str> {
    let mut args = Vec::new();
    let mut depth = 0usize;
    let mut start = 0;
    for (i, c) in s.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                let arg = s[start..i].trim();
                if !arg.is_empty() {
                    args.push(arg);
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    let last = s[start..].trim();
    if !last.is_empty() {
        args.push(last);
    }
    args
}

/// Parse `key = "value"` from a cfg expression atom.
fn parse_cfg_kv(s: &str) -> Option<(&str, &str)> {
    let mut parts = s.splitn(2, '=');
    let key = parts.next()?.trim();
    let value = parts.next()?.trim().trim_matches('"');
    Some((key, value))
}

/// Extract the OS from a target triple (e.g., "linux" from "x86_64-unknown-linux-gnu").
fn target_os(triple: &str) -> &str {
    let parts: Vec<&str> = triple.split('-').collect();
    match parts.len() {
        3 => parts[2], // arch-vendor-os
        4 => parts[2], // arch-vendor-os-env
        _ => "",
    }
}

/// Extract the architecture from a target triple.
fn target_arch(triple: &str) -> &str {
    triple.split('-').next().unwrap_or("")
}

/// Determine the target family from a triple.
fn target_family(triple: &str) -> &str {
    let os = target_os(triple);
    match os {
        "linux" | "macos" | "darwin" | "freebsd" | "openbsd" | "netbsd" | "dragonfly" => "unix",
        "windows" => "windows",
        _ => {
            // Check for "apple" in the triple (e.g., "arm64-apple-darwin")
            if triple.contains("apple") || triple.contains("darwin") {
                "unix"
            } else {
                "unknown"
            }
        }
    }
}

/// Extract the environment/ABI from a target triple (e.g., "gnu" from "x86_64-unknown-linux-gnu").
fn target_env(triple: &str) -> &str {
    let parts: Vec<&str> = triple.split('-').collect();
    if parts.len() >= 4 {
        parts[3]
    } else {
        ""
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
            sysroot: None,
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
        metadata: None,
        security: None,
        publish: None,
        hooks: None,
        target: BTreeMap::new(),
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
            version: None,
            members: vec![],
            exclude: vec![],
            dependencies: BTreeMap::new(),
            resolver: Some("2".to_string()),
        }),
        cache: None,
        metadata: None,
        security: None,
        publish: None,
        hooks: None,
        target: BTreeMap::new(),
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
            default_features: true,
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

    #[test]
    fn test_parse_metadata_section() {
        let toml_str = r#"
[package]
name = "test"
version = "0.1.0"

[metadata]
category = "math"
keywords = ["linear-algebra", "simd"]
documentation = "https://docs.example.com"

[metadata.links]
homepage = "https://example.com"
issues = "https://example.com/issues"
"#;
        let manifest = Manifest::from_str(toml_str).unwrap();
        let meta = manifest.metadata.unwrap();
        assert_eq!(meta.category.as_deref(), Some("math"));
        assert_eq!(meta.keywords, vec!["linear-algebra", "simd"]);
        assert_eq!(meta.links.len(), 2);
        assert_eq!(meta.documentation.as_deref(), Some("https://docs.example.com"));
    }

    #[test]
    fn test_parse_security_section() {
        let toml_str = r#"
[package]
name = "test"
version = "0.1.0"

[security]
signing_key = "ABCD1234"
verify_checksums = true
trusted_sources = ["github.com/*", "gitlab.com/myorg/*"]
signature_policy = "require"
"#;
        let manifest = Manifest::from_str(toml_str).unwrap();
        let sec = manifest.security.unwrap();
        assert_eq!(sec.signing_key.as_deref(), Some("ABCD1234"));
        assert_eq!(sec.verify_checksums, Some(true));
        assert_eq!(sec.trusted_sources.len(), 2);
        assert_eq!(sec.signature_policy.as_deref(), Some("require"));
    }

    #[test]
    fn test_parse_publish_section() {
        let toml_str = r#"
[package]
name = "test"
version = "0.1.0"

[publish]
registry = "https://registry.example.com"
include = ["src/**", "cmod.toml", "LICENSE"]
exclude = ["tests/**", ".git"]
tags = ["v0.1.0", "latest"]
"#;
        let manifest = Manifest::from_str(toml_str).unwrap();
        let pub_config = manifest.publish.unwrap();
        assert_eq!(pub_config.registry.as_deref(), Some("https://registry.example.com"));
        assert_eq!(pub_config.include.len(), 3);
        assert_eq!(pub_config.exclude.len(), 2);
        assert_eq!(pub_config.tags, vec!["v0.1.0", "latest"]);
    }

    #[test]
    fn test_validate_valid_manifest() {
        let manifest = default_manifest("hello");
        assert!(manifest.validate().is_ok());
    }

    #[test]
    fn test_validate_empty_name() {
        let mut manifest = default_manifest("hello");
        manifest.package.name = String::new();
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn test_validate_invalid_name_chars() {
        let mut manifest = default_manifest("hello");
        manifest.package.name = "my lib!".to_string();
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn test_validate_invalid_version() {
        let mut manifest = default_manifest("hello");
        manifest.package.version = "not-semver".to_string();
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn test_validate_git_and_path_conflict() {
        let mut manifest = default_manifest("hello");
        manifest.dependencies.insert(
            "dep".to_string(),
            Dependency::Detailed(DetailedDependency {
                version: None,
                git: Some("https://github.com/test/dep".to_string()),
                branch: None,
                rev: None,
                tag: None,
                path: Some(PathBuf::from("./dep")),
                features: vec![],
                optional: false,
                default_features: true,
                workspace: false,
            }),
        );
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn test_validate_invalid_security_policy() {
        let mut manifest = default_manifest("hello");
        manifest.security = Some(Security {
            signing_key: None,
            verify_checksums: None,
            trusted_sources: vec![],
            signature_policy: Some("invalid_policy".to_string()),
        });
        assert!(manifest.validate().is_err());
    }

    #[test]
    fn test_parse_toolchain_with_sysroot() {
        let toml_str = r#"
[package]
name = "cross-proj"
version = "0.1.0"

[toolchain]
compiler = "clang"
target = "aarch64-unknown-linux-gnu"
sysroot = "/opt/aarch64-sysroot"
"#;
        let manifest = Manifest::from_str(toml_str).unwrap();
        let tc = manifest.toolchain.unwrap();
        assert_eq!(tc.target.as_deref(), Some("aarch64-unknown-linux-gnu"));
        assert_eq!(tc.sysroot.as_deref(), Some(Path::new("/opt/aarch64-sysroot")));
    }

    // --- cfg() evaluator tests ---

    #[test]
    fn test_eval_cfg_target_os() {
        assert!(eval_cfg(r#"cfg(target_os = "linux")"#, "x86_64-unknown-linux-gnu"));
        assert!(!eval_cfg(r#"cfg(target_os = "linux")"#, "x86_64-apple-darwin"));
        assert!(eval_cfg(r#"cfg(target_os = "darwin")"#, "arm64-apple-darwin"));
    }

    #[test]
    fn test_eval_cfg_target_arch() {
        assert!(eval_cfg(r#"cfg(target_arch = "x86_64")"#, "x86_64-unknown-linux-gnu"));
        assert!(!eval_cfg(r#"cfg(target_arch = "aarch64")"#, "x86_64-unknown-linux-gnu"));
        assert!(eval_cfg(r#"cfg(target_arch = "aarch64")"#, "aarch64-unknown-linux-gnu"));
    }

    #[test]
    fn test_eval_cfg_family() {
        assert!(eval_cfg(r#"cfg(target_family = "unix")"#, "x86_64-unknown-linux-gnu"));
        assert!(eval_cfg(r#"cfg(target_family = "unix")"#, "arm64-apple-darwin"));
        assert!(eval_cfg(r#"cfg(target_family = "windows")"#, "x86_64-pc-windows-msvc"));
        assert!(!eval_cfg(r#"cfg(target_family = "unix")"#, "x86_64-pc-windows-msvc"));
    }

    #[test]
    fn test_eval_cfg_shorthand() {
        assert!(eval_cfg("cfg(unix)", "x86_64-unknown-linux-gnu"));
        assert!(!eval_cfg("cfg(unix)", "x86_64-pc-windows-msvc"));
        assert!(eval_cfg("cfg(windows)", "x86_64-pc-windows-msvc"));
        assert!(!eval_cfg("cfg(windows)", "x86_64-unknown-linux-gnu"));
    }

    #[test]
    fn test_eval_cfg_all() {
        assert!(eval_cfg(
            r#"cfg(all(target_os = "linux", target_arch = "x86_64"))"#,
            "x86_64-unknown-linux-gnu"
        ));
        assert!(!eval_cfg(
            r#"cfg(all(target_os = "linux", target_arch = "aarch64"))"#,
            "x86_64-unknown-linux-gnu"
        ));
    }

    #[test]
    fn test_eval_cfg_any() {
        assert!(eval_cfg(
            r#"cfg(any(target_os = "linux", target_os = "macos"))"#,
            "x86_64-unknown-linux-gnu"
        ));
        assert!(!eval_cfg(
            r#"cfg(any(target_os = "macos", target_os = "windows"))"#,
            "x86_64-unknown-linux-gnu"
        ));
    }

    #[test]
    fn test_eval_cfg_not() {
        assert!(eval_cfg(r#"cfg(not(target_os = "windows"))"#, "x86_64-unknown-linux-gnu"));
        assert!(!eval_cfg(r#"cfg(not(target_os = "linux"))"#, "x86_64-unknown-linux-gnu"));
    }

    #[test]
    fn test_eval_cfg_literal_triple() {
        assert!(eval_cfg("x86_64-unknown-linux-gnu", "x86_64-unknown-linux-gnu"));
        assert!(!eval_cfg("aarch64-unknown-linux-gnu", "x86_64-unknown-linux-gnu"));
    }

    #[test]
    fn test_effective_dependencies() {
        let toml_str = r#"
[package]
name = "mylib"
version = "0.1.0"

[dependencies]
common = "^1.0"

[target.'cfg(target_os = "linux")'.dependencies]
linux-only = "^2.0"

[target.'cfg(windows)'.dependencies]
win-only = "^3.0"
"#;
        let manifest = Manifest::from_str(toml_str).unwrap();

        let linux_deps = manifest.effective_dependencies("x86_64-unknown-linux-gnu");
        assert!(linux_deps.contains_key("common"));
        assert!(linux_deps.contains_key("linux-only"));
        assert!(!linux_deps.contains_key("win-only"));

        let win_deps = manifest.effective_dependencies("x86_64-pc-windows-msvc");
        assert!(win_deps.contains_key("common"));
        assert!(!win_deps.contains_key("linux-only"));
        assert!(win_deps.contains_key("win-only"));
    }

    #[test]
    fn test_target_env() {
        assert!(eval_cfg(r#"cfg(target_env = "gnu")"#, "x86_64-unknown-linux-gnu"));
        assert!(eval_cfg(r#"cfg(target_env = "msvc")"#, "x86_64-pc-windows-msvc"));
        assert!(!eval_cfg(r#"cfg(target_env = "musl")"#, "x86_64-unknown-linux-gnu"));
    }
}
