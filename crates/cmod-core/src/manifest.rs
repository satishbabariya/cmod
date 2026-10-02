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
    pub format: Option<Format>,

    #[serde(default)]
    pub lint: Option<Lint>,

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

    /// IDE integration settings.
    #[serde(default)]
    pub ide: Option<Ide>,

    /// Plugin configuration.
    #[serde(default)]
    pub plugins: Option<BTreeMap<String, PluginEntry>>,

    /// ABI compatibility metadata for BMI distribution.
    #[serde(default)]
    pub abi: Option<AbiConfig>,

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
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Dependency {
    /// Simple version string: `"^1.2"`
    Simple(String),
    /// Expanded dependency specification.
    Detailed(DetailedDependency),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
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

    /// Extract the local path, if this is a path dependency.
    pub fn path(&self) -> Option<&std::path::Path> {
        match self {
            Dependency::Detailed(d) => d.path.as_deref(),
            _ => None,
        }
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
    /// Additional include directories (relative to project root).
    #[serde(default)]
    pub include_dirs: Vec<String>,
    /// Extra compiler flags.
    #[serde(default)]
    pub extra_flags: Vec<String>,
    /// Source directories (relative to project root). Defaults to `["src"]`.
    #[serde(default)]
    pub sources: Vec<String>,
    /// Glob patterns for files to exclude from source discovery.
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Distributed build configuration.
    #[serde(default)]
    pub distributed: Option<DistributedBuildConfig>,
}

/// Configuration for distributed builds in cmod.toml `[build.distributed]`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DistributedBuildConfig {
    /// Whether distributed builds are enabled.
    #[serde(default)]
    pub enabled: bool,
    /// Worker endpoint URLs.
    #[serde(default)]
    pub workers: Vec<String>,
    /// Scheduling strategy: "least_loaded", "round_robin", "target_affinity".
    #[serde(default)]
    pub scheduler: Option<String>,
    /// Environment variable name containing the authentication token for worker
    /// communication (e.g., `"CMOD_DISTRIBUTED_AUTH_TOKEN"`).  The actual secret
    /// is read from the environment at runtime — never store tokens in the manifest.
    #[serde(default)]
    pub auth_token_env: Option<String>,
    /// Timeout for individual compilation tasks (seconds).
    #[serde(default)]
    pub task_timeout: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Test {
    #[serde(default)]
    pub framework: Option<String>,
    #[serde(default)]
    pub test_patterns: Vec<String>,
    #[serde(default)]
    pub exclude_patterns: Vec<String>,
    /// Custom test runner command.
    #[serde(default)]
    pub runner: Option<String>,
    /// Additional compiler flags for test compilation.
    #[serde(default)]
    pub extra_flags: Vec<String>,
    /// Default per-test timeout in seconds (overridden by CLI --timeout).
    #[serde(default)]
    pub timeout: Option<u64>,
}

/// Formatting configuration (`[format]` section in cmod.toml).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Format {
    /// Additional directories to format (merged with src_dirs).
    #[serde(default)]
    pub include_dirs: Vec<String>,
    /// Glob patterns to exclude from formatting.
    #[serde(default)]
    pub exclude: Vec<String>,
}

/// Linting configuration (`[lint]` section in cmod.toml).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Lint {
    /// Additional directories to lint (merged with src_dirs).
    #[serde(default)]
    pub include_dirs: Vec<String>,
    /// Glob patterns to exclude from linting.
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Maximum line length for the "line too long" rule. Default: 120.
    #[serde(default)]
    pub max_line_length: Option<usize>,
    /// Enable clang-tidy integration.
    #[serde(default)]
    pub clang_tidy: Option<bool>,
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
    /// Dependency overrides for development (replace git deps with local paths).
    ///
    /// ```toml
    /// [workspace.patch]
    /// fmt = { path = "../my-local-fmt" }
    /// ```
    #[serde(default)]
    pub patch: BTreeMap<String, Dependency>,
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
    /// Environment variable name containing the bearer token for remote cache
    /// authentication (e.g., `"CMOD_CACHE_AUTH_TOKEN"`).  The actual secret is
    /// read from the environment at runtime.
    #[serde(default)]
    pub auth_token_env: Option<String>,
    /// HTTP timeout in seconds for remote cache operations (default: 30).
    #[serde(default)]
    pub timeout: Option<u64>,
    /// Number of retry attempts for remote cache operations (default: 3).
    #[serde(default)]
    pub retries: Option<u32>,
    /// Whether to compress artifacts with zstd before uploading (default: true).
    #[serde(default)]
    pub compression: Option<bool>,
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
    #[serde(default)]
    pub readme: Option<String>,
}

