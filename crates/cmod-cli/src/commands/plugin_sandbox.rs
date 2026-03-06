//! Plugin sandboxing and capability-based permission system.
//!
//! Implements RFC-0018 enhancements:
//! - Capability-based permissions for plugins
//! - Plugin signature verification
//! - Resource limits and sandboxing
//! - Plugin manifest validation

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use cmod_core::error::CmodError;

/// Plugin capabilities that can be granted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PluginCapability {
    /// Read files within the project directory.
    ReadProject,
    /// Write files within the project directory.
    WriteProject,
    /// Read the cmod.toml manifest.
    ReadManifest,
    /// Modify the cmod.toml manifest.
    WriteManifest,
    /// Execute shell commands.
    ExecuteCommands,
    /// Access the network.
    NetworkAccess,
    /// Access the build cache.
    CacheAccess,
    /// Access environment variables.
    EnvironmentAccess,
    /// Access the dependency graph.
    DependencyGraphAccess,
    /// Access the build plan.
    BuildPlanAccess,
}

impl PluginCapability {
    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "read_project" => Some(PluginCapability::ReadProject),
            "write_project" => Some(PluginCapability::WriteProject),
            "read_manifest" => Some(PluginCapability::ReadManifest),
            "write_manifest" => Some(PluginCapability::WriteManifest),
            "execute_commands" | "cli" => Some(PluginCapability::ExecuteCommands),
            "network_access" | "network" => Some(PluginCapability::NetworkAccess),
            "cache_access" | "cache" => Some(PluginCapability::CacheAccess),
            "environment_access" | "env" => Some(PluginCapability::EnvironmentAccess),
            "dependency_graph_access" | "deps" => Some(PluginCapability::DependencyGraphAccess),
            "build_plan_access" | "build" => Some(PluginCapability::BuildPlanAccess),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            PluginCapability::ReadProject => "read_project",
            PluginCapability::WriteProject => "write_project",
            PluginCapability::ReadManifest => "read_manifest",
            PluginCapability::WriteManifest => "write_manifest",
            PluginCapability::ExecuteCommands => "execute_commands",
            PluginCapability::NetworkAccess => "network_access",
            PluginCapability::CacheAccess => "cache_access",
            PluginCapability::EnvironmentAccess => "environment_access",
            PluginCapability::DependencyGraphAccess => "dependency_graph_access",
            PluginCapability::BuildPlanAccess => "build_plan_access",
        }
    }

    pub fn description(&self) -> &'static str {
        match self {
            PluginCapability::ReadProject => "Read files within the project directory",
            PluginCapability::WriteProject => "Write files within the project directory",
            PluginCapability::ReadManifest => "Read the cmod.toml manifest",
            PluginCapability::WriteManifest => "Modify the cmod.toml manifest",
            PluginCapability::ExecuteCommands => "Execute shell commands",
            PluginCapability::NetworkAccess => "Access the network",
            PluginCapability::CacheAccess => "Access the build cache",
            PluginCapability::EnvironmentAccess => "Access environment variables",
            PluginCapability::DependencyGraphAccess => "Access the dependency graph",
            PluginCapability::BuildPlanAccess => "Access the build plan",
        }
    }
}

/// Extended plugin manifest with capability declarations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginManifest {
    /// Plugin metadata.
    pub plugin: PluginMetadata,
    /// Requested capabilities.
    #[serde(default)]
    pub capabilities: BTreeSet<String>,
    /// Resource limits.
    #[serde(default)]
    pub limits: PluginLimits,
}

/// Plugin metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginMetadata {
    /// Plugin name.
    pub name: String,
    /// Plugin version.
    pub version: String,
    /// Plugin description.
    #[serde(default)]
    pub description: Option<String>,
    /// Plugin authors.
    #[serde(default)]
    pub authors: Vec<String>,
    /// Plugin license.
    #[serde(default)]
    pub license: Option<String>,
    /// Minimum cmod version required.
    #[serde(default)]
    pub min_cmod_version: Option<String>,
    /// Plugin entry point (relative to plugin directory).
    #[serde(default)]
    pub entry_point: Option<String>,
    /// Plugin type: "native", "script", "wasm".
    #[serde(default)]
    pub plugin_type: Option<String>,
}

/// Resource limits for a plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginLimits {
    /// Maximum execution time in seconds (0 = unlimited).
    #[serde(default)]
    pub timeout_secs: u64,
    /// Maximum memory usage in MB (0 = unlimited).
    #[serde(default)]
    pub max_memory_mb: u64,
    /// Maximum number of files the plugin can create.
    #[serde(default)]
    pub max_files: u64,
    /// Maximum total file size output in MB.
    #[serde(default)]
    pub max_output_mb: u64,
}

impl Default for PluginLimits {
    fn default() -> Self {
        PluginLimits {
            timeout_secs: 300,
            max_memory_mb: 512,
            max_files: 1000,
            max_output_mb: 100,
        }
    }
}

