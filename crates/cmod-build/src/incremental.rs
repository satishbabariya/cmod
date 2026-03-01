//! Incremental rebuild detection via persistent build state.
//!
//! Tracks per-node content hashes so unchanged nodes can be skipped
//! without full cache lookups. Stores state in `.cmod-build-state.json`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use cmod_cache::key::hash_file;
use cmod_core::error::CmodError;

use crate::plan::BuildNode;

/// Build state persisted between builds for incremental detection.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BuildState {
    /// Per-node state, keyed by node ID.
    pub nodes: BTreeMap<String, NodeState>,
}

/// State tracked for a single build node.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeState {
    /// Hash of the source file content.
    pub source_hash: String,
    /// Hashes of the dependency outputs this node consumed.
    pub dep_hashes: Vec<String>,
    /// Compiler flags hash.
    pub flags_hash: String,
    /// Output file hashes (after successful compilation).
    pub output_hashes: Vec<(String, String)>,
}

/// Reason why a node needs rebuilding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RebuildReason {
    /// No previous state recorded.
    NoPreviousState,
    /// Source file content changed.
    SourceChanged,
    /// A dependency output changed.
    DependencyChanged,
    /// Compiler flags changed.
    FlagsChanged,
    /// An output file is missing.
    OutputMissing,
    /// Forced rebuild requested (--force).
    Forced,
}

impl std::fmt::Display for RebuildReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RebuildReason::NoPreviousState => write!(f, "no previous build state"),
            RebuildReason::SourceChanged => write!(f, "source file changed"),
            RebuildReason::DependencyChanged => write!(f, "dependency output changed"),
            RebuildReason::FlagsChanged => write!(f, "compiler flags changed"),
            RebuildReason::OutputMissing => write!(f, "output file missing"),
            RebuildReason::Forced => write!(f, "forced rebuild"),
        }
    }
}

