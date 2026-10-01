//! Incremental rebuild detection via persistent build state.
//!
//! Tracks per-node content hashes so unchanged nodes can be skipped
//! without full cache lookups. Stores state in `.cmod-build-state.json`.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use serde::{Deserialize, Serialize};

use cmod_cache::key::hash_file;
use cmod_core::error::CmodError;

use crate::plan::BuildNode;

/// Build state persisted between builds for incremental detection.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct BuildState {
    /// Per-node state, keyed by node ID.
    pub nodes: BTreeMap<String, NodeState>,
    /// Per-node compilation time in milliseconds from the last build.
    #[serde(default)]
    pub node_timings: BTreeMap<String, u64>,
    /// Every header any node included, once: nodes refer to it by index
    /// (most headers are included by many nodes).
    #[serde(default)]
    pub headers: Vec<HeaderState>,
    /// Path → index into `headers`. Rebuilt on load.
    #[serde(skip)]
    header_index: HashMap<PathBuf, usize>,
    /// Per index into `headers`: whether the header changed since it was
    /// recorded. Checked once per header, on first use.
    #[serde(skip)]
    header_changed: OnceLock<Vec<bool>>,
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
    /// Source file mtime (epoch milliseconds) for fast-path invalidation.
    #[serde(default)]
    pub mtime: Option<u64>,
    /// Headers the source included when it was last built, as indices into
    /// [`BuildState::headers`]. `None` means unknown (state written by a
    /// cmod that did not record them, or a node built without a dependency
    /// file), which forces a rebuild.
    #[serde(default)]
    pub headers: Option<Vec<usize>>,
}

