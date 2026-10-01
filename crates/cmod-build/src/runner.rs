use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crossbeam_channel::bounded;

use cmod_cache::cache::{
    ArtifactCache, ArtifactMetadata, CachedArtifactEntry, IncludeManifest, INCLUDE_MANIFEST_FILE,
};
use cmod_cache::key::{hash_file, CacheKey, CacheKeyInputs, HeaderDigest};
use cmod_core::error::CmodError;
use cmod_core::shell::Shell;
use cmod_core::types::{Artifact, BuildType, NodeKind, Profile};

use crate::compiler::CompilerBackend;
use crate::depfile;
use crate::graph::ModuleGraph;
use crate::incremental::{file_mtime, BuildState, HeaderState};
use crate::plan::{BuildNode, BuildPlan};

/// Statistics from a build execution.
#[derive(Debug, Clone, Default)]
pub struct BuildStats {
    /// Number of nodes that hit the cache.
    pub cache_hits: usize,
    /// Number of nodes that were compiled.
    pub cache_misses: usize,
    /// Number of nodes skipped (link nodes, etc).
    pub skipped: usize,
    /// Number of nodes skipped due to incremental state (up-to-date).
    pub incremental_skipped: usize,
    /// Total wall-clock time for the build.
    pub wall_time_ms: u64,
    /// Sum of individual compilation times (may exceed wall_time when parallel).
    pub total_compile_time_ms: u64,
    /// Per-node compile times in milliseconds, keyed by node ID.
    pub node_timings: BTreeMap<String, u64>,
}

/// Each header a source included, with its hash and the mtime read before
/// hashing it.
type IncludedHeaders = Vec<HeaderState>;

/// What a compiled or restored node was built against, for the next
/// build's incremental check ([`BuildState::record_node`]).
#[derive(Default)]
struct NodeInputs {
    dep_hashes: Vec<String>,
    /// `None` when unknown.
    headers: Option<IncludedHeaders>,
}

/// Build runner that executes a build plan.
pub struct BuildRunner {
    backend: Box<dyn CompilerBackend>,
    cache: Option<ArtifactCache>,
    remote_cache: Option<Box<dyn cmod_cache::RemoteCache>>,
    /// When true, skip cache lookups and always recompile.
    pub no_cache: bool,
    /// When true, ignore incremental state and rebuild everything.
    pub force_rebuild: bool,
    /// Maximum parallel jobs (0 = auto-detect CPU count).
    pub max_jobs: usize,
    /// Extra PCM paths from external sources (e.g., workspace dependencies).
    /// Maps module name to PCM file path.
    extra_pcm_paths: HashMap<String, PathBuf>,
    /// Extra object files to link (e.g., from workspace dependencies).
    extra_obj_paths: Vec<PathBuf>,
    /// Memoized `clang --version` result (populated lazily).
    compiler_version_cache: std::sync::OnceLock<String>,
    /// Directories containing precompiled BMI packages to check before compiling.
    bmi_dirs: Vec<PathBuf>,
    /// Shell for colored, structured output.
    shell: Option<Arc<Shell>>,
    /// Optional distributed worker pool for remote compilation.
    worker_pool: Option<crate::distributed::WorkerPool>,
    /// Headers and dependency outputs read during the current build, as
    /// observed once: content hash plus the mtime read before hashing
    /// (`None`: unreadable). Many sources include the same headers and
    /// import the same BMIs.
    file_hashes: Mutex<HashMap<PathBuf, Option<HeaderState>>>,
}

/// Outcome of executing a single build node.
enum NodeOutcome {
    CacheHit(u64),
    Compiled(u64),
    Linked(u64),
    /// Node skipped because incremental state shows it's up-to-date.
    Skipped(u64),
}

impl NodeOutcome {
    fn time_ms(&self) -> u64 {
        match self {
            NodeOutcome::CacheHit(ms)
            | NodeOutcome::Compiled(ms)
            | NodeOutcome::Linked(ms)
            | NodeOutcome::Skipped(ms) => *ms,
        }
    }
}

impl BuildRunner {
    pub fn new(backend: Box<dyn CompilerBackend>, cache: Option<ArtifactCache>) -> Self {
        BuildRunner {
            backend,
            cache,
            remote_cache: None,
            no_cache: false,
            force_rebuild: false,
            max_jobs: 0,
            extra_pcm_paths: HashMap::new(),
            extra_obj_paths: Vec::new(),
            compiler_version_cache: std::sync::OnceLock::new(),
            bmi_dirs: Vec::new(),
            shell: None,
            worker_pool: None,
            file_hashes: Mutex::new(HashMap::new()),
        }
    }

    /// Return the cached compiler version string, running `clang --version`
    /// on the first call.
    fn compiler_version(&self) -> &str {
        self.compiler_version_cache
            .get_or_init(|| self.backend.version())
    }

    /// Set the maximum parallel jobs.
    pub fn with_jobs(mut self, jobs: usize) -> Self {
        self.max_jobs = jobs;
        self
    }

    /// Attach a remote cache backend.
    pub fn with_remote_cache(mut self, remote: Box<dyn cmod_cache::RemoteCache>) -> Self {
        self.remote_cache = Some(remote);
        self
    }

    /// Disable cache lookups and stores.
    pub fn with_no_cache(mut self, no_cache: bool) -> Self {
        self.no_cache = no_cache;
        self
    }

    /// Enable force rebuild (ignore incremental state).
    pub fn with_force(mut self, force: bool) -> Self {
        self.force_rebuild = force;
        self
    }

    /// Add extra PCM paths from external sources (e.g., other workspace members).
    pub fn with_extra_pcm_paths(mut self, pcms: HashMap<String, PathBuf>) -> Self {
        self.extra_pcm_paths = pcms;
        self
    }

    /// Add extra object files to link (e.g., from workspace dependencies).
    pub fn with_extra_obj_paths(mut self, objs: Vec<PathBuf>) -> Self {
        self.extra_obj_paths = objs;
        self
    }

    /// Attach a Shell for colored, structured output.
    pub fn with_shell(mut self, shell: Arc<Shell>) -> Self {
        self.shell = Some(shell);
        self
    }

    /// Add BMI directories for precompiled module lookup.
    pub fn with_bmi_dirs(mut self, dirs: Vec<PathBuf>) -> Self {
        self.bmi_dirs = dirs;
        self
    }

    /// Attach a distributed worker pool for remote compilation.
    pub fn with_worker_pool(mut self, pool: crate::distributed::WorkerPool) -> Self {
        self.worker_pool = Some(pool);
        self
    }