impl BuildState {
    /// Load build state from disk.
    pub fn load(build_dir: &Path) -> Self {
        let path = Self::state_path(build_dir);
        if !path.exists() {
            return Self::default();
        }
        match std::fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        }
    }

    /// Save build state to disk.
    pub fn save(&self, build_dir: &Path) -> Result<(), CmodError> {
        let path = Self::state_path(build_dir);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self).map_err(|e| CmodError::Other(
            format!("failed to serialize build state: {}", e),
        ))?;
        std::fs::write(&path, content)?;
        Ok(())
    }

    fn state_path(build_dir: &Path) -> PathBuf {
        build_dir.join(".cmod-build-state.json")
    }

    /// Check whether a node needs rebuilding.
    ///
    /// Returns `None` if the node is up-to-date, or `Some(reason)` if it
    /// needs to be rebuilt.
    pub fn needs_rebuild(
        &self,
        node: &BuildNode,
        flags_hash: &str,
    ) -> Option<RebuildReason> {
        let prev = match self.nodes.get(&node.id) {
            Some(s) => s,
            None => return Some(RebuildReason::NoPreviousState),
        };

        // Check source hash
        if let Some(ref source) = node.source {
            let current_hash = hash_file(source).unwrap_or_default();
            if current_hash != prev.source_hash {
                return Some(RebuildReason::SourceChanged);
            }
        }

        // Check flags
        if flags_hash != prev.flags_hash {
            return Some(RebuildReason::FlagsChanged);
        }

        // Check outputs exist
        for output in &node.outputs {
            if !output.exists() {
                return Some(RebuildReason::OutputMissing);
            }
        }

        // Check dependency outputs haven't changed
        let mut current_dep_hashes = Vec::new();
        for dep_id in &node.dependencies {
            if let Some(dep_state) = self.nodes.get(dep_id) {
                for (_, h) in &dep_state.output_hashes {
                    current_dep_hashes.push(h.clone());
                }
            }
        }
        if current_dep_hashes != prev.dep_hashes {
            return Some(RebuildReason::DependencyChanged);
        }

        None
    }

    /// Record the state of a successfully built node.
    pub fn record_node(
        &mut self,
        node: &BuildNode,
        flags_hash: &str,
    ) {
        let source_hash = node
            .source
            .as_ref()
            .and_then(|s| hash_file(s).ok())
            .unwrap_or_default();

        let mut dep_hashes = Vec::new();
        for dep_id in &node.dependencies {
            if let Some(dep_state) = self.nodes.get(dep_id) {
                for (_, h) in &dep_state.output_hashes {
                    dep_hashes.push(h.clone());
                }
            }
        }

        let mut output_hashes = Vec::new();
        for output in &node.outputs {
            let name = output
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or("unknown")
                .to_string();
            let hash = hash_file(output).unwrap_or_default();
            output_hashes.push((name, hash));
        }

        self.nodes.insert(
            node.id.clone(),
            NodeState {
                source_hash,
                dep_hashes,
                flags_hash: flags_hash.to_string(),
                output_hashes,
            },
        );
    }

    /// Get the rebuild reason for a module by name (for `cmod explain`).
    pub fn explain_module(&self, module_name: &str) -> Option<String> {
        // Look for a node matching this module name
        let node_id_interface = format!("interface:{}", module_name);
        let node_id_impl = format!("impl:{}", module_name);
        let node_id_obj = format!("object:{}", module_name);

        for id in [&node_id_interface, &node_id_impl, &node_id_obj] {
            if let Some(state) = self.nodes.get(id) {
                return Some(format!(
                    "Last build state for {}:\n  Source hash: {}\n  Deps: {} hashes\n  Outputs: {}",
                    id,
                    &state.source_hash[..std::cmp::min(16, state.source_hash.len())],
                    state.dep_hashes.len(),
                    state.output_hashes.len(),
                ));
            }
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmod_core::types::NodeKind;
    use tempfile::TempDir;

    fn make_node(id: &str, source: Option<PathBuf>, deps: &[&str]) -> BuildNode {
        BuildNode {
            id: id.to_string(),
            kind: NodeKind::Interface,
            module_name: Some("test".to_string()),
            source,
            dependencies: deps.iter().map(|s| s.to_string()).collect(),
            outputs: vec![],
        }
    }

    #[test]
    fn test_build_state_save_load_roundtrip() {
        let tmp = TempDir::new().unwrap();
        let mut state = BuildState::default();
        state.nodes.insert(
            "interface:mymod".to_string(),
            NodeState {
                source_hash: "abc123".to_string(),
                dep_hashes: vec!["dep1".to_string()],
                flags_hash: "flags456".to_string(),
                output_hashes: vec![("mymod.pcm".to_string(), "out789".to_string())],
            },
        );

        state.save(tmp.path()).unwrap();
        let loaded = BuildState::load(tmp.path());

        assert_eq!(loaded.nodes.len(), 1);
        assert_eq!(loaded.nodes["interface:mymod"].source_hash, "abc123");
    }

    #[test]
    fn test_load_nonexistent_returns_default() {
        let state = BuildState::load(Path::new("/nonexistent/path"));
        assert!(state.nodes.is_empty());
    }

    #[test]
    fn test_needs_rebuild_no_previous_state() {
        let state = BuildState::default();
        let node = make_node("interface:test", None, &[]);
        assert_eq!(
            state.needs_rebuild(&node, "flags"),
            Some(RebuildReason::NoPreviousState)
        );
    }

    #[test]
    fn test_needs_rebuild_source_changed() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("test.cppm");
        std::fs::write(&src, "version 1").unwrap();

        let mut state = BuildState::default();
        let node = make_node("interface:test", Some(src.clone()), &[]);

        // Record with current content
        state.record_node(&node, "flags");

        // Modify source
        std::fs::write(&src, "version 2").unwrap();

        assert_eq!(
            state.needs_rebuild(&node, "flags"),
            Some(RebuildReason::SourceChanged)
        );
    }

    #[test]
    fn test_needs_rebuild_flags_changed() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("test.cppm");
        std::fs::write(&src, "source content").unwrap();

        let mut state = BuildState::default();
        let node = make_node("interface:test", Some(src), &[]);

        state.record_node(&node, "flags_v1");

        assert_eq!(
            state.needs_rebuild(&node, "flags_v2"),
            Some(RebuildReason::FlagsChanged)
        );
    }

    #[test]
    fn test_needs_rebuild_up_to_date() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("test.cppm");
        std::fs::write(&src, "source content").unwrap();

        let mut state = BuildState::default();
        let node = make_node("interface:test", Some(src), &[]);

        state.record_node(&node, "flags");

        // No changes → should be None (up-to-date)
        assert_eq!(state.needs_rebuild(&node, "flags"), None);
    }

    #[test]
    fn test_needs_rebuild_output_missing() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("test.cppm");
        std::fs::write(&src, "source content").unwrap();

        let output = tmp.path().join("test.pcm");
        std::fs::write(&output, "compiled").unwrap();

        let mut state = BuildState::default();
        let mut node = make_node("interface:test", Some(src), &[]);
        node.outputs = vec![output.clone()];

        state.record_node(&node, "flags");

        // Remove the output
        std::fs::remove_file(&output).unwrap();

        assert_eq!(
            state.needs_rebuild(&node, "flags"),
            Some(RebuildReason::OutputMissing)
        );
    }

    #[test]
    fn test_explain_module_found() {
        let mut state = BuildState::default();
        state.nodes.insert(
            "interface:mymod".to_string(),
            NodeState {
                source_hash: "abcdef1234567890abcdef".to_string(),
                dep_hashes: vec!["dep1".to_string()],
                flags_hash: "flags".to_string(),
                output_hashes: vec![("out.pcm".to_string(), "hash".to_string())],
            },
        );

        let explanation = state.explain_module("mymod");
        assert!(explanation.is_some());
        assert!(explanation.unwrap().contains("interface:mymod"));
    }

    #[test]
    fn test_explain_module_not_found() {
        let state = BuildState::default();
        assert!(state.explain_module("nonexistent").is_none());
    }

    #[test]
    fn test_rebuild_reason_display() {
        assert_eq!(format!("{}", RebuildReason::SourceChanged), "source file changed");
        assert_eq!(format!("{}", RebuildReason::Forced), "forced rebuild");
    }
}