/// IDE integration configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Ide {
    /// LSP server mode: "auto", "on", "off".
    #[serde(default)]
    pub lsp_server: Option<String>,
    /// Enable code completion integration.
    #[serde(default)]
    pub code_completion: Option<bool>,
    /// Enable real-time diagnostics.
    #[serde(default)]
    pub diagnostics: Option<bool>,
    /// Enable format-on-save.
    #[serde(default)]
    pub format_on_save: Option<bool>,
}

/// Plugin entry in the `[plugins]` section.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginEntry {
    /// Path to the plugin directory or executable.
    #[serde(default)]
    pub path: Option<PathBuf>,
    /// Plugin capabilities.
    #[serde(default)]
    pub capabilities: Vec<String>,
}

/// ABI compatibility configuration for BMI distribution and version tracking.
///
/// Used to track ABI compatibility between modules and detect breaking
/// changes during dependency resolution.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AbiConfig {
    /// ABI version string for this module (e.g., "1.0").
    /// Bumped when ABI-breaking changes are made.
    #[serde(default)]
    pub version: Option<String>,
    /// ABI variant: "itanium" or "msvc".
    #[serde(default)]
    pub variant: Option<Abi>,
    /// Whether this module provides a stable ABI guarantee.
    #[serde(default)]
    pub stable: bool,
    /// Minimum required C++ standard for ABI compatibility (e.g., "20").
    #[serde(default)]
    pub min_cpp_standard: Option<String>,
    /// Platforms with verified ABI compatibility.
    #[serde(default)]
    pub verified_platforms: Vec<String>,
    /// Notes on ABI-breaking changes (changelog-like).
    #[serde(default)]
    pub breaking_changes: Vec<String>,
}