    /// Check if a compatible precompiled BMI exists in any configured BMI directory.
    ///
    /// Searches each BMI directory for an `index.json` matching the given module name,
    /// then checks for a variant compatible with the current compiler settings.
    fn find_precompiled_bmi(&self, module_name: &str) -> Option<PathBuf> {
        for bmi_dir in &self.bmi_dirs {
            let index_path = bmi_dir.join("index.json");
            if !index_path.exists() {
                continue;
            }

            let content = match std::fs::read_to_string(&index_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let index: cmod_cache::distribution::BmiIndex = match serde_json::from_str(&content) {
                Ok(i) => i,
                Err(_) => continue,
            };

            if index.module_name != module_name {
                continue;
            }

            let compiler = self.backend.kind().to_string();
            let compiler_version = self.compiler_version();
            let target = self.backend.target().unwrap_or("");

            if let Some(variant) = cmod_cache::distribution::find_compatible_variant(
                &index,
                &compiler,
                compiler_version,
                target,
                self.backend.cxx_standard(),
            ) {
                let variant_dir = bmi_dir.join(&variant.directory);
                if variant_dir.exists() {
                    return Some(variant_dir);
                }
            }
        }

        None
    }

    /// Restore a node's outputs from a precompiled BMI variant directory.
    ///
    /// Copies `.pcm` and `.o` files from the variant directory to the node's
    /// expected output locations. Returns `true` on success.
    fn restore_bmi_from_dir(&self, variant_dir: &Path, node: &BuildNode) -> bool {
        for output in &node.outputs {
            let file_name = match output.file_name().and_then(|f| f.to_str()) {
                Some(n) => n,
                None => return false,
            };
            let src = variant_dir.join(file_name);
            if !src.exists() {
                return false;
            }
            if let Some(parent) = output.parent() {
                let _ = fs::create_dir_all(parent);
            }
            if fs::copy(&src, output).is_err() {
                return false;
            }
        }
        !node.outputs.is_empty()
    }

    /// Emit a normal status message through Shell, or fall back to eprintln.
    fn emit(&self, label: &str, message: impl std::fmt::Display) {
        if let Some(ref shell) = self.shell {
            shell.status(label, message);
        } else {
            eprintln!("{:>12}: {}", label, message);
        }
    }

    /// Emit a verbose-only status message through Shell, or fall back to eprintln.
    fn emit_verbose(&self, label: &str, message: impl std::fmt::Display) {
        if let Some(ref shell) = self.shell {
            shell.verbose(label, message);
        } else {
            eprintln!("{:>12}: {}", label, message);
        }
    }

    /// Compute a hash representing the current compiler configuration:
    /// the backend fingerprint plus the compiler executable and its
    /// version. The fingerprint covers flags but not which compiler runs
    /// them, so without the latter, pointing `CXX` at another installation
    /// or upgrading one in place left objects and links "up-to-date". MSVC's
    /// `link`/`lib` are resolved next to `cl`, so its path covers them.
    fn flags_hash(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(self.backend.fingerprint().as_bytes());
        hasher.update(b"\0");
        hasher.update(self.backend.compiler_path().to_string_lossy().as_bytes());
        hasher.update(b"\0");
        hasher.update(self.compiler_version().as_bytes());
        format!("{:x}", hasher.finalize())
    }

    fn clear_file_hashes(&self) {
        match self.file_hashes.lock() {
            Ok(mut guard) => guard.clear(),
            Err(poisoned) => poisoned.into_inner().clear(),
        }
    }

    /// Get the effective parallelism level.
    pub fn effective_jobs(&self) -> usize {
        if self.max_jobs == 0 {
            std::thread::available_parallelism()
                .map(|p| p.get())
                .unwrap_or(1)
        } else {
            self.max_jobs
        }
    }

    /// Execute a full build from a module graph.
    pub fn build(
        &self,
        graph: &ModuleGraph,
        build_dir: &Path,
        target: &str,
        profile: Profile,
        build_type: BuildType,
        package_name: Option<&str>,
    ) -> Result<PathBuf, CmodError> {
        // Validate the graph
        graph.validate()?;

        // Generate the build plan
        let plan = BuildPlan::from_graph(
            graph,
            build_dir,
            target,
            profile,
            build_type,
            package_name,
            self.backend.bmi_extension(),
        )?;

        // Ensure output directories exist
        fs::create_dir_all(build_dir.join("pcm"))?;
        fs::create_dir_all(build_dir.join("obj"))?;
        let pruned = plan.prune_stale_outputs();
        if pruned > 0 {
            self.emit_verbose("Pruned", format!("{} stale build outputs", pruned));
        }

        // Execute the plan
        let (output, _stats) = self.execute_plan(&plan)?;
        Ok(output)
    }

    /// Execute a full build and return statistics.
    pub fn build_with_stats(
        &self,
        graph: &ModuleGraph,
        build_dir: &Path,
        target: &str,
        profile: Profile,
        build_type: BuildType,
        package_name: Option<&str>,
    ) -> Result<(PathBuf, BuildStats), CmodError> {
        graph.validate()?;
        let plan = BuildPlan::from_graph(
            graph,
            build_dir,
            target,
            profile,
            build_type,
            package_name,
            self.backend.bmi_extension(),
        )?;
        fs::create_dir_all(build_dir.join("pcm"))?;
        fs::create_dir_all(build_dir.join("obj"))?;
        let pruned = plan.prune_stale_outputs();
        if pruned > 0 {
            self.emit_verbose("Pruned", format!("{} stale build outputs", pruned));
        }
        self.execute_plan(&plan)
    }

    /// Content hashes of the outputs of `node`'s dependencies, as they are on
    /// disk now: the plan's own dependency nodes in `node.dependencies`
    /// order, then the BMIs of imported modules from other packages.
    fn dep_output_hashes(&self, node: &BuildNode, plan: &BuildPlan) -> Vec<String> {
        let mut hashes = Vec::new();
        for dep_id in &node.dependencies {
            if let Some(dep_node) = plan.nodes.iter().find(|n| &n.id == dep_id) {
                for output in &dep_node.outputs {
                    if let Some(hash) = self.hash_input(output) {
                        hashes.push(hash);
                    }
                }
            }
        }
        for module in &node.external_imports {
            if let Some(hash) = self
                .extra_pcm_paths
                .get(module)
                .and_then(|bmi| self.hash_input(bmi))
            {
                hashes.push(hash);
            }
        }
        hashes
    }

    /// Compute a node's cache key from everything but its headers. Artifacts
    /// live under [`CacheKey::with_headers`] of this key; its include
    /// manifest under [`CacheKey::include_manifest_key`].
    fn compute_cache_key(
        &self,
        node: &BuildNode,
        plan: &BuildPlan,
        dep_hashes: &[String],
    ) -> Option<(String, CacheKey)> {
        let source = node.source.as_ref()?;
        let module_id = node.module_name.as_ref()?;

        let source_hash = hash_file(source).ok()?;

        let compiler_version = self.compiler_version().to_string();

        // The backend fingerprint covers stdlib, sysroot, LTO, optimization,
        // and extra flags — everything configuration-derived that affects
        // codegen — so those no longer need individual fields here.
        let inputs = CacheKeyInputs {
            source_hash,
            dependency_hashes: dep_hashes.to_vec(),
            compiler: self.backend.kind().to_string(),
            compiler_version,
            cxx_standard: self.backend.cxx_standard().to_string(),
            stdlib: String::new(),
            target: plan.target.clone(),
            flags: vec![self.backend.fingerprint()],
        };

        Some((module_id.clone(), CacheKey::compute(&inputs)))
    }

    /// Content hash of a header or dependency output, computed once per
    /// build. Only call it for a dependency output once that dependency has
    /// finished: the scheduler guarantees this for a node's own
    /// dependencies.
    fn hash_input(&self, path: &Path) -> Option<String> {
        self.observe_input(path).map(|observed| observed.hash)
    }

    /// [`HeaderState::observe`] of `path`, once per build.
    fn observe_input(&self, path: &Path) -> Option<HeaderState> {
        let lock = || match self.file_hashes.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(observed) = lock().get(path) {
            return observed.clone();
        }
        // Hash outside the lock: BMIs can be large, and workers hash in
        // parallel. Two workers may hash the same file once each.
        let observed = HeaderState::observe(path);
        lock().insert(path.to_path_buf(), observed.clone());
        observed
    }

    /// The headers the compile that just wrote `obj_output` read, hashed:
    /// absolute paths for build state, and package-relative digests for
    /// cache keys. `None` when the compiler reported no header list, a
    /// header can no longer be read, or a header may have changed while the
    /// compile that started at `compile_start` (epoch ms) ran: its hash
    /// might then not be what the compiler read. ccache refuses to cache in
    /// that case for the same reason. `None` means not cached and rebuilt
    /// next time.
    fn hashed_headers(
        &self,
        source: &Path,
        obj_output: &Path,
        compile_start: u64,
    ) -> Option<(IncludedHeaders, Vec<HeaderDigest>)> {
        let paths = self.backend.included_headers(source, obj_output)?;
        let root = package_root(source);
        let mut included = Vec::with_capacity(paths.len());
        let mut digests = Vec::with_capacity(paths.len());
        for path in paths {
            let observed = self.observe_input(&path)?;
            let mtime = file_mtime(&path)?;
            if observed.mtime != Some(mtime) || mtime >= compile_start {
                return None;
            }
            digests.push(HeaderDigest {
                path: portable_header_path(&path, root.as_deref()),
                hash: observed.hash.clone(),
            });
            included.push(observed);
        }
        Some((included, digests))
    }

    /// Restore a node's outputs from the cache. `key` covers everything but
    /// the headers; the include manifest stored under it names the header
    /// sets seen before, and the first set whose headers all still have the
    /// recorded content gives the artifacts' key. Local manifest first, then
    /// the remote one. Returns the matched headers on a hit.
    fn restore_from_cache(
        &self,
        module_id: &str,
        key: &CacheKey,
        node: &BuildNode,
    ) -> Option<IncludedHeaders> {
        if self.no_cache {
            return None;
        }
        let source = node.source.as_ref()?;
        let root = package_root(source);

        let local = self
            .cache
            .as_ref()
            .and_then(|cache| cache.get_include_manifest(module_id, key))
            .unwrap_or_default();
        for entry in &local.entries {
            if let Some(headers) =
                self.restore_header_set(module_id, key, entry, root.as_deref(), node)
            {
                return Some(headers);
            }
        }

        let scratch = depfile::depfile_path(node.outputs.last()?, "remote-includes.json");
        let remote = self.fetch_remote_include_manifest(module_id, key, &scratch)?;
        for entry in remote.entries.iter().filter(|e| !local.entries.contains(e)) {
            if let Some(headers) =
                self.restore_header_set(module_id, key, entry, root.as_deref(), node)
            {
                if let Some(ref cache) = self.cache {
                    let mut merged = local.clone();
                    merged.record(entry.clone());
                    let _ = cache.store_include_manifest(module_id, key, &merged);
                }
                return Some(headers);
            }
        }
        None
    }

    /// Restore a node's outputs for one recorded header set, if every header
    /// in it still has the recorded content.
    fn restore_header_set(
        &self,
        module_id: &str,
        key: &CacheKey,
        entry: &[HeaderDigest],
        root: Option<&Path>,
        node: &BuildNode,
    ) -> Option<IncludedHeaders> {
        let mut headers = Vec::with_capacity(entry.len());
        for digest in entry {
            let path = resolve_header_path(&digest.path, root);
            let observed = self.observe_input(&path)?;
            if observed.hash != digest.hash {
                return None;
            }
            headers.push(observed);
        }
        self.try_cache_restore(module_id, &key.with_headers(entry), node)
            .then_some(headers)
    }

    /// Download the remote include manifest for `key` via `scratch`.
    fn fetch_remote_include_manifest(
        &self,
        module_id: &str,
        key: &CacheKey,
        scratch: &Path,
    ) -> Option<IncludeManifest> {
        let remote = self.remote_cache.as_ref()?;
        let fetched = remote
            .get(
                module_id,
                &key.include_manifest_key(),
                INCLUDE_MANIFEST_FILE,
                scratch,
            )
            .unwrap_or(false);
        let manifest = if fetched {
            fs::read_to_string(scratch)
                .ok()
                .and_then(|s| serde_json::from_str(&s).ok())
        } else {
            None
        };
        let _ = fs::remove_file(scratch);
        manifest
    }

    /// Add a compiled node's header set to the include manifest for `key`,
    /// locally and, when configured, in the remote cache (merged with the
    /// remote copy first, so other machines' sets are kept).
    fn record_include_manifest(
        &self,
        module_id: &str,
        key: &CacheKey,
        headers: Vec<HeaderDigest>,
        obj_output: &Path,
    ) {
        if self.no_cache {
            return;
        }
        let Some(ref cache) = self.cache else {
            return;
        };
        let mut manifest = cache
            .get_include_manifest(module_id, key)
            .unwrap_or_default();
        let scratch = depfile::depfile_path(obj_output, "remote-includes.json");
        if let Some(remote) = self.fetch_remote_include_manifest(module_id, key, &scratch) {
            manifest.merge(remote);
        }
        manifest.record(headers);

        let Ok(path) = cache.store_include_manifest(module_id, key, &manifest) else {
            return;
        };
        if let Some(ref remote) = self.remote_cache {
            let manifest_key = key.include_manifest_key();
            let _ = remote.put(module_id, &manifest_key, INCLUDE_MANIFEST_FILE, &path);
            let _ = remote.put(
                module_id,
                &manifest_key,
                "metadata.json",
                &path.with_file_name("metadata.json"),
            );
        }
    }

    /// Try to restore a node's outputs from cache. Returns true on hit.
    ///
    /// Checks local cache first. On local miss, tries the remote cache
    /// (if configured) and stores the downloaded artifact locally.
    fn try_cache_restore(&self, module_id: &str, key: &CacheKey, node: &BuildNode) -> bool {
        if self.no_cache {
            return false;
        }

        // Try local cache first
        if let Some(ref cache) = self.cache {
            if cache.has(module_id, key) {
                let mut all_found = true;
                for output in &node.outputs {
                    let artifact_name = output
                        .file_name()
                        .and_then(|f| f.to_str())
                        .unwrap_or("unknown");

                    match cache.get_artifact(module_id, key, artifact_name) {
                        Some(cached_path) => {
                            if let Some(parent) = output.parent() {
                                let _ = fs::create_dir_all(parent);
                            }
                            if fs::copy(&cached_path, output).is_err() {
                                all_found = false;
                                break;
                            }
                        }
                        None => {
                            all_found = false;
                            break;
                        }
                    }
                }
                if all_found {
                    return true;
                }
            }
        }

        // Try remote cache on local miss
        if let Some(ref remote) = self.remote_cache {
            // Fetch the entry's metadata first: downloaded artifacts are
            // verified against its per-artifact hashes so a truncated
            // server-side file (interrupted upload) is never used (#62).
            let metadata = node.outputs.first().and_then(|first| {
                let meta_path = first.with_file_name(".remote-metadata.json");
                let fetched = remote
                    .get(module_id, key, "metadata.json", &meta_path)
                    .unwrap_or(false);
                let parsed = if fetched {
                    std::fs::read_to_string(&meta_path)
                        .ok()
                        .and_then(|s| serde_json::from_str::<ArtifactMetadata>(&s).ok())
                } else {
                    None
                };
                let _ = std::fs::remove_file(&meta_path);
                parsed
            });

            if let Some(metadata) = metadata {
                let mut all_downloaded = true;
                for output in &node.outputs {
                    let artifact_name = output
                        .file_name()
                        .and_then(|f| f.to_str())
                        .unwrap_or("unknown");

                    match remote.get(module_id, key, artifact_name, output) {
                        Ok(true) => {
                            if !cmod_cache::artifact_matches_metadata(
                                &metadata,
                                artifact_name,
                                output,
                            ) {
                                self.emit_verbose(
                                    "Rejected",
                                    format!(
                                        "{}/{} failed hash verification; treating as miss",
                                        module_id, artifact_name
                                    ),
                                );
                                let _ = std::fs::remove_file(output);
                                all_downloaded = false;
                                break;
                            }
                            // Store locally for next time
                            if let Some(ref cache) = self.cache {
                                let name = artifact_name.to_string();
                                let _ = cache.store_single_artifact(module_id, key, &name, output);
                            }
                        }
                        _ => {
                            all_downloaded = false;
                            break;
                        }
                    }
                }
                if all_downloaded && !node.outputs.is_empty() {
                    return true;
                }
            }
        }

        false
    }

    /// Store a node's outputs into cache after successful compilation.
    ///
    /// Stores locally and, if a remote cache is configured for writes,
    /// pushes the artifacts upstream.
    fn cache_store(&self, module_id: &str, key: &CacheKey, node: &BuildNode) {
        if self.no_cache {
            return;
        }

        let mut artifact_entries = Vec::new();
        let mut artifact_files = Vec::new();

        for output in &node.outputs {
            let name = output
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or("unknown")
                .to_string();

            let hash = hash_file(output).unwrap_or_default();
            let size = fs::metadata(output).map(|m| m.len()).unwrap_or(0);

            artifact_entries.push(CachedArtifactEntry {
                name: name.clone(),
                hash,
                size,
            });
            artifact_files.push((name, output.clone()));
        }

        // Store locally
        if let Some(ref cache) = self.cache {
            let source_hash = node
                .source
                .as_ref()
                .and_then(|s| hash_file(s).ok())
                .unwrap_or_default();

            let metadata = ArtifactMetadata {
                module_name: module_id.to_string(),
                cache_key: key.to_string(),
                source_hash,
                compiler: self.backend.kind().to_string(),
                compiler_version: self.compiler_version().to_string(),
                target: self
                    .backend
                    .target()
                    .map(str::to_string)
                    .unwrap_or_default(),
                created_at: String::new(),
                artifacts: artifact_entries,
            };

            let file_refs: Vec<(&str, &Path)> = artifact_files
                .iter()
                .map(|(name, path)| (name.as_str(), path.as_path()))
                .collect();

            let _ = cache.store(module_id, key, &metadata, &file_refs);
        }

        // Push to remote cache if configured. Metadata goes last: restores
        // need it, so an entry is never visible before its artifacts are.
        if let Some(ref remote) = self.remote_cache {
            for (name, path) in &artifact_files {
                let _ = remote.put(module_id, key, name, path);
            }
            if let Some(ref cache) = self.cache {
                let meta_path = cache.entry_dir(module_id, key).join("metadata.json");
                if meta_path.exists() {
                    let _ = remote.put(module_id, key, "metadata.json", &meta_path);
                }
            }
        }
    }

    /// Attempt to execute a build node on a remote worker.
    /// Returns Ok(Some(outcome)) if distributed, Ok(None) if should fall back to local.
    fn try_distribute_node(
        &self,
        node: &crate::plan::BuildNode,
        project_root: &Path,
    ) -> Result<Option<NodeOutcome>, CmodError> {
        let pool = match &self.worker_pool {
            Some(p) => p,
            None => return Ok(None),
        };

        // Only distribute compilation nodes, not link nodes
        if !matches!(
            node.kind,
            cmod_core::types::NodeKind::Interface
                | cmod_core::types::NodeKind::Implementation
                | cmod_core::types::NodeKind::Object
        ) {
            return Ok(None);
        }

        let tasks =
            crate::distributed::nodes_to_remote_tasks(std::slice::from_ref(node), project_root);
        let task = match tasks.into_iter().next() {
            Some(t) => t,
            None => return Ok(None),
        };

        let task_id = task.task_id.clone();

        // Select a worker
        let worker_id = match pool.select_worker(&task) {
            Some(id) => id,
            None => {
                self.emit_verbose(
                    "Distributed",
                    format!("no available worker for {}", node.id),
                );
                return Ok(None); // Fall back to local
            }
        };

        // Submit task
        match pool.submit_task(&worker_id, task) {
            Ok(()) => {
                self.emit_verbose(
                    "Distributed",
                    format!("submitted {} to worker {}", node.id, worker_id),
                );
            }
            Err(e) => {
                self.emit_verbose(
                    "Distributed",
                    format!("failed to submit {}: {}, falling back to local", node.id, e),
                );
                return Ok(None); // Fall back to local
            }
        }

        // Poll for result
        let start = std::time::Instant::now();
        let timeout = std::time::Duration::from_secs(300);
        loop {
            if let Some(result) = pool.collect_result(&task_id) {
                if result.success {
                    // Materialize remote outputs locally so downstream
                    // link/cache steps can find the files.
                    let endpoint = pool.worker_endpoint(&worker_id).unwrap_or_default();
                    for remote_ref in &result.outputs {
                        // Match remote output path to the corresponding
                        // local node output by filename.
                        let remote_name = std::path::Path::new(&remote_ref.path)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or(&remote_ref.path);
                        let local_path = node
                            .outputs
                            .iter()
                            .find(|p| {
                                p.file_name()
                                    .and_then(|n| n.to_str())
                                    .map(|n| n == remote_name)
                                    .unwrap_or(false)
                            })
                            .cloned()
                            .unwrap_or_else(|| {
                                // Fallback: place the file relative to the
                                // first output's parent directory.
                                node.outputs
                                    .first()
                                    .and_then(|p| p.parent())
                                    .unwrap_or_else(|| std::path::Path::new("."))
                                    .join(remote_name)
                            });

                        if let Err(e) = pool.fetch_output(&endpoint, remote_ref, &local_path) {
                            self.emit_verbose(
                                "Distributed",
                                format!(
                                    "failed to fetch output '{}' for {}: {}, falling back to local",
                                    remote_ref.path, node.id, e
                                ),
                            );
                            return Ok(None);
                        }
                    }

                    self.emit_verbose(
                        "Distributed",
                        format!(
                            "{} completed on {} in {}ms ({} output(s) fetched)",
                            node.id,
                            worker_id,
                            result.duration_ms,
                            result.outputs.len()
                        ),
                    );
                    return Ok(Some(NodeOutcome::Compiled(result.duration_ms)));
                } else {
                    self.emit_verbose(
                        "Distributed",
                        format!("{} failed on {}: {}", node.id, worker_id, result.stderr),
                    );
                    return Ok(None); // Fall back to local on failure
                }
            }

            if start.elapsed() > timeout {
                self.emit_verbose(
                    "Distributed",
                    format!("{} timed out, falling back to local", node.id),
                );
                return Ok(None);
            }

            std::thread::sleep(std::time::Duration::from_millis(100));
        }
    }

    /// Execute a single compile/link node.
    ///
    /// The `build_state` and `flags_hash` enable incremental skip detection.
    /// If the node is unchanged since the last build, it is skipped without
    /// touching the cache at all.
    ///
    /// Also returns what the node was built against, for the next build's
    /// incremental check.
    fn execute_node(
        &self,
        node: &BuildNode,
        plan: &BuildPlan,
        pcm_map: &HashMap<String, PathBuf>,
        build_state: Option<&BuildState>,
        flags_hash: &str,
    ) -> Result<(NodeOutcome, NodeInputs), CmodError> {
        let start = Instant::now();

        // Attempt distributed compilation for non-link nodes when a worker pool is configured.
        if self.worker_pool.is_some()
            && matches!(
                node.kind,
                NodeKind::Interface | NodeKind::Implementation | NodeKind::Object
            )
        {
            let project_root = plan
                .build_dir
                .parent()
                .unwrap_or_else(|| std::path::Path::new("."));
            if let Some(outcome) = self.try_distribute_node(node, project_root)? {
                // Remote workers report no header list.
                return Ok((outcome, NodeInputs::default()));
            }
            // Fall through to local compilation
        }

        match node.kind {
            NodeKind::Interface | NodeKind::Implementation | NodeKind::Object => {
                let source = node.source.as_ref().unwrap();
                // Interface nodes output [BMI, object]; the others [object].
                let obj_output = node.outputs.last().unwrap();
                let label = match node.kind {
                    NodeKind::Interface => format!("interface: {}", source.display()),
                    NodeKind::Implementation => format!("impl: {}", source.display()),
                    _ => source.display().to_string(),
                };

                let dep_hashes = self.dep_output_hashes(node, plan);

                // Check incremental state first (cheapest check)
                if !self.force_rebuild {
                    if let Some(state) = build_state {
                        if state.needs_rebuild(node, flags_hash, &dep_hashes).is_none() {
                            self.emit_verbose("Up-to-date", source.display());
                            return Ok((
                                NodeOutcome::Skipped(start.elapsed().as_millis() as u64),
                                NodeInputs::default(),
                            ));
                        }
                    }
                }

                // Try cache next
                let base_key = self.compute_cache_key(node, plan, &dep_hashes);
                if let Some((module_id, key)) = &base_key {
                    if let Some(headers) = self.restore_from_cache(module_id, key, node) {
                        self.emit_verbose("Cached", &label);
                        return Ok((
                            NodeOutcome::CacheHit(start.elapsed().as_millis() as u64),
                            NodeInputs {
                                dep_hashes,
                                headers: Some(headers),
                            },
                        ));
                    }
                }

                // Try precompiled BMI from configured BMI directories
                if node.kind == NodeKind::Interface {
                    if let Some(module_name) = &node.module_name {
                        if let Some(variant_dir) = self.find_precompiled_bmi(module_name) {
                            if self.restore_bmi_from_dir(&variant_dir, node) {
                                self.emit_verbose("Precompiled", &label);
                                return Ok((
                                    NodeOutcome::CacheHit(start.elapsed().as_millis() as u64),
                                    NodeInputs {
                                        dep_hashes,
                                        headers: None,
                                    },
                                ));
                            }
                        }
                    }
                }

                // Pass all available PCMs — clang needs transitive module visibility
                // (e.g., when a module re-exports partitions via `export import :part;`)
                // Only pass PCMs that actually exist on disk to avoid races
                // where a parallel Interface node is still writing a PCM file.
                let all_pcms: Vec<(&str, &Path)> = pcm_map
                    .iter()
                    .filter(|(_, path)| path.exists())
                    .map(|(name, path)| (name.as_str(), path.as_path()))
                    .collect();

                for output in &node.outputs {
                    if let Some(parent) = output.parent() {
                        fs::create_dir_all(parent)?;
                    }
                }

                let compile_start = epoch_millis();
                if node.kind == NodeKind::Interface {
                    self.backend.compile_interface(
                        source,
                        &node.outputs[0],
                        obj_output,
                        &all_pcms,
                    )?;
                } else {
                    self.backend
                        .compile_implementation(source, obj_output, &all_pcms)?;
                }

                // Without the header list there is no correct key for the
                // artifacts, so they are not cached.
                let headers = self.hashed_headers(source, obj_output, compile_start);
                if let (Some((module_id, key)), Some((_, digests))) = (&base_key, &headers) {
                    self.cache_store(module_id, &key.with_headers(digests), node);
                    self.record_include_manifest(module_id, key, digests.clone(), obj_output);
                }

                self.emit("Compiled", &label);
                Ok((
                    NodeOutcome::Compiled(start.elapsed().as_millis() as u64),
                    NodeInputs {
                        dep_hashes,
                        headers: headers.map(|(included, _)| included),
                    },
                ))
            }

            NodeKind::Link => {
                let output = &node.outputs[0];
                let mut obj_files = plan.object_paths();
                // For static libraries, only archive the package's own objects.
                // Dependency objects/archives must not be nested inside this archive;
                // downstream consumers will link them separately.
                if plan.build_type != BuildType::StaticLib {
                    obj_files.extend(self.extra_obj_paths.clone());
                }

                // Skip linking when there are no objects (e.g., header-only packages)
                if obj_files.is_empty() {
                    return Ok((
                        NodeOutcome::Linked(start.elapsed().as_millis() as u64),
                        NodeInputs::default(),
                    ));
                }

                let obj_refs: Vec<&Path> = obj_files.iter().map(|p| p.as_path()).collect();

                if let Some(parent) = output.parent() {
                    fs::create_dir_all(parent)?;
                }

                let artifact = match plan.build_type {
                    BuildType::Binary => Artifact::Executable {
                        path: output.clone(),
                    },
                    BuildType::StaticLib => Artifact::StaticLib {
                        path: output.clone(),
                    },
                    BuildType::SharedLib => Artifact::SharedLib {
                        path: output.clone(),
                    },
                };

                self.backend.link(&obj_refs, output, &artifact)?;

                self.emit("Linked", output.display());
                Ok((
                    NodeOutcome::Linked(start.elapsed().as_millis() as u64),
                    NodeInputs::default(),
                ))
            }
        }
    }

    /// Hash of everything a link of `plan` reads: the objects (by the
    /// output hashes `state` recorded for the nodes that wrote them), the
    /// dependency objects and archives linked in, the configuration and the
    /// output path. `None` when an object's hash is unknown.
    fn link_key(&self, plan: &BuildPlan, state: &BuildState, flags_hash: &str) -> Option<String> {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(flags_hash.as_bytes());
        hasher.update(format!("{:?}", plan.build_type).as_bytes());
        for node in plan.nodes.iter().filter(|n| n.kind == NodeKind::Link) {
            for output in &node.outputs {
                hasher.update(output.to_string_lossy().as_bytes());
            }
        }
        for obj in plan.object_paths() {
            let name = obj.file_name()?.to_string_lossy();
            let hash = plan
                .nodes
                .iter()
                .find(|n| n.outputs.contains(&obj))
                .and_then(|n| state.nodes.get(&n.id))
                .and_then(|s| s.output_hashes.iter().find(|(n, _)| *n == name))
                .map(|(_, h)| h.clone())
                .filter(|h| !h.is_empty())?;
            hasher.update(obj.to_string_lossy().as_bytes());
            hasher.update(hash.as_bytes());
        }
        // Mirrors the Link arm of `execute_node`: static libraries archive
        // only the package's own objects.
        if plan.build_type != BuildType::StaticLib {
            for obj in &self.extra_obj_paths {
                hasher.update(obj.to_string_lossy().as_bytes());
                hasher.update(self.hash_input(obj)?.as_bytes());
            }
        }
        Some(format!("{:x}", hasher.finalize()))
    }

    /// Save `state` and run the link nodes, unless nothing they read changed
    /// since the last successful link and their outputs are still there.
    /// The link key is saved only after the link succeeds, so a failed link
    /// is retried. Returns the final output path.
    fn link_phase(
        &self,
        plan: &BuildPlan,
        link_nodes: &[&BuildNode],
        state: &mut BuildState,
        prev_state: &BuildState,
        flags_hash: &str,
    ) -> Result<PathBuf, CmodError> {
        if link_nodes.is_empty() {
            // Header-only package: nothing to link.
            let _ = state.save(&plan.build_dir);
            return Ok(PathBuf::new());
        }
        let final_output = link_nodes
            .last()
            .and_then(|n| n.outputs.first())
            .cloned()
            .unwrap_or_default();
        let key = self.link_key(plan, state, flags_hash);
        let up_to_date = !self.force_rebuild
            && key.is_some()
            && key == prev_state.link_key
            && link_nodes
                .iter()
                .all(|n| n.outputs.iter().all(|o| o.exists()));
        if up_to_date {
            self.emit_verbose("Up-to-date", final_output.display());
            state.link_key = key;
            let _ = state.save(&plan.build_dir);
            return Ok(final_output);
        }

        state.link_key = None;
        let _ = state.save(&plan.build_dir);
        let pcm_map: HashMap<String, PathBuf> = plan.pcm_paths().into_iter().collect();
        for node in link_nodes {
            self.execute_node(node, plan, &pcm_map, None, flags_hash)?;
        }
        state.link_key = key;
        let _ = state.save(&plan.build_dir);
        Ok(final_output)
    }

    /// Execute the build plan with parallel compilation.
    ///
    /// Uses a work-stealing scheduler: nodes whose dependencies are all
    /// complete are enqueued for execution across worker threads.
    /// The link node always runs last on the main thread.
    fn execute_plan(&self, plan: &BuildPlan) -> Result<(PathBuf, BuildStats), CmodError> {
        let wall_start = Instant::now();
        let jobs = self.effective_jobs();

        // Headers may have changed since a previous build by this runner.
        self.clear_file_hashes();

        // Load incremental build state
        let build_state = Arc::new(BuildState::load(&plan.build_dir));
        let flags_hash = Arc::new(self.flags_hash());

        // Separate compile nodes from the link node
        let (compile_nodes, link_nodes): (Vec<_>, Vec<_>) = plan
            .nodes
            .iter()
            .enumerate()
            .partition(|(_, n)| n.kind != NodeKind::Link);

        // Build a map of node_id → index for fast lookup
        let id_to_idx: HashMap<String, usize> = plan
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.clone(), i))
            .collect();