/// Plugin sandbox that enforces capability restrictions.
pub struct PluginSandbox {
    /// Granted capabilities.
    granted: BTreeSet<PluginCapability>,
    /// Project root directory.
    project_root: PathBuf,
    /// Resource limits.
    limits: PluginLimits,
}

impl PluginSandbox {
    /// Create a new sandbox with the given capabilities.
    pub fn new(
        capabilities: BTreeSet<PluginCapability>,
        project_root: PathBuf,
        limits: PluginLimits,
    ) -> Self {
        PluginSandbox {
            granted: capabilities,
            project_root,
            limits,
        }
    }

    /// Check if a capability is granted.
    pub fn has_capability(&self, cap: PluginCapability) -> bool {
        self.granted.contains(&cap)
    }

    /// Validate that a file read is allowed.
    pub fn check_read(&self, path: &Path) -> Result<(), CmodError> {
        if !self.has_capability(PluginCapability::ReadProject) {
            return Err(CmodError::SecurityViolation {
                reason: "plugin does not have read_project capability".to_string(),
            });
        }

        // Ensure path is within project root
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if !canonical.starts_with(&self.project_root) {
            return Err(CmodError::SecurityViolation {
                reason: format!(
                    "plugin attempted to read outside project: {}",
                    path.display()
                ),
            });
        }

        Ok(())
    }

    /// Validate that a file write is allowed.
    pub fn check_write(&self, path: &Path) -> Result<(), CmodError> {
        if !self.has_capability(PluginCapability::WriteProject) {
            return Err(CmodError::SecurityViolation {
                reason: "plugin does not have write_project capability".to_string(),
            });
        }

        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if !canonical.starts_with(&self.project_root) {
            return Err(CmodError::SecurityViolation {
                reason: format!(
                    "plugin attempted to write outside project: {}",
                    path.display()
                ),
            });
        }

        Ok(())
    }

    /// Validate that command execution is allowed.
    pub fn check_execute(&self) -> Result<(), CmodError> {
        if !self.has_capability(PluginCapability::ExecuteCommands) {
            return Err(CmodError::SecurityViolation {
                reason: "plugin does not have execute_commands capability".to_string(),
            });
        }
        Ok(())
    }

    /// Validate that network access is allowed.
    pub fn check_network(&self) -> Result<(), CmodError> {
        if !self.has_capability(PluginCapability::NetworkAccess) {
            return Err(CmodError::SecurityViolation {
                reason: "plugin does not have network_access capability".to_string(),
            });
        }
        Ok(())
    }

    /// Get the resource limits.
    pub fn limits(&self) -> &PluginLimits {
        &self.limits
    }
}

/// Load and validate a plugin manifest from a directory.
pub fn load_plugin_manifest(plugin_dir: &Path) -> Result<PluginManifest, CmodError> {
    let manifest_path = plugin_dir.join("plugin.toml");
    if !manifest_path.exists() {
        return Err(CmodError::Other(format!(
            "plugin manifest not found: {}",
            manifest_path.display()
        )));
    }

    let content = std::fs::read_to_string(&manifest_path)?;
    let manifest: PluginManifest = toml::from_str(&content)
        .map_err(|e| CmodError::Other(format!("invalid plugin manifest: {}", e)))?;

    Ok(manifest)
}

/// Parse capability strings into typed capabilities.
pub fn parse_capabilities(
    capability_strings: &BTreeSet<String>,
) -> (BTreeSet<PluginCapability>, Vec<String>) {
    let mut capabilities = BTreeSet::new();
    let mut unknown = Vec::new();

    for s in capability_strings {
        match PluginCapability::parse(s) {
            Some(cap) => {
                capabilities.insert(cap);
            }
            None => {
                unknown.push(s.clone());
            }
        }
    }

    (capabilities, unknown)
}