/// Security configuration for the project.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Security {
    /// Signing configuration (GPG key ID, SSH key path, etc.).
    #[serde(default)]
    pub signing_key: Option<String>,
    /// Signing backend: "pgp", "ssh", "sigstore".
    #[serde(default)]
    pub signing_backend: Option<String>,
    /// Whether to verify content hashes on dependency fetch.
    #[serde(default)]
    pub verify_checksums: Option<bool>,
    /// Trusted source URLs/patterns.
    #[serde(default)]
    pub trusted_sources: Vec<String>,
    /// Required signature policy: "none", "warn", "require".
    #[serde(default)]
    pub signature_policy: Option<String>,
    /// OIDC issuer for Sigstore keyless signing.
    #[serde(default)]
    pub oidc_issuer: Option<String>,
    /// Certificate identity (email) for Sigstore verification.
    #[serde(default)]
    pub certificate_identity: Option<String>,
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
    /// Command to run after testing completes.
    #[serde(default, rename = "post-test")]
    pub post_test: Option<String>,
    /// Command to run before dependency resolution.
    #[serde(default, rename = "pre-resolve")]
    pub pre_resolve: Option<String>,
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
    #[allow(clippy::should_implement_trait)]
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
    ///
    /// This writes the whole manifest anew: to change part of a manifest
    /// someone wrote, use [`Manifest::save_dependencies`] or
    /// [`edit_toml_file`], which keep the rest as written.
    pub fn save(&self, path: &Path) -> Result<(), CmodError> {
        let content = self.to_toml_string()?;
        std::fs::write(path, content)?;
        Ok(())
    }

    /// Write this manifest's `[dependencies]` to the manifest at `path`,
    /// leaving the rest of the file as written (see
    /// [`sync_dependency_table`]).
    pub fn save_dependencies(&self, path: &Path) -> Result<(), CmodError> {
        edit_toml_file(path, |doc| {
            sync_dependency_table(doc, &["dependencies"], &self.dependencies)
        })
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
        if !self
            .package
            .name
            .chars()
            .all(|c| c.is_alphanumeric() || c == '_' || c == '-')
        {
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
                        reason: format!("dependency '{}' specifies both `git` and `path`", name),
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
    ///
    /// Optimized to avoid unnecessary cloning when there are no target-specific deps.
    pub fn effective_dependencies(&self, target_triple: &str) -> BTreeMap<String, Dependency> {
        // Fast path: if no target-specific deps, return a reference-based view
        // or clone only once at the end
        if self.target.is_empty() {
            return self.dependencies.clone();
        }

        // Check if any target configs match before cloning
        let matching_targets: Vec<_> = self
            .target
            .iter()
            .filter(|(cfg_expr, _)| eval_cfg(cfg_expr, target_triple))
            .collect();

        if matching_targets.is_empty() {
            // No matching targets, just return base deps
            return self.dependencies.clone();
        }

        // Only clone if we actually need to merge
        let mut deps = self.dependencies.clone();

        for (_, spec) in matching_targets {
            for (name, dep) in &spec.dependencies {
                deps.entry(name.clone()).or_insert_with(|| dep.clone());
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
        // Keys are canonically bare (`github.com/owner/repo`), but tolerate
        // hand-edited manifests that already carry a scheme.
        let to_url = |key: &str| {
            if key.contains("://") {
                key.to_string()
            } else {
                format!("https://{}", key)
            }
        };
        match dep {
            Dependency::Simple(_) => to_url(key),
            Dependency::Detailed(d) => {
                if let Some(git) = &d.git {
                    git.clone()
                } else {
                    to_url(key)
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
    if let Some(inner) = trimmed
        .strip_prefix("cfg(")
        .and_then(|s| s.strip_suffix(')'))
    {
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
            name: format!("local.{}", name.replace('-', "_")),
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
            include_dirs: Vec::new(),
            extra_flags: Vec::new(),
            sources: Vec::new(),
            exclude: Vec::new(),
            distributed: None,
        }),
        test: None,
        format: None,
        lint: None,
        workspace: None,
        cache: None,
        metadata: None,
        security: None,
        publish: None,
        hooks: None,
        ide: None,
        plugins: None,
        abi: None,
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
        format: None,
        lint: None,
        workspace: Some(Workspace {
            name: Some(name.to_string()),
            version: None,
            members: vec![],
            exclude: vec![],
            dependencies: BTreeMap::new(),
            resolver: Some("2".to_string()),
            patch: BTreeMap::new(),
        }),
        cache: None,
        metadata: None,
        security: None,
        publish: None,
        hooks: None,
        ide: None,
        plugins: None,
        abi: None,
        target: BTreeMap::new(),
    }
}

/// Make the string array `key` of `table` hold `list`: entries no longer
/// listed are dropped and new ones appended, so those kept keep their
/// place and comments. An absent array is only created to hold entries.
pub fn sync_string_array(table: &mut dyn toml_edit::TableLike, key: &str, list: &[String]) {
    match table.get_mut(key).and_then(|item| item.as_array_mut()) {
        Some(array) => {
            let mut i = 0;
            while i < array.len() {
                let listed = array
                    .get(i)
                    .and_then(|v| v.as_str())
                    .is_some_and(|entry| list.iter().any(|l| l == entry));
                if listed {
                    i += 1;
                } else {
                    remove_entry(array, i);
                }
            }
            for entry in list {
                if !array.iter().any(|v| v.as_str() == Some(entry)) {
                    push_entry(array, entry);
                }
            }
        }
        None if list.is_empty() => {}
        None => {
            let array: toml_edit::Array = list.iter().map(String::as_str).collect();
            table.insert(key, toml_edit::value(array));
        }
    }
}

/// The text before a value in an array: the rest of the line of the
/// entry before it (its comment), then the lines leading to this one.
fn value_prefix(value: &toml_edit::Value) -> String {
    value
        .decor()
        .prefix()
        .and_then(|p| p.as_str())
        .unwrap_or("")
        .to_string()
}

/// `text` split at its first line break: the end of a line, and the lines
/// after it (`""` when there is none).
fn split_line_end(text: &str) -> (&str, &str) {
    match text.find('\n') {
        Some(at) => text.split_at(at),
        None => (text, ""),
    }
}

/// Give what follows the last entry of `array` (without a trailing comma,
/// the last value holds it) to the array, which prints the same.
fn take_last_suffix(array: &mut toml_edit::Array) {
    if array.trailing_comma() || array.is_empty() {
        return;
    }
    let last = array.len() - 1;
    if let Some(value) = array.get_mut(last) {
        let suffix = value
            .decor()
            .suffix()
            .and_then(|s| s.as_str())
            .unwrap_or("")
            .to_string();
        value.decor_mut().set_suffix("");
        let trailing = array.trailing().as_str().unwrap_or("").to_string();
        array.set_trailing(suffix + &trailing);
    }
}

/// Append `entry` to `array`; in an array written one entry per line, on
/// a line of its own, indented as the first, after any comment ending the
/// last.
fn push_entry(array: &mut toml_edit::Array, entry: &str) {
    if !array.iter().any(|v| value_prefix(v).contains('\n')) {
        array.push(entry);
        return;
    }
    let indent = array
        .get(0)
        .map(value_prefix)
        .and_then(|p| p.rsplit_once('\n').map(|(_, indent)| indent.to_string()))
        .unwrap_or_default();
    take_last_suffix(array);
    let trailing = array.trailing().as_str().unwrap_or("").to_string();
    let (before, after) = trailing
        .rsplit_once('\n')
        .unwrap_or((trailing.as_str(), ""));
    let mut value = toml_edit::Value::from(entry);
    value
        .decor_mut()
        .set_prefix(format!("{}\n{}", before, indent));
    let after = format!("\n{}", after);
    array.push_formatted(value);
    array.set_trailing_comma(true);
    array.set_trailing(after);
}

/// Remove entry `index` of `array` with its line: the comment ending its
/// line and those above it go, the comment ending the line before stays.
fn remove_entry(array: &mut toml_edit::Array, index: usize) {
    take_last_suffix(array);
    let Some(removed) = array.get(index).map(value_prefix) else {
        return;
    };
    // The end of the line before it, kept; on one line, the space before it.
    let (kept, _) = split_line_end(&removed);
    let kept = kept.to_string();
    let multiline = removed.contains('\n');
    if index + 1 < array.len() {
        if let Some(next) = array.get_mut(index + 1) {
            let prefix = value_prefix(next);
            let (_, lines) = split_line_end(&prefix);
            let lines = lines.to_string();
            next.decor_mut().set_prefix(kept + &lines);
        }
    } else if multiline {
        let trailing = array.trailing().as_str().unwrap_or("").to_string();
        let (_, lines) = split_line_end(&trailing);
        let lines = lines.to_string();
        array.set_trailing(kept + &lines);
    }
    array.remove(index);
}

/// Edit the TOML file at `path` in place: parse it, let `edit` change the
/// document, and write it back with everything `edit` left alone as it
/// was. Lines end as most of the file's did; the file is replaced at once,
/// so it is never left half written.
pub fn edit_toml_file(
    path: &Path,
    edit: impl FnOnce(&mut toml_edit::DocumentMut) -> Result<(), CmodError>,
) -> Result<(), CmodError> {
    let text = std::fs::read_to_string(path)?;
    let mut doc: toml_edit::DocumentMut =
        text.parse()
            .map_err(|e: toml_edit::TomlError| CmodError::InvalidManifest {
                reason: format!("{}: {}", path.display(), e),
            })?;
    edit(&mut doc).map_err(|e| match e {
        CmodError::InvalidManifest { reason } => CmodError::InvalidManifest {
            reason: format!("{}: {}", path.display(), reason),
        },
        e => e,
    })?;
    // toml_edit writes `\n`.
    let mut out = doc.to_string();
    let crlf = text.matches("\r\n").count();
    if crlf * 2 > text.matches('\n').count() {
        out = out.replace("\r\n", "\n").replace('\n', "\r\n");
    }
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".cmod-tmp");
    let tmp = PathBuf::from(tmp);
    std::fs::write(&tmp, out)?;
    std::fs::rename(&tmp, path).inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })?;
    Ok(())
}

/// Make the dependency table at `table_path` of `doc` (`["dependencies"]`)
/// hold `deps`, leaving the rest of the document as written:
///
/// - a dependency no longer listed is removed with its line and the
///   comments right above it; comments above a blank line before it stay,
///   with the line after it;
/// - a changed dependency is rewritten in place, keeping its comments,
///   and a table (`[dependencies.fmt]`) stays a table;
/// - a new one is written as a version string, or an inline table of the
///   fields that differ from their defaults;
/// - an unchanged one is not touched.
///
/// The table is only created to hold dependencies.
pub fn sync_dependency_table(
    doc: &mut toml_edit::DocumentMut,
    table_path: &[&str],
    deps: &BTreeMap<String, Dependency>,
) -> Result<(), CmodError> {
    let not_a_table = || CmodError::InvalidManifest {
        reason: format!("[{}] is not a table", table_path.join(".")),
    };
    let mut item = doc.as_item_mut();
    for (depth, key) in table_path.iter().enumerate() {
        let table = item.as_table_like_mut().ok_or_else(not_a_table)?;
        if table.get(key).is_none() {
            if deps.is_empty() {
                return Ok(());
            }
            let mut new = toml_edit::Table::new();
            new.set_implicit(depth + 1 < table_path.len());
            table.insert(key, toml_edit::Item::Table(new));
        }
        item = table.get_mut(key).ok_or_else(not_a_table)?;
    }
    let table = item.as_table_like_mut().ok_or_else(not_a_table)?;

    let gone: Vec<String> = table
        .iter()
        .map(|(key, _)| key.to_string())
        .filter(|key| !deps.contains_key(key))
        .collect();
    for key in gone {
        remove_dependency_entry(table, &key);
    }
    for (key, dep) in deps {
        let value = dependency_value(dep)?;
        match table.get_mut(key) {
            Some(item) if parse_dependency_item(item).as_ref() == Some(dep) => {}
            Some(item) => replace_dependency_item(item, value),
            None => {
                table.insert(key, toml_edit::Item::Value(value));
            }
        }
    }
    Ok(())
}

/// The dependency `item` declares, if it declares one.
fn parse_dependency_item(item: &toml_edit::Item) -> Option<Dependency> {
    #[derive(Deserialize)]
    struct Entry {
        d: Dependency,
    }
    let mut value = item.clone().into_value().ok()?;
    value.decor_mut().clear();
    if let toml_edit::Value::InlineTable(table) = &mut value {
        table.fmt();
    }
    toml::from_str::<Entry>(&format!("d = {}", value))
        .ok()
        .map(|e| e.d)
}

/// Make `item` declare what `value` does, keeping its comments: a table
/// (`[dependencies.fmt]`) keeps its form and the fields that stay, with
/// theirs; a value (inline tables hold no comments) keeps the comment
/// after it.
fn replace_dependency_item(item: &mut toml_edit::Item, value: toml_edit::Value) {
    if let (Some(table), toml_edit::Value::InlineTable(fields)) = (item.as_table_mut(), &value) {
        let gone: Vec<String> = table
            .iter()
            .map(|(key, _)| key.to_string())
            .filter(|key| !fields.contains_key(key))
            .collect();
        for key in gone {
            table.remove(&key);
        }
        for (key, field) in fields.iter() {
            let mut field = field.clone();
            match table.get_mut(key).and_then(|i| i.as_value_mut()) {
                Some(old) => {
                    *field.decor_mut() = old.decor().clone();
                    *old = field;
                }
                None => {
                    field.decor_mut().clear();
                    table.insert(key, toml_edit::Item::Value(field));
                }
            }
        }
        return;
    }
    let mut value = value;
    if let Some(old) = item.as_value() {
        *value.decor_mut() = old.decor().clone();
    }
    *item = toml_edit::Item::Value(value);
}

/// Remove dependency `key` of `table` with its line and the comments right
/// above it. What is above a blank line before it is about the lines that
/// follow, so it moves to the next entry.
fn remove_dependency_entry(table: &mut dyn toml_edit::TableLike, key: &str) {
    let prefix = table
        .key(key)
        .and_then(|k| k.leaf_decor().prefix())
        .and_then(|p| p.as_str())
        .unwrap_or("")
        .to_string();
    let lines: Vec<&str> = prefix.split_inclusive('\n').collect();
    let section: String = match lines
        .iter()
        .rposition(|l| l.trim().is_empty() && l.ends_with('\n'))
    {
        Some(blank) => lines[..=blank].concat(),
        None => String::new(),
    };
    let keys: Vec<String> = table.iter().map(|(k, _)| k.to_string()).collect();
    let next = keys
        .iter()
        .position(|k| k == key)
        .and_then(|at| keys.get(at + 1))
        .cloned();
    table.remove(key);
    if section.is_empty() {
        return;
    }
    if let Some(mut next_key) = next.as_deref().and_then(|n| table.key_mut(n)) {
        let old = next_key
            .leaf_decor()
            .prefix()
            .and_then(|p| p.as_str())
            .unwrap_or("")
            .to_string();
        next_key.leaf_decor_mut().set_prefix(section + &old);
    }
}

/// `dep` as it is written in `[dependencies]`: a version string, or an
/// inline table of the fields that differ from their defaults.
fn dependency_value(dep: &Dependency) -> Result<toml_edit::Value, CmodError> {
    let detailed = match dep {
        Dependency::Simple(version) => return Ok(version.as_str().into()),
        Dependency::Detailed(detailed) => detailed,
    };
    let mut table = toml_edit::InlineTable::new();
    let strings = [
        ("version", detailed.version.as_deref()),
        ("git", detailed.git.as_deref()),
        ("branch", detailed.branch.as_deref()),
        ("rev", detailed.rev.as_deref()),
        ("tag", detailed.tag.as_deref()),
    ];
    for (key, value) in strings {
        if let Some(value) = value {
            table.insert(key, value.into());
        }
    }
    if let Some(path) = &detailed.path {
        let path = path.to_str().ok_or_else(|| CmodError::InvalidManifest {
            reason: format!("dependency path {} is not UTF-8", path.display()),
        })?;
        table.insert("path", path.into());
    }
    if !detailed.features.is_empty() {
        let features: toml_edit::Array = detailed.features.iter().map(String::as_str).collect();
        table.insert("features", features.into());
    }
    if detailed.optional {
        table.insert("optional", true.into());
    }
    if !detailed.default_features {
        table.insert("default_features", false.into());
    }
    if detailed.workspace {
        table.insert("workspace", true.into());
    }
    table.fmt();
    Ok(table.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_push_entry_follows_the_array_layout() {
        let cases = [
            ("a = [\"x\"]", "a = [\"x\", \"y\"]"),
            ("a = []", "a = [\"y\"]"),
            ("a = [\n  \"x\"\n]", "a = [\n  \"x\",\n  \"y\",\n]"),
            (
                "a = [\n  \"x\", # x\n  # end\n]",
                "a = [\n  \"x\", # x\n  # end\n  \"y\",\n]",
            ),
        ];
        for (before, after) in cases {
            let mut doc: toml_edit::DocumentMut = before.parse().unwrap();
            push_entry(doc["a"].as_array_mut().unwrap(), "y");
            assert_eq!(doc.to_string().trim_end(), after, "from {:?}", before);
        }
    }

    #[test]
    fn test_remove_entry_takes_its_line_and_comment() {
        let cases = [
            ("a = [\"x\", \"y\"]", 0, "a = [\"y\"]"),
            ("a = [\"x\", \"y\"]", 1, "a = [\"x\"]"),
            ("a = [\"x\", \"y\", \"z\"]", 1, "a = [\"x\", \"z\"]"),
            (
                "a = [\n  \"x\", # X\n  \"y\", # Y\n  \"z\", # Z\n]",
                1,
                "a = [\n  \"x\", # X\n  \"z\", # Z\n]",
            ),
            (
                "a = [\n  \"x\", # X\n  \"y\", # Y\n]",
                0,
                "a = [\n  \"y\", # Y\n]",
            ),
            (
                "a = [\n  \"x\", # X\n  # about y\n  \"y\", # Y\n]",
                1,
                "a = [\n  \"x\", # X\n]",
            ),
            ("a = [\n  \"x\",\n  \"y\"\n]", 1, "a = [\n  \"x\"\n]"),
        ];
        for (before, index, after) in cases {
            let mut doc: toml_edit::DocumentMut = before.parse().unwrap();
            remove_entry(doc["a"].as_array_mut().unwrap(), index);
            assert_eq!(doc.to_string().trim_end(), after, "from {:?}", before);
        }
    }

    #[test]
    fn test_save_dependencies_keeps_the_manifest_as_written() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("cmod.toml");
        let written = "# My package.\n[package]\nname = \"p\"\nversion = \"0.1.0\"\n\n\
                       [dependencies]\n# Formatting.\n\"github.com/fmtlib/fmt\" = \"^10\" # pinned\n\
                       old = { path = \"../old\" }\n\n[build]\ntype = \"binary\"\n";
        std::fs::write(&path, written).unwrap();

        let mut manifest = Manifest::load(&path).unwrap();
        manifest.dependencies.remove("old");
        manifest.dependencies.insert(
            "util".to_string(),
            Dependency::Detailed(DetailedDependency {
                version: None,
                git: None,
                branch: None,
                rev: None,
                tag: None,
                path: Some(PathBuf::from("../util")),
                features: vec!["fast".to_string()],
                optional: false,
                default_features: true,
                workspace: false,
            }),
        );
        manifest.save_dependencies(&path).unwrap();

        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "# My package.\n[package]\nname = \"p\"\nversion = \"0.1.0\"\n\n\
             [dependencies]\n# Formatting.\n\"github.com/fmtlib/fmt\" = \"^10\" # pinned\n\
             util = { path = \"../util\", features = [\"fast\"] }\n\n[build]\ntype = \"binary\"\n"
        );
        let reloaded = Manifest::load(&path).unwrap();
        assert_eq!(reloaded.dependencies.len(), 2);
        assert!(reloaded.dependencies["util"].is_path());

        // A changed dependency is rewritten; the manifest gains no table.
        let mut manifest = reloaded;
        manifest.dependencies.insert(
            "github.com/fmtlib/fmt".to_string(),
            Dependency::Simple("^11".to_string()),
        );
        manifest.save_dependencies(&path).unwrap();
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(
            text.contains("\"github.com/fmtlib/fmt\" = \"^11\""),
            "{}",
            text
        );
        assert!(!text.contains("[features]"), "{}", text);
    }

    /// `deps` synced into the `[dependencies]` of `before`.
    fn synced(before: &str, deps: &[(&str, Dependency)]) -> String {
        let mut doc: toml_edit::DocumentMut = before.parse().unwrap();
        let deps: BTreeMap<String, Dependency> = deps
            .iter()
            .map(|(k, d)| (k.to_string(), d.clone()))
            .collect();
        sync_dependency_table(&mut doc, &["dependencies"], &deps).unwrap();
        doc.to_string()
    }

    fn simple(version: &str) -> Dependency {
        Dependency::Simple(version.to_string())
    }

    #[test]
    fn test_a_changed_dependency_keeps_its_comments() {
        assert_eq!(
            synced(
                "[dependencies]\n# Formatting lib\nfmt = \"^10\" # pinned for ABI\n",
                &[("fmt", simple("^11"))]
            ),
            "[dependencies]\n# Formatting lib\nfmt = \"^11\" # pinned for ABI\n"
        );
        // A table stays one, with the fields that stay.
        let detailed = Dependency::Detailed(DetailedDependency {
            version: Some("^11".to_string()),
            git: Some("https://example.com/fmt".to_string()),
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
            synced(
                "[dependencies.fmt]\n# Where from.\ngit = \"https://example.com/fmt\"\nversion = \"^10\" # was\noptional = true\n",
                &[("fmt", detailed)]
            ),
            "[dependencies.fmt]\n# Where from.\ngit = \"https://example.com/fmt\"\nversion = \"^11\" # was\n"
        );
    }

    #[test]
    fn test_a_removed_dependency_leaves_the_section_comment() {
        let before = "[dependencies]\n# Runtime dependencies, keep sorted\n\n# The formatter.\n\
                      fmt = \"^10\"\nb = \"^2\"\n";
        assert_eq!(
            synced(before, &[("b", simple("^2"))]),
            "[dependencies]\n# Runtime dependencies, keep sorted\n\nb = \"^2\"\n"
        );
        // Without a blank line, the comment is the dependency's.
        assert_eq!(
            synced(
                "[dependencies]\n# The formatter.\nfmt = \"^10\"\nb = \"^2\"\n",
                &[("b", simple("^2"))]
            ),
            "[dependencies]\nb = \"^2\"\n"
        );
        // A table goes with its header.
        assert_eq!(
            synced(
                "[package]\nname = \"p\"\n\n[dependencies.fmt]\nversion = \"^10\"\n\n[dependencies.b]\nversion = \"^2\"\n",
                &[("b", Dependency::Detailed(DetailedDependency {
                    version: Some("^2".to_string()),
                    git: None,
                    branch: None,
                    rev: None,
                    tag: None,
                    path: None,
                    features: vec![],
                    optional: false,
                    default_features: true,
                    workspace: false,
                }))]
            ),
            "[package]\nname = \"p\"\n\n[dependencies.b]\nversion = \"^2\"\n"
        );
    }

    #[test]
    fn test_no_dependencies_add_no_table() {
        let before = "[package]\nname = \"p\"\n";
        assert_eq!(synced(before, &[]), before);
    }

    #[test]
    fn test_edit_toml_file_keeps_the_usual_line_ending() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("cmod.toml");
        // Mostly LF, with one CRLF line: written with LF throughout.
        std::fs::write(&path, "a = 1\nb = 2\r\nc = 3\n").unwrap();
        edit_toml_file(&path, |doc| {
            doc["d"] = toml_edit::value(4);
            Ok(())
        })
        .unwrap();
        let out = std::fs::read_to_string(&path).unwrap();
        assert!(out.starts_with("a = 1\n"), "{:?}", out);
        assert!(out.ends_with("d = 4\n"), "{:?}", out);
        assert!(!tmp.path().join("cmod.toml.cmod-tmp").exists());
    }

    #[test]
    fn test_save_dependencies_adds_the_table_when_missing() {
        let tmp = tempfile::TempDir::new().unwrap();
        let path = tmp.path().join("cmod.toml");
        std::fs::write(
            &path,
            "[package]\r\nname = \"p\"\r\nversion = \"0.1.0\"\r\n",
        )
        .unwrap();
        let mut manifest = Manifest::load(&path).unwrap();
        manifest
            .dependencies
            .insert("a".to_string(), Dependency::Simple("^1".to_string()));
        manifest.save_dependencies(&path).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "[package]\r\nname = \"p\"\r\nversion = \"0.1.0\"\r\n\r\n[dependencies]\r\na = \"^1\"\r\n"
        );
    }

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
        assert_eq!(
            manifest.module.as_ref().unwrap().name,
            "github.user.my_math"
        );
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
        assert_eq!(
            meta.documentation.as_deref(),
            Some("https://docs.example.com")
        );
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
        assert_eq!(
            pub_config.registry.as_deref(),
            Some("https://registry.example.com")
        );
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
            signing_backend: None,
            verify_checksums: None,
            trusted_sources: vec![],
            signature_policy: Some("invalid_policy".to_string()),
            oidc_issuer: None,
            certificate_identity: None,
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
        assert_eq!(
            tc.sysroot.as_deref(),
            Some(Path::new("/opt/aarch64-sysroot"))
        );
    }

    #[test]
    fn test_parse_build_sources_and_exclude() {
        let toml_str = r#"
[package]
name = "jolt"
version = "0.1.0"

[build]
type = "static-lib"
sources = ["Jolt/", "extra/src/"]
exclude = ["*_test.cc", "test/**"]
"#;
        let manifest = Manifest::from_str(toml_str).unwrap();
        let build = manifest.build.unwrap();
        assert_eq!(build.sources, vec!["Jolt/", "extra/src/"]);
        assert_eq!(build.exclude, vec!["*_test.cc", "test/**"]);
    }

    #[test]
    fn test_parse_build_sources_defaults_empty() {
        let toml_str = r#"
[package]
name = "simple"
version = "0.1.0"

[build]
type = "binary"
"#;
        let manifest = Manifest::from_str(toml_str).unwrap();
        let build = manifest.build.unwrap();
        assert!(build.sources.is_empty());
        assert!(build.exclude.is_empty());
    }

    // --- cfg() evaluator tests ---

    #[test]
    fn test_eval_cfg_target_os() {
        assert!(eval_cfg(
            r#"cfg(target_os = "linux")"#,
            "x86_64-unknown-linux-gnu"
        ));
        assert!(!eval_cfg(
            r#"cfg(target_os = "linux")"#,
            "x86_64-apple-darwin"
        ));
        assert!(eval_cfg(
            r#"cfg(target_os = "darwin")"#,
            "arm64-apple-darwin"
        ));
    }

    #[test]
    fn test_eval_cfg_target_arch() {
        assert!(eval_cfg(
            r#"cfg(target_arch = "x86_64")"#,
            "x86_64-unknown-linux-gnu"
        ));
        assert!(!eval_cfg(
            r#"cfg(target_arch = "aarch64")"#,
            "x86_64-unknown-linux-gnu"
        ));
        assert!(eval_cfg(
            r#"cfg(target_arch = "aarch64")"#,
            "aarch64-unknown-linux-gnu"
        ));
    }

    #[test]
    fn test_eval_cfg_family() {
        assert!(eval_cfg(
            r#"cfg(target_family = "unix")"#,
            "x86_64-unknown-linux-gnu"
        ));
        assert!(eval_cfg(
            r#"cfg(target_family = "unix")"#,
            "arm64-apple-darwin"
        ));
        assert!(eval_cfg(
            r#"cfg(target_family = "windows")"#,
            "x86_64-pc-windows-msvc"
        ));
        assert!(!eval_cfg(
            r#"cfg(target_family = "unix")"#,
            "x86_64-pc-windows-msvc"
        ));
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
        assert!(eval_cfg(
            r#"cfg(not(target_os = "windows"))"#,
            "x86_64-unknown-linux-gnu"
        ));
        assert!(!eval_cfg(
            r#"cfg(not(target_os = "linux"))"#,
            "x86_64-unknown-linux-gnu"
        ));
    }

    #[test]
    fn test_eval_cfg_literal_triple() {
        assert!(eval_cfg(
            "x86_64-unknown-linux-gnu",
            "x86_64-unknown-linux-gnu"
        ));
        assert!(!eval_cfg(
            "aarch64-unknown-linux-gnu",
            "x86_64-unknown-linux-gnu"
        ));
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
        assert!(eval_cfg(
            r#"cfg(target_env = "gnu")"#,
            "x86_64-unknown-linux-gnu"
        ));
        assert!(eval_cfg(
            r#"cfg(target_env = "msvc")"#,
            "x86_64-pc-windows-msvc"
        ));
        assert!(!eval_cfg(
            r#"cfg(target_env = "musl")"#,
            "x86_64-unknown-linux-gnu"
        ));
    }

    #[test]
    fn test_parse_format_section() {
        let toml_str = r#"
[package]
name = "test"
version = "0.1.0"

[format]
include_dirs = ["benchmarks", "examples"]
exclude = ["generated/**"]
"#;
        let manifest = Manifest::from_str(toml_str).unwrap();
        let fmt = manifest.format.unwrap();
        assert_eq!(fmt.include_dirs, vec!["benchmarks", "examples"]);
        assert_eq!(fmt.exclude, vec!["generated/**"]);
    }

    #[test]
    fn test_parse_lint_section() {
        let toml_str = r#"
[package]
name = "test"
version = "0.1.0"

[lint]
include_dirs = ["benchmarks"]
exclude = ["vendor/**"]
max_line_length = 100
clang_tidy = true
"#;
        let manifest = Manifest::from_str(toml_str).unwrap();
        let lint = manifest.lint.unwrap();
        assert_eq!(lint.include_dirs, vec!["benchmarks"]);
        assert_eq!(lint.exclude, vec!["vendor/**"]);
        assert_eq!(lint.max_line_length, Some(100));
        assert_eq!(lint.clang_tidy, Some(true));
    }

    #[test]
    fn test_parse_lint_defaults() {
        let toml_str = r#"
[package]
name = "test"
version = "0.1.0"

[lint]
"#;
        let manifest = Manifest::from_str(toml_str).unwrap();
        let lint = manifest.lint.unwrap();
        assert!(lint.include_dirs.is_empty());
        assert!(lint.exclude.is_empty());
        assert_eq!(lint.max_line_length, None);
        assert_eq!(lint.clang_tidy, None);
    }

    #[test]
    fn test_format_and_lint_absent_by_default() {
        let manifest = default_manifest("hello");
        assert!(manifest.format.is_none());
        assert!(manifest.lint.is_none());
    }
}