        // Compute PCM paths for dependency resolution during compilation.
        // Include extra PCMs from workspace dependencies.
        let mut pcm_map_inner: HashMap<String, PathBuf> = plan.pcm_paths().into_iter().collect();
        pcm_map_inner.extend(self.extra_pcm_paths.clone());
        let pcm_map: Arc<HashMap<String, PathBuf>> = Arc::new(pcm_map_inner);

        // For single-job or very small plans, use sequential execution
        if jobs <= 1 || compile_nodes.len() <= 1 {
            return self.execute_plan_sequential(plan);
        }

        // Parallel scheduler state
        let total = compile_nodes.len();
        let completed = Arc::new(AtomicUsize::new(0));
        let total_compile_ms = Arc::new(AtomicUsize::new(0));
        let cache_hits = Arc::new(AtomicUsize::new(0));
        let cache_misses = Arc::new(AtomicUsize::new(0));
        let incr_skipped = Arc::new(AtomicUsize::new(0));

        // In-degree tracking: how many deps each node is still waiting on
        // Only count deps that are compile nodes (not link)
        let mut in_degree: Vec<usize> = vec![0; plan.nodes.len()];
        let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); plan.nodes.len()];

        for (idx, node) in plan.nodes.iter().enumerate() {
            if node.kind == NodeKind::Link {
                continue;
            }
            for dep_id in &node.dependencies {
                if let Some(&dep_idx) = id_to_idx.get(dep_id) {
                    in_degree[idx] += 1;
                    dependents[dep_idx].push(idx);
                }
            }
        }

        // Protected mutable state
        let in_degree = Arc::new(Mutex::new(in_degree));
        let dependents = Arc::new(dependents);
        let errors: Arc<Mutex<Vec<CmodError>>> = Arc::new(Mutex::new(Vec::new()));
        let new_build_state: Arc<Mutex<BuildState>> = Arc::new(Mutex::new(BuildState::default()));
        let node_timings: Arc<Mutex<BTreeMap<String, u64>>> = Arc::new(Mutex::new(BTreeMap::new()));

        // Work channel: send ready node indices to workers
        let (work_tx, work_rx) = bounded::<usize>(total);

        // Enqueue initially ready compile nodes (in-degree == 0)
        {
            // Use .ok() to handle poisoned lock gracefully - if poisoned, we can't proceed
            let in_deg = match in_degree.lock() {
                Ok(guard) => guard,
                Err(poisoned) => poisoned.into_inner(),
            };
            for &(idx, _) in &compile_nodes {
                if in_deg[idx] == 0 {
                    let _ = work_tx.send(idx);
                }
            }
        }

        // Spawn worker threads
        //
        // Workers use recv_timeout to avoid deadlock: since workers hold
        // sender clones (needed to enqueue newly-ready nodes), a plain
        // recv() would block forever after all work is done. The timeout
        // lets workers check the completion count and exit gracefully.
        std::thread::scope(|scope| {
            let effective_workers = jobs.min(total);
            for _ in 0..effective_workers {
                let work_rx = work_rx.clone();
                let work_tx = work_tx.clone();
                let completed = Arc::clone(&completed);
                let total_compile_ms = Arc::clone(&total_compile_ms);
                let cache_hits = Arc::clone(&cache_hits);
                let cache_misses = Arc::clone(&cache_misses);
                let incr_skipped = Arc::clone(&incr_skipped);
                let in_degree = Arc::clone(&in_degree);
                let dependents = Arc::clone(&dependents);
                let errors = Arc::clone(&errors);
                let pcm_map = Arc::clone(&pcm_map);
                let new_build_state = Arc::clone(&new_build_state);
                let build_state = Arc::clone(&build_state);
                let flags_hash = Arc::clone(&flags_hash);
                let node_timings = Arc::clone(&node_timings);

                scope.spawn(move || {
                    loop {
                        // Check if all work is done
                        if completed.load(Ordering::SeqCst) >= total {
                            break;
                        }
                        // Check if there are errors — stop early
                        // Handle poisoned lock by recovering the inner data
                        let has_errors = match errors.lock() {
                            Ok(guard) => !guard.is_empty(),
                            Err(poisoned) => !poisoned.into_inner().is_empty(),
                        };
                        if has_errors {
                            break;
                        }

                        let idx = match work_rx.recv_timeout(std::time::Duration::from_millis(50)) {
                            Ok(idx) => idx,
                            Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
                            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                        };

                        let node = &plan.nodes[idx];
                        match self.execute_node(
                            node,
                            plan,
                            &pcm_map,
                            Some(&build_state),
                            &flags_hash,
                        ) {
                            Ok((outcome, inputs)) => {
                                let ms = outcome.time_ms();
                                total_compile_ms.fetch_add(ms as usize, Ordering::Relaxed);
                                // Handle poisoned locks gracefully by recovering inner data
                                match node_timings.lock() {
                                    Ok(mut guard) => {
                                        guard.insert(node.id.clone(), ms);
                                    }
                                    Err(poisoned) => {
                                        poisoned.into_inner().insert(node.id.clone(), ms);
                                    }
                                }
                                match outcome {
                                    NodeOutcome::CacheHit(_) => {
                                        cache_hits.fetch_add(1, Ordering::Relaxed);
                                        match new_build_state.lock() {
                                            Ok(mut guard) => guard.record_node(
                                                node,
                                                &flags_hash,
                                                &inputs.dep_hashes,
                                                inputs.headers.as_deref(),
                                            ),
                                            Err(poisoned) => poisoned.into_inner().record_node(
                                                node,
                                                &flags_hash,
                                                &inputs.dep_hashes,
                                                inputs.headers.as_deref(),
                                            ),
                                        }
                                    }
                                    NodeOutcome::Compiled(_) => {
                                        cache_misses.fetch_add(1, Ordering::Relaxed);
                                        match new_build_state.lock() {
                                            Ok(mut guard) => guard.record_node(
                                                node,
                                                &flags_hash,
                                                &inputs.dep_hashes,
                                                inputs.headers.as_deref(),
                                            ),
                                            Err(poisoned) => poisoned.into_inner().record_node(
                                                node,
                                                &flags_hash,
                                                &inputs.dep_hashes,
                                                inputs.headers.as_deref(),
                                            ),
                                        }
                                    }
                                    NodeOutcome::Skipped(_) => {
                                        incr_skipped.fetch_add(1, Ordering::Relaxed);
                                        match new_build_state.lock() {
                                            Ok(mut guard) => {
                                                guard.carry_over(&build_state, &node.id)
                                            }
                                            Err(poisoned) => poisoned
                                                .into_inner()
                                                .carry_over(&build_state, &node.id),
                                        }
                                    }
                                    NodeOutcome::Linked(_) => {}
                                }
                            }
                            Err(e) => match errors.lock() {
                                Ok(mut guard) => guard.push(e),
                                Err(poisoned) => poisoned.into_inner().push(e),
                            },
                        }

                        // Signal completion and enqueue newly-ready nodes
                        let c = completed.fetch_add(1, Ordering::SeqCst) + 1;
                        {
                            let mut in_deg = match in_degree.lock() {
                                Ok(guard) => guard,
                                Err(poisoned) => poisoned.into_inner(),
                            };
                            for &dep_idx in &dependents[idx] {
                                in_deg[dep_idx] -= 1;
                                if in_deg[dep_idx] == 0 {
                                    let _ = work_tx.send(dep_idx);
                                }
                            }
                        }
                        let _ = c;
                    }
                    drop(work_tx);
                });
            }
            // Drop sender on main thread so workers can detect disconnection
            drop(work_tx);
        });

        // Check for errors - handle poisoned lock
        let errs = match errors.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if let Some(first) = errs.first() {
            return Err(CmodError::BuildFailed {
                reason: format!("{}", first),
            });
        }
        drop(errs);

        // Save the new build state with node timings, then link on the
        // main thread - handle poisoned locks
        let mut final_state = match new_build_state.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        };
        final_state.node_timings = match node_timings.lock() {
            Ok(guard) => guard.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        };
        let link_nodes: Vec<&BuildNode> = link_nodes.iter().map(|&(_, n)| n).collect();
        let final_output = self.link_phase(
            plan,
            &link_nodes,
            &mut final_state,
            &build_state,
            &flags_hash,
        )?;

        let stats = BuildStats {
            cache_hits: cache_hits.load(Ordering::Relaxed),
            cache_misses: cache_misses.load(Ordering::Relaxed),
            skipped: link_nodes.len(),
            incremental_skipped: incr_skipped.load(Ordering::Relaxed),
            wall_time_ms: wall_start.elapsed().as_millis() as u64,
            total_compile_time_ms: total_compile_ms.load(Ordering::Relaxed) as u64,
            node_timings: Arc::try_unwrap(node_timings)
                .unwrap_or_default()
                .into_inner()
                .unwrap_or_default(),
        };

        Ok((final_output, stats))
    }

    /// Sequential fallback for single-job mode or trivial plans.
    fn execute_plan_sequential(
        &self,
        plan: &BuildPlan,
    ) -> Result<(PathBuf, BuildStats), CmodError> {
        let wall_start = Instant::now();
        self.clear_file_hashes();
        let build_state = BuildState::load(&plan.build_dir);
        let flags_hash = self.flags_hash();
        let mut pcm_map: HashMap<String, PathBuf> = plan.pcm_paths().into_iter().collect();
        pcm_map.extend(self.extra_pcm_paths.clone());
        let mut new_state = BuildState::default();
        let mut stats = BuildStats::default();
        let mut link_nodes = Vec::new();

        for node in &plan.nodes {
            if node.kind == NodeKind::Link {
                link_nodes.push(node);
                continue;
            }
            let (outcome, inputs) =
                self.execute_node(node, plan, &pcm_map, Some(&build_state), &flags_hash)?;
            let ms = outcome.time_ms();
            stats.node_timings.insert(node.id.clone(), ms);
            match outcome {
                NodeOutcome::CacheHit(ms) => {
                    stats.cache_hits += 1;
                    stats.total_compile_time_ms += ms;
                    new_state.record_node(
                        node,
                        &flags_hash,
                        &inputs.dep_hashes,
                        inputs.headers.as_deref(),
                    );
                }
                NodeOutcome::Compiled(ms) => {
                    stats.cache_misses += 1;
                    stats.total_compile_time_ms += ms;
                    new_state.record_node(
                        node,
                        &flags_hash,
                        &inputs.dep_hashes,
                        inputs.headers.as_deref(),
                    );
                }
                NodeOutcome::Skipped(ms) => {
                    stats.incremental_skipped += 1;
                    stats.total_compile_time_ms += ms;
                    // Preserve existing state for skipped nodes
                    new_state.carry_over(&build_state, &node.id);
                }
                NodeOutcome::Linked(ms) => {
                    stats.total_compile_time_ms += ms;
                }
            }
        }

        // Save updated build state with node timings, and link
        new_state.node_timings = stats.node_timings.clone();
        stats.skipped += link_nodes.len();
        let final_output =
            self.link_phase(plan, &link_nodes, &mut new_state, &build_state, &flags_hash)?;

        stats.wall_time_ms = wall_start.elapsed().as_millis() as u64;
        Ok((final_output, stats))
    }
}