/// A header some node included, as it was when that node was built.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HeaderState {
    /// Absolute path of the header.
    pub path: PathBuf,
    /// SHA-256 of the header's content.
    pub hash: String,
    /// Header mtime (epoch milliseconds): when unchanged, the hash is not
    /// recomputed.
    #[serde(default)]
    pub mtime: Option<u64>,
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
    /// A header the source includes changed or disappeared.
    HeaderChanged(PathBuf),
    /// The headers the source includes were not recorded.
    HeadersUnknown,
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
            RebuildReason::HeaderChanged(path) => {
                write!(f, "included header changed: {}", path.display())
            }
            RebuildReason::HeadersUnknown => write!(f, "included headers not recorded"),
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
        let mut state: Self = match std::fs::read_to_string(&path) {
            Ok(content) => serde_json::from_str(&content).unwrap_or_default(),
            Err(_) => Self::default(),
        };
        state.header_index = state
            .headers
            .iter()
            .enumerate()
            .map(|(i, h)| (h.path.clone(), i))
            .collect();
        state
    }

    /// Add `header` to the header table, replacing an entry for the same
    /// path, and return its index.
    fn intern_header(&mut self, header: HeaderState) -> usize {
        match self.header_index.get(&header.path) {
            Some(&i) => {
                self.headers[i] = header;
                i
            }
            None => {
                let i = self.headers.len();
                self.header_index.insert(header.path.clone(), i);
                self.headers.push(header);
                i
            }
        }
    }

    /// Copy `node_id`'s state from `prev`, for a node that was up to date.
    pub fn carry_over(&mut self, prev: &BuildState, node_id: &str) {
        let Some(state) = prev.nodes.get(node_id) else {
            return;
        };
        let mut state = state.clone();
        let headers: Option<Option<Vec<HeaderState>>> = state
            .headers
            .as_ref()
            .map(|ids| ids.iter().map(|&i| prev.headers.get(i).cloned()).collect());
        // An index outside `prev`'s table makes the header set unknown.
        state.headers = headers
            .flatten()
            .map(|headers| headers.into_iter().map(|h| self.intern_header(h)).collect());
        self.nodes.insert(node_id.to_string(), state);
    }

    /// Save build state to disk.
    pub fn save(&self, build_dir: &Path) -> Result<(), CmodError> {
        let path = Self::state_path(build_dir);
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| CmodError::Other(format!("failed to serialize build state: {}", e)))?;
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
    ///
    /// `dep_hashes` are the content hashes of the node's dependencies'
    /// outputs as they are now on disk, in `node.dependencies` order. They
    /// must come from disk, not from this state: a dependency rebuilt
    /// earlier in the same build has new outputs this state has not seen.
    ///
    /// Uses mtime as a fast path: if the mtime hasn't changed, the source
    /// hash is assumed unchanged and the expensive hash computation is skipped.
    ///
    /// Headers are checked against disk once per `BuildState`, on the first
    /// call that reaches them: load a fresh state for each build.
    pub fn needs_rebuild(
        &self,
        node: &BuildNode,
        flags_hash: &str,
        dep_hashes: &[String],
    ) -> Option<RebuildReason> {
        let prev = match self.nodes.get(&node.id) {
            Some(s) => s,
            None => return Some(RebuildReason::NoPreviousState),
        };

        // Check source: use mtime fast path, fall back to hash
        if let Some(ref source) = node.source {
            let current_mtime = file_mtime(source);
            let mtime_changed = match (current_mtime, prev.mtime) {
                (Some(cur), Some(prev_mt)) => cur != prev_mt,
                _ => true, // No mtime available — fall through to hash check
            };

            if mtime_changed {
                return Some(RebuildReason::SourceChanged);
            }
            // mtime unchanged → skip hash, source is assumed the same
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
        if dep_hashes != prev.dep_hashes.as_slice() {
            return Some(RebuildReason::DependencyChanged);
        }

        // Check included headers. The whole table is checked once, on the
        // first node that gets here, not once per including node.
        if node.source.is_some() {
            let Some(ids) = &prev.headers else {
                return Some(RebuildReason::HeadersUnknown);
            };
            let changed = self
                .header_changed
                .get_or_init(|| self.headers.iter().map(HeaderState::changed).collect());
            for &i in ids {
                match (self.headers.get(i), changed.get(i)) {
                    (Some(_), Some(false)) => {}
                    (Some(header), _) => {
                        return Some(RebuildReason::HeaderChanged(header.path.clone()))
                    }
                    (None, _) => return Some(RebuildReason::HeadersUnknown),
                }
            }
        }

        None
    }

    /// Record the state of a successfully built node.
    ///
    /// `dep_hashes` are the dependency output hashes the node was built
    /// against (see [`Self::needs_rebuild`]); `headers` are the
    /// `(absolute path, content hash)` pairs the node's source included, or
    /// `None` when unknown.
    ///
    /// Note: If hash computation fails for output files, we log a warning and
    /// use an empty hash. This means the next build will recompute the node,
    /// which is the safe fallback behavior.
    pub fn record_node(
        &mut self,
        node: &BuildNode,
        flags_hash: &str,
        dep_hashes: &[String],
        headers: Option<&[(PathBuf, String)]>,
    ) {
        let source_hash = node
            .source
            .as_ref()
            .and_then(|s| hash_file(s).ok())
            .unwrap_or_default();

        let mtime = node.source.as_ref().and_then(|s| file_mtime(s));

        let headers = headers.map(|headers| {
            let mut ids = Vec::with_capacity(headers.len());
            for (path, hash) in headers {
                // Another node recorded this header in this build already:
                // same content, so its mtime stands and the stat is saved.
                let known = self.header_index.get(path).map(|&i| &self.headers[i]);
                let mtime = match known {
                    Some(header) if header.hash == *hash => header.mtime,
                    _ => file_mtime(path),
                };
                ids.push(self.intern_header(HeaderState {
                    path: path.clone(),
                    hash: hash.clone(),
                    mtime,
                }));
            }
            ids
        });

        let mut output_hashes = Vec::new();
        for output in &node.outputs {
            let name = output
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or("unknown")
                .to_string();
            let hash = match hash_file(output) {
                Ok(h) => h,
                Err(e) => {
                    // Log warning - hash failure means this node will rebuild next time
                    eprintln!(
                        "warning: failed to hash output file '{}': {} (node will rebuild next time)",
                        output.display(),
                        e
                    );
                    String::new()
                }
            };
            output_hashes.push((name, hash));
        }

        self.nodes.insert(
            node.id.clone(),
            NodeState {
                source_hash,
                dep_hashes: dep_hashes.to_vec(),
                flags_hash: flags_hash.to_string(),
                output_hashes,
                mtime,
                headers,
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

impl HeaderState {
    /// Whether the file differs from this record: mtime fast path, then
    /// content hash, so a `touch` alone is not a change. A missing file is.
    fn changed(&self) -> bool {
        let mtime = file_mtime(&self.path);
        if mtime.is_some() && mtime == self.mtime {
            return false;
        }
        hash_file(&self.path).map_or(true, |hash| hash != self.hash)
    }
}

/// Get the mtime of a file as epoch milliseconds for sub-second granularity.
fn file_mtime(path: &Path) -> Option<u64> {
    std::fs::metadata(path)
        .ok()
        .and_then(|m| m.modified().ok())
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map(|d| d.as_millis() as u64)
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
            external_imports: vec![],
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
                mtime: None,
                headers: None,
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
            state.needs_rebuild(&node, "flags", &[]),
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
        state.record_node(&node, "flags", &[], Some(&[]));

        // Ensure mtime changes (sub-millisecond writes can share same mtime)
        std::thread::sleep(std::time::Duration::from_millis(10));

        // Modify source
        std::fs::write(&src, "version 2").unwrap();

        assert_eq!(
            state.needs_rebuild(&node, "flags", &[]),
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

        state.record_node(&node, "flags_v1", &[], Some(&[]));

        assert_eq!(
            state.needs_rebuild(&node, "flags_v2", &[]),
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

        state.record_node(&node, "flags", &[], Some(&[]));

        // No changes → should be None (up-to-date)
        assert_eq!(state.needs_rebuild(&node, "flags", &[]), None);
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

        state.record_node(&node, "flags", &[], Some(&[]));

        // Remove the output
        std::fs::remove_file(&output).unwrap();

        assert_eq!(
            state.needs_rebuild(&node, "flags", &[]),
            Some(RebuildReason::OutputMissing)
        );
    }

    /// A dependency rebuilt earlier in the same build changes the hashes
    /// the caller passes in, even though this state still holds the old
    /// ones for it.
    #[test]
    fn test_needs_rebuild_dependency_changed() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("main.cpp");
        std::fs::write(&src, "import dep;").unwrap();

        let mut state = BuildState::default();
        let node = make_node("object:main", Some(src), &["interface:dep"]);
        state.record_node(&node, "flags", &["pcm-v1".to_string()], Some(&[]));

        assert_eq!(
            state.needs_rebuild(&node, "flags", &["pcm-v1".to_string()]),
            None
        );
        assert_eq!(
            state.needs_rebuild(&node, "flags", &["pcm-v2".to_string()]),
            Some(RebuildReason::DependencyChanged)
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
                mtime: None,
                headers: None,
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
    fn test_mtime_fast_path_skips_hash() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("test.cppm");
        std::fs::write(&src, "source content").unwrap();

        let mut state = BuildState::default();
        let node = make_node("interface:test", Some(src.clone()), &[]);

        // Record node — this stores both hash and mtime
        state.record_node(&node, "flags", &[], Some(&[]));

        // Verify mtime was stored
        let recorded = state.nodes.get("interface:test").unwrap();
        assert!(recorded.mtime.is_some());

        // Without modifying the file, mtime is the same → should skip hash and be up-to-date
        assert_eq!(state.needs_rebuild(&node, "flags", &[]), None);
    }

    /// A source plus one header it includes, recorded as built.
    fn built_with_header(tmp: &TempDir) -> (BuildState, BuildNode, PathBuf) {
        let src = tmp.path().join("main.cpp");
        let header = tmp.path().join("value.h");
        std::fs::write(&src, "#include \"value.h\"").unwrap();
        std::fs::write(&header, "#define VALUE 1").unwrap();

        let mut state = BuildState::default();
        let node = make_node("object:main", Some(src), &[]);
        let headers = vec![(header.clone(), hash_file(&header).unwrap())];
        state.record_node(&node, "flags", &[], Some(&headers));
        (state, node, header)
    }

    #[test]
    fn test_needs_rebuild_header_changed() {
        let tmp = TempDir::new().unwrap();
        let (state, node, header) = built_with_header(&tmp);
        assert_eq!(state.clone().needs_rebuild(&node, "flags", &[]), None);

        std::thread::sleep(std::time::Duration::from_millis(10));
        std::fs::write(&header, "#define VALUE 2").unwrap();

        assert_eq!(
            state.needs_rebuild(&node, "flags", &[]),
            Some(RebuildReason::HeaderChanged(header))
        );
    }

    /// A header included by many nodes is stored once, and survives
    /// a save/load and a carry-over of an up-to-date node.
    #[test]
    fn test_headers_are_interned() {
        let tmp = TempDir::new().unwrap();
        let header = tmp.path().join("value.h");
        std::fs::write(&header, "#define VALUE 1").unwrap();
        let headers = vec![(header.clone(), hash_file(&header).unwrap())];

        let mut state = BuildState::default();
        let mut nodes = Vec::new();
        for name in ["a", "b", "c"] {
            let src = tmp.path().join(format!("{}.cpp", name));
            std::fs::write(&src, "#include \"value.h\"").unwrap();
            let node = make_node(&format!("object:{}", name), Some(src), &[]);
            state.record_node(&node, "flags", &[], Some(&headers));
            nodes.push(node);
        }
        assert_eq!(state.headers.len(), 1);

        state.save(tmp.path()).unwrap();
        let loaded = BuildState::load(tmp.path());
        assert_eq!(loaded.headers.len(), 1);
        assert_eq!(loaded.needs_rebuild(&nodes[0], "flags", &[]), None);

        let mut next = BuildState::default();
        next.carry_over(&loaded, "object:b");
        assert_eq!(next.headers.len(), 1);
        assert_eq!(next.needs_rebuild(&nodes[1], "flags", &[]), None);
        assert!(!next.nodes.contains_key("object:a"));
    }

    #[test]
    fn test_needs_rebuild_header_index_out_of_range() {
        let tmp = TempDir::new().unwrap();
        let (mut state, node, _) = built_with_header(&tmp);
        state.nodes.get_mut("object:main").unwrap().headers = Some(vec![7]);
        assert_eq!(
            state.needs_rebuild(&node, "flags", &[]),
            Some(RebuildReason::HeadersUnknown)
        );
    }

    #[test]
    fn test_needs_rebuild_header_touched_but_unchanged() {
        let tmp = TempDir::new().unwrap();
        let (state, node, header) = built_with_header(&tmp);

        std::thread::sleep(std::time::Duration::from_millis(10));
        std::fs::write(&header, "#define VALUE 1").unwrap();

        assert_eq!(state.needs_rebuild(&node, "flags", &[]), None);
    }

    #[test]
    fn test_needs_rebuild_header_deleted() {
        let tmp = TempDir::new().unwrap();
        let (state, node, header) = built_with_header(&tmp);

        std::fs::remove_file(&header).unwrap();

        assert_eq!(
            state.needs_rebuild(&node, "flags", &[]),
            Some(RebuildReason::HeaderChanged(header))
        );
    }

    #[test]
    fn test_needs_rebuild_headers_unknown() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("main.cpp");
        std::fs::write(&src, "int main() {}").unwrap();

        let mut state = BuildState::default();
        let node = make_node("object:main", Some(src), &[]);
        state.record_node(&node, "flags", &[], None);

        assert_eq!(
            state.needs_rebuild(&node, "flags", &[]),
            Some(RebuildReason::HeadersUnknown)
        );
    }

    /// State files written before headers were tracked have no `headers`
    /// key: they load, and every node rebuilds once.
    #[test]
    fn test_state_without_headers_field_rebuilds() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("main.cpp");
        std::fs::write(&src, "int main() {}").unwrap();
        let mtime = file_mtime(&src).unwrap();

        let json = format!(
            r#"{{"nodes": {{"object:main": {{"source_hash": "", "dep_hashes": [],
                "flags_hash": "flags", "output_hashes": [], "mtime": {}}}}}}}"#,
            mtime
        );
        std::fs::write(tmp.path().join(".cmod-build-state.json"), json).unwrap();

        let state = BuildState::load(tmp.path());
        let node = make_node("object:main", Some(src), &[]);
        assert_eq!(
            state.needs_rebuild(&node, "flags", &[]),
            Some(RebuildReason::HeadersUnknown)
        );
    }

    #[test]
    fn test_rebuild_reason_display() {
        assert_eq!(
            format!("{}", RebuildReason::SourceChanged),
            "source file changed"
        );
        assert_eq!(format!("{}", RebuildReason::Forced), "forced rebuild");
        assert_eq!(
            format!("{}", RebuildReason::HeaderChanged(PathBuf::from("a/b.h"))),
            format!("included header changed: {}", Path::new("a/b.h").display())
        );
    }
}