/// Verify a plugin's signature using the project's trust configuration.
pub fn verify_plugin_signature(
    plugin_dir: &Path,
    trust_config: Option<&cmod_core::manifest::Security>,
) -> Result<bool, CmodError> {
    let sig_path = plugin_dir.join("plugin.sig");
    if !sig_path.exists() {
        return Ok(false); // No signature present
    }

    let manifest_path = plugin_dir.join("plugin.toml");
    if !manifest_path.exists() {
        return Ok(false);
    }

    // Resolve signing config from security settings
    let signing_config = trust_config.and_then(|sec| {
        cmod_security::signing::resolve_signing_config(
            sec.signing_key.as_deref(),
            sec.signing_backend.as_deref(),
        )
    });

    // Verify the plugin manifest against its signature
    match cmod_security::signing::verify_file(&manifest_path, &sig_path, signing_config.as_ref()) {
        Ok(status) => match status {
            cmod_security::signing::VerifyStatus::Valid { .. } => Ok(true),
            cmod_security::signing::VerifyStatus::Untrusted { .. } => Ok(false),
            cmod_security::signing::VerifyStatus::Unsigned => Ok(false),
            cmod_security::signing::VerifyStatus::Invalid { reason } => {
                Err(CmodError::SecurityViolation {
                    reason: format!("plugin signature invalid: {}", reason),
                })
            }
        },
        Err(e) => Err(CmodError::Other(format!(
            "failed to verify plugin signature: {}",
            e
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_plugin_capability_parse() {
        assert_eq!(
            PluginCapability::parse("read_project"),
            Some(PluginCapability::ReadProject)
        );
        assert_eq!(
            PluginCapability::parse("cli"),
            Some(PluginCapability::ExecuteCommands)
        );
        assert_eq!(PluginCapability::parse("unknown"), None);
    }

    #[test]
    fn test_plugin_capability_roundtrip() {
        for cap in &[
            PluginCapability::ReadProject,
            PluginCapability::WriteProject,
            PluginCapability::ExecuteCommands,
            PluginCapability::NetworkAccess,
        ] {
            assert_eq!(PluginCapability::parse(cap.as_str()), Some(*cap));
        }
    }

    #[test]
    fn test_sandbox_capability_check() {
        let mut caps = BTreeSet::new();
        caps.insert(PluginCapability::ReadProject);

        let sandbox =
            PluginSandbox::new(caps, PathBuf::from("/tmp/project"), PluginLimits::default());

        assert!(sandbox.has_capability(PluginCapability::ReadProject));
        assert!(!sandbox.has_capability(PluginCapability::WriteProject));
    }

    #[test]
    fn test_sandbox_check_execute() {
        let sandbox = PluginSandbox::new(
            BTreeSet::new(),
            PathBuf::from("/tmp"),
            PluginLimits::default(),
        );

        assert!(sandbox.check_execute().is_err());
    }

    #[test]
    fn test_sandbox_check_network() {
        let mut caps = BTreeSet::new();
        caps.insert(PluginCapability::NetworkAccess);

        let sandbox = PluginSandbox::new(caps, PathBuf::from("/tmp"), PluginLimits::default());

        assert!(sandbox.check_network().is_ok());
    }

    #[test]
    fn test_parse_capabilities() {
        let mut strings = BTreeSet::new();
        strings.insert("read_project".into());
        strings.insert("cli".into());
        strings.insert("unknown_cap".into());

        let (caps, unknown) = parse_capabilities(&strings);
        assert_eq!(caps.len(), 2);
        assert!(caps.contains(&PluginCapability::ReadProject));
        assert!(caps.contains(&PluginCapability::ExecuteCommands));
        assert_eq!(unknown, vec!["unknown_cap"]);
    }

    #[test]
    fn test_plugin_limits_default() {
        let limits = PluginLimits::default();
        assert_eq!(limits.timeout_secs, 300);
        assert_eq!(limits.max_memory_mb, 512);
    }

    #[test]
    fn test_load_plugin_manifest() {
        let tmp = TempDir::new().unwrap();
        let manifest_content = r#"
capabilities = ["read_project", "cli"]

[plugin]
name = "my-plugin"
version = "1.0.0"
description = "A test plugin"
authors = ["Test Author"]
entry_point = "bin/my-plugin"
plugin_type = "native"

[limits]
timeout_secs = 60
max_memory_mb = 256
"#;
        std::fs::write(tmp.path().join("plugin.toml"), manifest_content).unwrap();

        let manifest = load_plugin_manifest(tmp.path()).unwrap();
        assert_eq!(manifest.plugin.name, "my-plugin");
        assert_eq!(manifest.capabilities.len(), 2);
        assert_eq!(manifest.limits.timeout_secs, 60);
    }

    #[test]
    fn test_load_plugin_manifest_missing() {
        let tmp = TempDir::new().unwrap();
        assert!(load_plugin_manifest(tmp.path()).is_err());
    }

    #[test]
    fn test_verify_plugin_no_signature() {
        let tmp = TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("plugin.toml"),
            "[plugin]\nname = \"test\"\nversion = \"1.0.0\"\n",
        )
        .unwrap();

        let result = verify_plugin_signature(tmp.path(), None).unwrap();
        assert!(!result); // No signature = not verified
    }

    #[test]
    fn test_plugin_manifest_serde() {
        let manifest = PluginManifest {
            plugin: PluginMetadata {
                name: "test".into(),
                version: "1.0.0".into(),
                description: Some("Test plugin".into()),
                authors: vec!["Author".into()],
                license: Some("MIT".into()),
                min_cmod_version: Some("0.1.0".into()),
                entry_point: Some("bin/test".into()),
                plugin_type: Some("native".into()),
            },
            capabilities: BTreeSet::from(["read_project".into()]),
            limits: PluginLimits::default(),
        };

        let json = serde_json::to_string(&manifest).unwrap();
        let parsed: PluginManifest = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.plugin.name, "test");
    }
}