/// The current time as epoch milliseconds, the unit of recorded mtimes.
fn epoch_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as u64)
}

/// The directory of the nearest `cmod.toml` above `source`: header paths
/// inside it are cached relative to it, so checkouts in different places
/// share cache entries.
fn package_root(source: &Path) -> Option<PathBuf> {
    source
        .ancestors()
        .skip(1)
        .find(|dir| dir.join("cmod.toml").is_file())
        .map(Path::to_path_buf)
}

/// A header path as recorded in an include manifest: relative to `root`
/// (with `/` separators) when inside it, absolute otherwise.
fn portable_header_path(path: &Path, root: Option<&Path>) -> String {
    match root.and_then(|root| path.strip_prefix(root).ok()) {
        Some(relative) => relative
            .components()
            .map(|c| c.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
        None => path.to_string_lossy().into_owned(),
    }
}

/// Inverse of [`portable_header_path`].
fn resolve_header_path(path: &str, root: Option<&Path>) -> PathBuf {
    let path = Path::new(path);
    match root {
        Some(root) if path.is_relative() => root.join(path),
        _ => path.to_path_buf(),
    }
}

/// Discover C/C++ module source files in a directory.
///
/// Looks for `.cppm`, `.ixx`, `.mpp` (module interface), `.cpp`, `.cc`, `.cxx`,
/// and `.c` (plain C) files. Plain C files are compiled as C++ by clang++.
pub fn discover_sources(src_dir: &Path) -> Result<Vec<PathBuf>, CmodError> {
    let mut sources = Vec::new();

    if !src_dir.exists() {
        return Ok(sources);
    }

    for entry in walkdir::WalkDir::new(src_dir)
        .into_iter()
        .filter_map(|e| e.ok())
    {
        let path = entry.path();
        if path.is_file() {
            if let Some("cppm" | "ixx" | "mpp" | "cpp" | "cc" | "cxx" | "c") =
                path.extension().and_then(|e| e.to_str())
            {
                sources.push(path.to_path_buf());
            }
        }
    }

    sources.sort();
    Ok(sources)
}

/// Discover C++ source files from multiple directories, applying exclude patterns.
///
/// Each `src_dir` is walked recursively. Files matching any `exclude` glob pattern
/// (checked against both the relative path within the source dir and the filename)
/// are omitted. Results are sorted and deduplicated.
pub fn discover_sources_multi(
    src_dirs: &[PathBuf],
    exclude: &[String],
) -> Result<Vec<PathBuf>, CmodError> {
    let patterns: Vec<glob::Pattern> = exclude
        .iter()
        .map(|p| {
            glob::Pattern::new(p)
                .map_err(|e| CmodError::Other(format!("invalid exclude pattern '{}': {}", p, e)))
        })
        .collect::<Result<Vec<_>, _>>()?;

    let mut sources = Vec::new();

    for src_dir in src_dirs {
        if !src_dir.exists() {
            continue;
        }

        for entry in walkdir::WalkDir::new(src_dir)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if !path.is_file() {
                continue;
            }
            if !matches!(
                path.extension().and_then(|e| e.to_str()),
                Some("cppm" | "ixx" | "mpp" | "cpp" | "cc" | "cxx" | "c")
            ) {
                continue;
            }

            // Check exclude patterns against relative path and filename
            let rel_path = path.strip_prefix(src_dir).unwrap_or(path);
            let rel_str = rel_path.to_string_lossy();
            let filename = path
                .file_name()
                .map(|f| f.to_string_lossy())
                .unwrap_or_default();

            let excluded = patterns
                .iter()
                .any(|pat| pat.matches(&rel_str) || pat.matches(&filename));

            if !excluded {
                sources.push(path.to_path_buf());
            }
        }
    }

    sources.sort();
    sources.dedup();
    Ok(sources)
}

/// Classify a source file as a module interface or implementation based on content.
///
/// Scans the entire file for module declarations, skipping comments and
/// preprocessor blocks. Handles the global module fragment (`module;`).
pub fn classify_source(path: &Path) -> Result<cmod_core::types::ModuleUnitKind, CmodError> {
    let content = fs::read_to_string(path)?;

    let mut in_block_comment = false;

    for line in content.lines() {
        let trimmed = line.trim();

        // Handle block comments
        if in_block_comment {
            if let Some(pos) = trimmed.find("*/") {
                // Rest of the line after the block comment end
                let rest = trimmed[pos + 2..].trim();
                in_block_comment = false;
                if rest.is_empty() {
                    continue;
                }
                // Process the rest below
                return classify_line(rest);
            }
            continue;
        }

        // Skip empty lines
        if trimmed.is_empty() {
            continue;
        }

        // Skip line comments
        if trimmed.starts_with("//") {
            continue;
        }

        // Start of block comment
        if trimmed.starts_with("/*") {
            if !trimmed.contains("*/") {
                in_block_comment = true;
            }
            continue;
        }

        // Skip preprocessor directives
        if trimmed.starts_with('#') {
            continue;
        }

        // Global module fragment: `module;` is NOT a module declaration —
        // it's the start of a global module fragment. Continue scanning.
        if trimmed == "module;" {
            continue;
        }

        if let Ok(kind) = classify_line(trimmed) {
            return Ok(kind);
        }
    }

    // No module declaration found — treat as legacy TU
    Ok(cmod_core::types::ModuleUnitKind::LegacyUnit)
}

/// Classify a single non-comment, non-empty line.
fn classify_line(trimmed: &str) -> Result<cmod_core::types::ModuleUnitKind, CmodError> {
    if trimmed.starts_with("export module") {
        if trimmed.contains(':') {
            return Ok(cmod_core::types::ModuleUnitKind::PartitionUnit);
        }
        return Ok(cmod_core::types::ModuleUnitKind::InterfaceUnit);
    }
    // `module foo;` — implementation unit (but not `module;` which is global module fragment)
    if trimmed.starts_with("module ") {
        let rest = trimmed.strip_prefix("module").unwrap().trim();
        if !rest.is_empty() && rest.ends_with(';') {
            return Ok(cmod_core::types::ModuleUnitKind::ImplementationUnit);
        }
    }
    Err(CmodError::Other("not a module declaration".to_string()))
}

/// Extract the module name from an `export module ...;` or `module ...;` declaration.
///
/// Skips comments and the global module fragment (`module;`).
pub fn extract_module_name(path: &Path) -> Result<Option<String>, CmodError> {
    let content = fs::read_to_string(path)?;
    extract_module_name_from_content(&content)
}

/// Extract module name from source content (testable without filesystem).
pub fn extract_module_name_from_content(content: &str) -> Result<Option<String>, CmodError> {
    let mut in_block_comment = false;

    for line in content.lines() {
        let trimmed = line.trim();

        // Handle block comments
        if in_block_comment {
            if trimmed.contains("*/") {
                in_block_comment = false;
            }
            continue;
        }

        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('#') {
            continue;
        }

        if trimmed.starts_with("/*") {
            if !trimmed.contains("*/") {
                in_block_comment = true;
            }
            continue;
        }

        // Skip global module fragment
        if trimmed == "module;" {
            continue;
        }

        if trimmed.starts_with("export module") || trimmed.starts_with("module ") {
            let decl = trimmed
                .trim_start_matches("export")
                .trim()
                .trim_start_matches("module")
                .trim()
                .trim_end_matches(';')
                .trim();
            if !decl.is_empty() {
                return Ok(Some(decl.to_string()));
            }
        }
    }

    Ok(None)
}

/// Extract import names from source content by scanning for `import` and
/// `export import` lines.
///
/// Returns the imported module (or partition) names, e.g. `["base", ":detail"]`.
/// Skips comments, the global module fragment, and module declarations.
pub fn extract_imports_from_content(content: &str) -> Vec<String> {
    let mut imports = Vec::new();
    let mut in_block_comment = false;

    for line in content.lines() {
        let trimmed = line.trim();

        if in_block_comment {
            if trimmed.contains("*/") {
                in_block_comment = false;
            }
            continue;
        }

        if trimmed.is_empty() || trimmed.starts_with("//") || trimmed.starts_with('#') {
            continue;
        }

        if trimmed.starts_with("/*") {
            if !trimmed.contains("*/") {
                in_block_comment = true;
            }
            continue;
        }

        // Handle optional "export" prefix
        let after_export = trimmed
            .strip_prefix("export")
            .map(|s| s.trim())
            .unwrap_or(trimmed);

        if let Some(after_import) = after_export.strip_prefix("import") {
            // Skip module declarations that happen to start with "import"
            // (shouldn't occur in valid C++20, but be safe)
            let name = after_import.trim().trim_end_matches(';').trim();
            if !name.is_empty() {
                imports.push(name.to_string());
            }
        }
    }

    imports
}

/// Extract imports from a source file on disk.
pub fn extract_imports(path: &Path) -> Result<Vec<String>, CmodError> {
    let content = fs::read_to_string(path)?;
    Ok(extract_imports_from_content(&content))
}

/// Extract the owning module for a partition declaration.
///
/// For `export module foo:bar;`, returns `Some("foo")`.
/// For `export module foo;` or non-partition TUs, returns `None`.
pub fn extract_partition_owner(path: &Path) -> Result<Option<String>, CmodError> {
    let content = fs::read_to_string(path)?;
    if let Some(module_name) = extract_module_name_from_content(&content)? {
        if module_name.contains(':') {
            // "foo:bar" → owner is "foo"
            return Ok(module_name.split(':').next().map(|s| s.to_string()));
        }
    }
    Ok(None)
}

/// Filter out source files that are `#include`d by module interface/partition files.
///
/// Some C++ modules (e.g., fmtlib) `#include` implementation `.cc`/`.cpp` files
/// inside the module interface's private fragment. If those files are also discovered
/// as standalone translation units, they get compiled twice — producing duplicate
/// symbols at link time. This function detects such includes and removes them.
pub fn filter_included_sources(sources: &[PathBuf]) -> Vec<PathBuf> {
    use std::collections::HashSet;

    // Identify interface/partition source files and collect their #include'd sources.
    // Resolve each include path relative to the including file's directory so that
    // e.g. src/foo/detail.cpp and src/bar/detail.cpp are distinguished correctly.
    let mut included_paths: HashSet<PathBuf> = HashSet::new();

    for source in sources {
        let kind = match classify_source(source) {
            Ok(k) => k,
            Err(_) => continue,
        };

        // Only scan interface and partition units for #include directives
        if !matches!(
            kind,
            cmod_core::types::ModuleUnitKind::InterfaceUnit
                | cmod_core::types::ModuleUnitKind::PartitionUnit
        ) {
            continue;
        }

        let content = match fs::read_to_string(source) {
            Ok(c) => c,
            Err(_) => continue,
        };

        let source_dir = source.parent().unwrap_or(Path::new("."));

        for line in content.lines() {
            let trimmed = line.trim();
            // Match #include "file.cc", #include "file.cpp", #include "file.cxx"
            if let Some(rest) = trimmed.strip_prefix("#include") {
                let rest = rest.trim();
                if let Some(quoted) = rest.strip_prefix('"') {
                    if let Some(filename) = quoted.strip_suffix('"') {
                        let filename = filename.trim();
                        if filename.ends_with(".cc")
                            || filename.ends_with(".cpp")
                            || filename.ends_with(".cxx")
                        {
                            // Resolve relative to the including file's directory
                            let resolved = source_dir.join(filename);
                            // Canonicalize to normalize ../components; fall back to joined path
                            let normalized = resolved.canonicalize().unwrap_or(resolved);
                            included_paths.insert(normalized);
                        }
                    }
                }
            }
        }
    }

    if included_paths.is_empty() {
        return sources.to_vec();
    }

    sources
        .iter()
        .filter(|source| {
            let normalized = source
                .canonicalize()
                .unwrap_or_else(|_| source.to_path_buf());
            !included_paths.contains(&normalized)
        })
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_discover_sources() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        fs::create_dir_all(&src).unwrap();

        fs::write(src.join("lib.cppm"), "export module mylib;").unwrap();
        fs::write(src.join("impl.cpp"), "module mylib;").unwrap();
        fs::write(src.join("readme.txt"), "not a source").unwrap();

        let sources = discover_sources(&src).unwrap();
        assert_eq!(sources.len(), 2);
    }

    #[test]
    fn test_classify_interface() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("test.cppm");
        fs::write(&file, "export module foo.bar;\n\nvoid hello();").unwrap();

        let kind = classify_source(&file).unwrap();
        assert_eq!(kind, cmod_core::types::ModuleUnitKind::InterfaceUnit);
    }

    #[test]
    fn test_classify_partition() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("test.cppm");
        fs::write(&file, "export module foo:detail;").unwrap();

        let kind = classify_source(&file).unwrap();
        assert_eq!(kind, cmod_core::types::ModuleUnitKind::PartitionUnit);
    }

    #[test]
    fn test_classify_implementation() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("test.cpp");
        fs::write(&file, "module foo;\n\nvoid impl() {}").unwrap();

        let kind = classify_source(&file).unwrap();
        assert_eq!(kind, cmod_core::types::ModuleUnitKind::ImplementationUnit);
    }

    #[test]
    fn test_classify_legacy() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("test.cpp");
        fs::write(&file, "#include <iostream>\nint main() {}").unwrap();

        let kind = classify_source(&file).unwrap();
        assert_eq!(kind, cmod_core::types::ModuleUnitKind::LegacyUnit);
    }

    #[test]
    fn test_extract_module_name() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("test.cppm");
        fs::write(&file, "export module github.fmtlib.fmt;\n").unwrap();

        let name = extract_module_name(&file).unwrap();
        assert_eq!(name, Some("github.fmtlib.fmt".to_string()));
    }

    #[test]
    fn test_extract_partition_name() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("test.cppm");
        fs::write(&file, "export module foo:detail;\n").unwrap();

        let name = extract_module_name(&file).unwrap();
        assert_eq!(name, Some("foo:detail".to_string()));
    }

    #[test]
    fn test_extract_module_name_none_for_legacy() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("test.cpp");
        fs::write(&file, "#include <iostream>\nint main() {}\n").unwrap();

        let name = extract_module_name(&file).unwrap();
        assert_eq!(name, None);
    }

    #[test]
    fn test_extract_module_name_impl_unit() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("test.cpp");
        fs::write(&file, "module mylib;\nvoid impl() {}\n").unwrap();

        let name = extract_module_name(&file).unwrap();
        assert_eq!(name, Some("mylib".to_string()));
    }

    #[test]
    fn test_discover_sources_empty_dir() {
        let tmp = TempDir::new().unwrap();
        let sources = discover_sources(tmp.path()).unwrap();
        assert!(sources.is_empty());
    }

    #[test]
    fn test_discover_sources_nonexistent_dir() {
        let sources = discover_sources(Path::new("/nonexistent/path")).unwrap();
        assert!(sources.is_empty());
    }

    #[test]
    fn test_discover_sources_nested() {
        let tmp = TempDir::new().unwrap();
        let sub = tmp.path().join("sub");
        fs::create_dir_all(&sub).unwrap();

        fs::write(tmp.path().join("top.cppm"), "export module top;").unwrap();
        fs::write(sub.join("nested.cpp"), "module top;").unwrap();

        let sources = discover_sources(tmp.path()).unwrap();
        assert_eq!(sources.len(), 2);
    }

    #[test]
    fn test_discover_sources_all_extensions() {
        let tmp = TempDir::new().unwrap();
        fs::write(tmp.path().join("a.cppm"), "").unwrap();
        fs::write(tmp.path().join("b.ixx"), "").unwrap();
        fs::write(tmp.path().join("c.mpp"), "").unwrap();
        fs::write(tmp.path().join("d.cpp"), "").unwrap();
        fs::write(tmp.path().join("e.cc"), "").unwrap();
        fs::write(tmp.path().join("f.cxx"), "").unwrap();
        fs::write(tmp.path().join("g.h"), "").unwrap(); // should be excluded
        fs::write(tmp.path().join("h.txt"), "").unwrap(); // should be excluded

        let sources = discover_sources(tmp.path()).unwrap();
        assert_eq!(sources.len(), 6);
    }

    #[test]
    fn test_build_stats_default() {
        let stats = BuildStats::default();
        assert_eq!(stats.cache_hits, 0);
        assert_eq!(stats.cache_misses, 0);
        assert_eq!(stats.skipped, 0);
        assert_eq!(stats.incremental_skipped, 0);
        assert_eq!(stats.wall_time_ms, 0);
        assert_eq!(stats.total_compile_time_ms, 0);
    }

    /// A compiled TU with one header, and a depfile naming it, as the
    /// compiler would have left them.
    fn compiled_with_header(tmp: &TempDir) -> (BuildRunner, PathBuf, PathBuf, PathBuf) {
        let src = tmp.path().join("main.cpp");
        let header = tmp.path().join("value.h");
        let obj = tmp.path().join("main.o");
        fs::write(&src, "#include \"value.h\"\n").unwrap();
        fs::write(&header, "#define VALUE 1\n").unwrap();
        fs::write(
            depfile::depfile_path(&obj, "d"),
            format!(
                "{}: {} {}\n",
                obj.display(),
                src.display(),
                header.display()
            ),
        )
        .unwrap();
        let backend = crate::compiler::ClangBackend::new("20", cmod_core::types::Profile::Debug);
        (BuildRunner::new(Box::new(backend), None), src, header, obj)
    }

    #[test]
    fn test_hashed_headers_from_depfile() {
        let tmp = TempDir::new().unwrap();
        let (runner, src, header, obj) = compiled_with_header(&tmp);
        let (included, digests) = runner
            .hashed_headers(&src, &obj, epoch_millis() + 60_000)
            .unwrap();
        assert_eq!(included.len(), 1);
        assert_eq!(included[0].path, header);
        assert_eq!(included[0].mtime, file_mtime(&header));
        assert_eq!(digests[0].hash, hash_file(&header).unwrap());
    }

    /// A header newer than the compile may have changed under it: no header
    /// list, so the object is neither cached nor trusted next build.
    #[test]
    fn test_hashed_headers_rejects_header_written_during_compile() {
        let tmp = TempDir::new().unwrap();
        let (runner, src, _, obj) = compiled_with_header(&tmp);
        assert!(runner.hashed_headers(&src, &obj, 0).is_none());
    }

    /// A header hashed earlier in the build (say, for another node's cache
    /// lookup) and edited since: the memoized hash is not what this compile
    /// read.
    #[test]
    fn test_hashed_headers_rejects_header_changed_since_hashed() {
        let tmp = TempDir::new().unwrap();
        let (runner, src, header, obj) = compiled_with_header(&tmp);
        runner.observe_input(&header).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(20));
        fs::write(&header, "#define VALUE 2\n").unwrap();
        assert!(runner
            .hashed_headers(&src, &obj, epoch_millis() + 60_000)
            .is_none());
    }

    #[test]
    fn test_hashed_headers_without_depfile_is_unknown() {
        let tmp = TempDir::new().unwrap();
        let (runner, src, _, obj) = compiled_with_header(&tmp);
        fs::remove_file(depfile::depfile_path(&obj, "d")).unwrap();
        assert!(runner
            .hashed_headers(&src, &obj, epoch_millis() + 60_000)
            .is_none());
    }

    #[test]
    fn test_portable_header_path_roundtrip() {
        let proj = TempDir::new().unwrap();
        let other = TempDir::new().unwrap();
        let root = proj.path();
        let inside = root.join("include").join("value.h");
        let outside = other.path().join("stdio.h");

        let portable = portable_header_path(&inside, Some(root));
        assert_eq!(portable, "include/value.h");
        assert_eq!(resolve_header_path(&portable, Some(root)), inside);
        // A checkout elsewhere resolves the same relative path.
        assert_eq!(
            resolve_header_path(&portable, Some(other.path())),
            other.path().join("include").join("value.h")
        );
        // Outside the package: absolute, unchanged.
        let portable = portable_header_path(&outside, Some(root));
        assert_eq!(resolve_header_path(&portable, Some(root)), outside);
    }

    /// Same flags, different compiler executable: incremental state and
    /// link keys must not carry over.
    #[test]
    fn test_flags_hash_covers_compiler_executable() {
        let make = |path: &str| {
            let mut backend =
                crate::compiler::ClangBackend::new("20", cmod_core::types::Profile::Debug);
            backend.clang_path = PathBuf::from(path);
            BuildRunner::new(Box::new(backend), None)
        };
        let a = make("/opt/llvm-17/bin/clang++");
        let b = make("/opt/llvm-18/bin/clang++");
        assert_eq!(
            a.flags_hash(),
            make("/opt/llvm-17/bin/clang++").flags_hash()
        );
        assert_ne!(a.flags_hash(), b.flags_hash());
    }

    #[test]
    fn test_effective_jobs_auto() {
        let backend = crate::compiler::ClangBackend::new("20", cmod_core::types::Profile::Debug);
        let runner = BuildRunner::new(Box::new(backend), None);
        // auto-detect should be at least 1
        assert!(runner.effective_jobs() >= 1);
    }

    #[test]
    fn test_effective_jobs_explicit() {
        let backend = crate::compiler::ClangBackend::new("20", cmod_core::types::Profile::Debug);
        let runner = BuildRunner::new(Box::new(backend), None).with_jobs(4);
        assert_eq!(runner.effective_jobs(), 4);
    }

    #[test]
    fn test_node_outcome_time() {
        assert_eq!(NodeOutcome::CacheHit(42).time_ms(), 42);
        assert_eq!(NodeOutcome::Compiled(100).time_ms(), 100);
        assert_eq!(NodeOutcome::Linked(7).time_ms(), 7);
        assert_eq!(NodeOutcome::Skipped(3).time_ms(), 3);
    }

    #[test]
    fn test_classify_module_preamble_with_comments() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("test.cppm");
        // Module declaration should be found even with leading comments
        fs::write(
            &file,
            "// Copyright 2024\n// License: MIT\nexport module mymod;\n",
        )
        .unwrap();

        let kind = classify_source(&file).unwrap();
        assert_eq!(kind, cmod_core::types::ModuleUnitKind::InterfaceUnit);
    }

    // ── Phase 1.5: Robust classification tests ─────────────────

    #[test]
    fn test_classify_after_block_comment() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("test.cppm");
        fs::write(
            &file,
            "/* This is a long\n * multi-line comment\n * that goes on\n */\nexport module mymod;\n",
        )
        .unwrap();

        let kind = classify_source(&file).unwrap();
        assert_eq!(kind, cmod_core::types::ModuleUnitKind::InterfaceUnit);
    }

    #[test]
    fn test_classify_global_module_fragment() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("test.cppm");
        // Global module fragment pattern: `module;` followed by `export module`
        fs::write(&file, "module;\n#include <cassert>\nexport module mymod;\n").unwrap();

        let kind = classify_source(&file).unwrap();
        assert_eq!(kind, cmod_core::types::ModuleUnitKind::InterfaceUnit);
    }

    #[test]
    fn test_classify_after_preprocessor_blocks() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("test.cppm");
        fs::write(
            &file,
            "#pragma once\n#ifdef __linux__\n#endif\nexport module mymod;\n",
        )
        .unwrap();

        let kind = classify_source(&file).unwrap();
        assert_eq!(kind, cmod_core::types::ModuleUnitKind::InterfaceUnit);
    }

    #[test]
    fn test_classify_module_after_many_comment_lines() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("test.cppm");
        // 100 comment lines followed by the module declaration
        let mut content = String::new();
        for i in 0..100 {
            content.push_str(&format!("// Comment line {}\n", i));
        }
        content.push_str("export module mymod;\n");
        fs::write(&file, &content).unwrap();

        let kind = classify_source(&file).unwrap();
        assert_eq!(kind, cmod_core::types::ModuleUnitKind::InterfaceUnit);
    }

    #[test]
    fn test_extract_partition_owner() {
        let tmp = TempDir::new().unwrap();

        // Partition
        let file = tmp.path().join("ops.cppm");
        fs::write(&file, "export module math:ops;\n").unwrap();
        let owner = extract_partition_owner(&file).unwrap();
        assert_eq!(owner, Some("math".to_string()));

        // Non-partition
        let file2 = tmp.path().join("math.cppm");
        fs::write(&file2, "export module math;\n").unwrap();
        let owner2 = extract_partition_owner(&file2).unwrap();
        assert_eq!(owner2, None);

        // Legacy
        let file3 = tmp.path().join("main.cpp");
        fs::write(&file3, "int main() {}\n").unwrap();
        let owner3 = extract_partition_owner(&file3).unwrap();
        assert_eq!(owner3, None);
    }

    #[test]
    fn test_extract_module_name_from_content() {
        assert_eq!(
            extract_module_name_from_content("export module foo.bar;\n").unwrap(),
            Some("foo.bar".to_string())
        );
        assert_eq!(
            extract_module_name_from_content("module impl_mod;\nvoid f() {}\n").unwrap(),
            Some("impl_mod".to_string())
        );
        assert_eq!(
            extract_module_name_from_content("// comment\nmodule;\nexport module real;\n").unwrap(),
            Some("real".to_string())
        );
        assert_eq!(
            extract_module_name_from_content("int main() {}\n").unwrap(),
            None
        );
    }

    #[test]
    fn test_discover_sources_multi_single_dir() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("lib.cppm"), "export module mylib;").unwrap();
        fs::write(src.join("main.cpp"), "int main() {}").unwrap();

        let sources = discover_sources_multi(&[src], &[]).unwrap();
        assert_eq!(sources.len(), 2);
    }

    #[test]
    fn test_discover_sources_multi_dirs() {
        let tmp = TempDir::new().unwrap();
        let dir_a = tmp.path().join("dirA");
        let dir_b = tmp.path().join("dirB");
        fs::create_dir_all(&dir_a).unwrap();
        fs::create_dir_all(&dir_b).unwrap();
        fs::write(dir_a.join("a.cppm"), "export module a;").unwrap();
        fs::write(dir_b.join("b.cpp"), "int b() {}").unwrap();

        let sources = discover_sources_multi(&[dir_a, dir_b], &[]).unwrap();
        assert_eq!(sources.len(), 2);
    }

    #[test]
    fn test_discover_sources_multi_exclude_filename() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("lib.cpp"), "void lib() {}").unwrap();
        fs::write(src.join("lib_test.cc"), "void test() {}").unwrap();
        fs::write(src.join("other_test.cc"), "void test2() {}").unwrap();

        let sources = discover_sources_multi(&[src], &["*_test.cc".to_string()]).unwrap();
        assert_eq!(sources.len(), 1);
        assert!(sources[0].to_str().unwrap().contains("lib.cpp"));
    }

    #[test]
    fn test_discover_sources_multi_exclude_dir_pattern() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        let test_dir = src.join("test");
        fs::create_dir_all(&test_dir).unwrap();
        fs::write(src.join("lib.cpp"), "void lib() {}").unwrap();
        fs::write(test_dir.join("check.cpp"), "void check() {}").unwrap();

        let sources = discover_sources_multi(&[src], &["test/**".to_string()]).unwrap();
        assert_eq!(sources.len(), 1);
        assert!(sources[0].to_str().unwrap().contains("lib.cpp"));
    }

    #[test]
    fn test_discover_sources_multi_nonexistent_dir() {
        let sources = discover_sources_multi(&[PathBuf::from("/nonexistent/path")], &[]).unwrap();
        assert!(sources.is_empty());
    }

    #[test]
    fn test_discover_sources_multi_dedup() {
        let tmp = TempDir::new().unwrap();
        let src = tmp.path().join("src");
        fs::create_dir_all(&src).unwrap();
        fs::write(src.join("lib.cpp"), "void lib() {}").unwrap();

        // Pass the same dir twice — should deduplicate
        let sources = discover_sources_multi(&[src.clone(), src], &[]).unwrap();
        assert_eq!(sources.len(), 1);
    }

    #[test]
    fn test_extract_imports_from_content() {
        let content = "\
export module mylib;
import base;
export import utils;
import :detail;
// import commented_out;
";
        let imports = extract_imports_from_content(content);
        assert_eq!(imports, vec!["base", "utils", ":detail"]);
    }

    #[test]
    fn test_extract_imports_no_semicolon() {
        let content = "import base\nimport utils;\n";
        let imports = extract_imports_from_content(content);
        assert_eq!(imports, vec!["base", "utils"]);
    }

    #[test]
    fn test_extract_imports_empty() {
        let content = "int main() { return 0; }\n";
        let imports = extract_imports_from_content(content);
        assert!(imports.is_empty());
    }
}
