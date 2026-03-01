use std::collections::{BTreeMap, HashMap};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

use crossbeam_channel::bounded;

use cmod_cache::cache::{ArtifactCache, ArtifactMetadata, CachedArtifactEntry};
use cmod_cache::key::{hash_file, CacheKey, CacheKeyInputs};
use cmod_core::error::CmodError;
use cmod_core::types::{Artifact, BuildType, NodeKind, Profile};

use crate::compiler::{ClangBackend, CompilerBackend};
use crate::graph::ModuleGraph;
use crate::incremental::BuildState;
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

/// Build runner that executes a build plan.
pub struct BuildRunner {
    backend: ClangBackend,
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
    pub fn new(backend: ClangBackend, cache: Option<ArtifactCache>) -> Self {
        BuildRunner {
            backend,
            cache,
            remote_cache: None,
            no_cache: false,
            force_rebuild: false,
            max_jobs: 0,
            extra_pcm_paths: HashMap::new(),
            extra_obj_paths: Vec::new(),
        }
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

    /// Compute a hash representing the current compiler flags.
    fn flags_hash(&self) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(self.backend.cxx_standard.as_bytes());
        if let Some(ref stdlib) = self.backend.stdlib {
            hasher.update(stdlib.as_bytes());
        }
        if let Some(ref target) = self.backend.target {
            hasher.update(target.as_bytes());
        }
        for flag in &self.backend.extra_flags {
            hasher.update(flag.as_bytes());
        }
        if let Some(ref sysroot) = self.backend.sysroot {
            hasher.update(sysroot.to_string_lossy().as_bytes());
        }
        let profile_str = match self.backend.profile {
            Profile::Debug => "debug",
            Profile::Release => "release",
        };
        hasher.update(profile_str.as_bytes());
        format!("{:x}", hasher.finalize())
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
    ) -> Result<PathBuf, CmodError> {
        // Validate the graph
        graph.validate()?;

        // Generate the build plan
        let plan = BuildPlan::from_graph(
            graph,
            &build_dir.to_path_buf(),
            target,
            profile,
            build_type,
        )?;

        // Ensure output directories exist
        fs::create_dir_all(build_dir.join("pcm"))?;
        fs::create_dir_all(build_dir.join("obj"))?;

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
    ) -> Result<(PathBuf, BuildStats), CmodError> {
        graph.validate()?;
        let plan = BuildPlan::from_graph(
            graph,
            &build_dir.to_path_buf(),
            target,
            profile,
            build_type,
        )?;
        fs::create_dir_all(build_dir.join("pcm"))?;
        fs::create_dir_all(build_dir.join("obj"))?;
        self.execute_plan(&plan)
    }

    /// Compute a cache key for a build node.
    fn compute_cache_key(
        &self,
        node: &BuildNode,
        plan: &BuildPlan,
    ) -> Option<(String, CacheKey)> {
        let source = node.source.as_ref()?;
        let module_id = node.module_name.as_ref()?;

        let source_hash = hash_file(source).ok()?;

        // Gather dependency hashes from the dependency output files
        let mut dep_hashes = Vec::new();
        for dep_id in &node.dependencies {
            // Find the dependency node and hash its outputs
            if let Some(dep_node) = plan.nodes.iter().find(|n| &n.id == dep_id) {
                for output in &dep_node.outputs {
                    if output.exists() {
                        if let Ok(h) = hash_file(output) {
                            dep_hashes.push(h);
                        }
                    }
                }
            }
        }

        let cxx_standard = self.backend.cxx_standard.clone();
        let stdlib = self.backend.stdlib.clone().unwrap_or_default();

        let inputs = CacheKeyInputs {
            source_hash,
            dependency_hashes: dep_hashes,
            compiler: "clang".to_string(),
            compiler_version: String::new(),
            cxx_standard,
            stdlib,
            target: plan.target.clone(),
            flags: self.backend.extra_flags.clone(),
        };

        Some((module_id.clone(), CacheKey::compute(&inputs)))
    }

    /// Try to restore a node's outputs from cache. Returns true on hit.
    ///
    /// Checks local cache first. On local miss, tries the remote cache
    /// (if configured) and stores the downloaded artifact locally.
    fn try_cache_restore(
        &self,
        module_id: &str,
        key: &CacheKey,
        node: &BuildNode,
    ) -> bool {
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
            let mut all_downloaded = true;
            for output in &node.outputs {
                let artifact_name = output
                    .file_name()
                    .and_then(|f| f.to_str())
                    .unwrap_or("unknown");

                match remote.get(module_id, key, artifact_name, output) {
                    Ok(true) => {
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

        false
    }

    /// Store a node's outputs into cache after successful compilation.
    ///
    /// Stores locally and, if a remote cache is configured for writes,
    /// pushes the artifacts upstream.
    fn cache_store(
        &self,
        module_id: &str,
        key: &CacheKey,
        node: &BuildNode,
    ) {
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
                compiler: "clang".to_string(),
                compiler_version: String::new(),
                target: String::new(),
                created_at: String::new(),
                artifacts: artifact_entries,
            };

            let file_refs: Vec<(&str, &Path)> = artifact_files
                .iter()
                .map(|(name, path)| (name.as_str(), path.as_path()))
                .collect();

            let _ = cache.store(module_id, key, &metadata, &file_refs);
        }

        // Push to remote cache if configured
        if let Some(ref remote) = self.remote_cache {
            for (name, path) in &artifact_files {
                let _ = remote.put(module_id, key, name, path);
            }
        }
    }

    /// Execute a single compile/link node.
    ///
    /// The `build_state` and `flags_hash` enable incremental skip detection.
    /// If the node is unchanged since the last build, it is skipped without
    /// touching the cache at all.
    fn execute_node(
        &self,
        node: &BuildNode,
        plan: &BuildPlan,
        pcm_map: &HashMap<String, PathBuf>,
        build_state: Option<&BuildState>,
        flags_hash: &str,
    ) -> Result<NodeOutcome, CmodError> {
        let start = Instant::now();

        match node.kind {
            NodeKind::Interface => {
                let source = node.source.as_ref().unwrap();
                let pcm_output = &node.outputs[0];
                let obj_output = &node.outputs[1];

                // Check incremental state first (cheapest check)
                if !self.force_rebuild {
                    if let Some(state) = build_state {
                        if state.needs_rebuild(node, flags_hash).is_none() {
                            eprintln!("  Up-to-date: {}", source.display());
                            return Ok(NodeOutcome::Skipped(start.elapsed().as_millis() as u64));
                        }
                    }
                }

                // Try cache next
                if let Some((module_id, key)) = self.compute_cache_key(node, plan) {
                    if self.try_cache_restore(&module_id, &key, node) {
                        eprintln!("  Cached interface: {}", source.display());
                        return Ok(NodeOutcome::CacheHit(start.elapsed().as_millis() as u64));
                    }
                }

                // Pass all available PCMs — clang needs transitive module visibility
                // (e.g., when a module re-exports partitions via `export import :part;`)
                let all_pcms: Vec<(&str, &Path)> = pcm_map
                    .iter()
                    .map(|(name, path)| (name.as_str(), path.as_path()))
                    .collect();

                if let Some(parent) = pcm_output.parent() {
                    fs::create_dir_all(parent)?;
                }
                if let Some(parent) = obj_output.parent() {
                    fs::create_dir_all(parent)?;
                }

                self.backend
                    .compile_interface(source, pcm_output, obj_output, &all_pcms)?;

                if let Some((module_id, key)) = self.compute_cache_key(node, plan) {
                    self.cache_store(&module_id, &key, node);
                }

                eprintln!("  Compiled interface: {}", source.display());
                Ok(NodeOutcome::Compiled(start.elapsed().as_millis() as u64))
            }

            NodeKind::Implementation => {
                let source = node.source.as_ref().unwrap();
                let obj_output = &node.outputs[0];

                if !self.force_rebuild {
                    if let Some(state) = build_state {
                        if state.needs_rebuild(node, flags_hash).is_none() {
                            eprintln!("  Up-to-date: {}", source.display());
                            return Ok(NodeOutcome::Skipped(start.elapsed().as_millis() as u64));
                        }
                    }
                }

                if let Some((module_id, key)) = self.compute_cache_key(node, plan) {
                    if self.try_cache_restore(&module_id, &key, node) {
                        eprintln!("  Cached impl: {}", source.display());
                        return Ok(NodeOutcome::CacheHit(start.elapsed().as_millis() as u64));
                    }
                }

                // Pass all available PCMs for transitive visibility
                let all_pcms: Vec<(&str, &Path)> = pcm_map
                    .iter()
                    .map(|(name, path)| (name.as_str(), path.as_path()))
                    .collect();

                if let Some(parent) = obj_output.parent() {
                    fs::create_dir_all(parent)?;
                }

                self.backend
                    .compile_implementation(source, obj_output, &all_pcms)?;

                if let Some((module_id, key)) = self.compute_cache_key(node, plan) {
                    self.cache_store(&module_id, &key, node);
                }

                eprintln!("  Compiled impl: {}", source.display());
                Ok(NodeOutcome::Compiled(start.elapsed().as_millis() as u64))
            }

            NodeKind::Object => {
                let source = node.source.as_ref().unwrap();
                let obj_output = &node.outputs[0];

                if !self.force_rebuild {
                    if let Some(state) = build_state {
                        if state.needs_rebuild(node, flags_hash).is_none() {
                            eprintln!("  Up-to-date: {}", source.display());
                            return Ok(NodeOutcome::Skipped(start.elapsed().as_millis() as u64));
                        }
                    }
                }

                if let Some((module_id, key)) = self.compute_cache_key(node, plan) {
                    if self.try_cache_restore(&module_id, &key, node) {
                        eprintln!("  Cached: {}", source.display());
                        return Ok(NodeOutcome::CacheHit(start.elapsed().as_millis() as u64));
                    }
                }

                if let Some(parent) = obj_output.parent() {
                    fs::create_dir_all(parent)?;
                }

                // Pass all available PCMs for transitive module visibility
                let all_pcms: Vec<(&str, &Path)> = pcm_map
                    .iter()
                    .map(|(name, path)| (name.as_str(), path.as_path()))
                    .collect();

                self.backend
                    .compile_implementation(source, obj_output, &all_pcms)?;

                if let Some((module_id, key)) = self.compute_cache_key(node, plan) {
                    self.cache_store(&module_id, &key, node);
                }

                eprintln!("  Compiled: {}", source.display());
                Ok(NodeOutcome::Compiled(start.elapsed().as_millis() as u64))
            }

            NodeKind::Link => {
                let output = &node.outputs[0];
                let mut obj_files = plan.object_paths();
                obj_files.extend(self.extra_obj_paths.clone());
                let obj_refs: Vec<&Path> =
                    obj_files.iter().map(|p| p.as_path()).collect();

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

                eprintln!("  Linked: {}", output.display());
                Ok(NodeOutcome::Linked(start.elapsed().as_millis() as u64))
            }
        }
    }

    /// Execute the build plan with parallel compilation.
    ///
    /// Uses a work-stealing scheduler: nodes whose dependencies are all
    /// complete are enqueued for execution across worker threads.
    /// The link node always runs last on the main thread.
    fn execute_plan(&self, plan: &BuildPlan) -> Result<(PathBuf, BuildStats), CmodError> {
        let wall_start = Instant::now();
        let jobs = self.effective_jobs();

        // Load incremental build state
        let build_state = Arc::new(BuildState::load(&plan.build_dir));
        let flags_hash = Arc::new(self.flags_hash());

        // Separate compile nodes from the link node
        let (compile_nodes, link_nodes): (Vec<_>, Vec<_>) =
            plan.nodes.iter().enumerate().partition(|(_, n)| n.kind != NodeKind::Link);

        // Build a map of node_id → index for fast lookup
        let id_to_idx: HashMap<String, usize> = plan
            .nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.clone(), i))
            .collect();

        // Compute PCM paths for dependency resolution during compilation.
        // Include extra PCMs from workspace dependencies.
        let mut pcm_map_inner: HashMap<String, PathBuf> =
            plan.pcm_paths().into_iter().collect();
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
            let in_deg = in_degree.lock().unwrap();
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
                        if !errors.lock().unwrap().is_empty() {
                            break;
                        }

                        let idx = match work_rx.recv_timeout(std::time::Duration::from_millis(50)) {
                            Ok(idx) => idx,
                            Err(crossbeam_channel::RecvTimeoutError::Timeout) => continue,
                            Err(crossbeam_channel::RecvTimeoutError::Disconnected) => break,
                        };

                        let node = &plan.nodes[idx];
                        match self.execute_node(node, plan, &pcm_map, Some(&build_state), &flags_hash) {
                            Ok(outcome) => {
                                let ms = outcome.time_ms();
                                total_compile_ms.fetch_add(ms as usize, Ordering::Relaxed);
                                node_timings.lock().unwrap().insert(node.id.clone(), ms);
                                match outcome {
                                    NodeOutcome::CacheHit(_) => {
                                        cache_hits.fetch_add(1, Ordering::Relaxed);
                                        new_build_state.lock().unwrap().record_node(node, &flags_hash);
                                    }
                                    NodeOutcome::Compiled(_) => {
                                        cache_misses.fetch_add(1, Ordering::Relaxed);
                                        new_build_state.lock().unwrap().record_node(node, &flags_hash);
                                    }
                                    NodeOutcome::Skipped(_) => {
                                        incr_skipped.fetch_add(1, Ordering::Relaxed);
                                        if let Some(prev) = build_state.nodes.get(&node.id) {
                                            new_build_state.lock().unwrap().nodes.insert(
                                                node.id.clone(), prev.clone(),
                                            );
                                        }
                                    }
                                    NodeOutcome::Linked(_) => {}
                                }
                            }
                            Err(e) => {
                                errors.lock().unwrap().push(e);
                            }
                        }

                        // Signal completion and enqueue newly-ready nodes
                        let c = completed.fetch_add(1, Ordering::SeqCst) + 1;
                        {
                            let mut in_deg = in_degree.lock().unwrap();
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

        // Check for errors
        let errs = errors.lock().unwrap();
        if let Some(first) = errs.first() {
            return Err(CmodError::BuildFailed {
                reason: format!("{}", first),
            });
        }
        drop(errs);

        // Save the new build state with node timings
        {
            let mut final_state = new_build_state.lock().unwrap().clone();
            let timings = node_timings.lock().unwrap();
            final_state.node_timings = timings.clone();
            let _ = final_state.save(&plan.build_dir);
        }

        // Rebuild pcm_map for link phase (Arc was shared with threads)
        let link_pcm_map: HashMap<String, PathBuf> = plan.pcm_paths().into_iter().collect();

        // Execute link node(s) on main thread
        let mut final_output = PathBuf::new();
        for &(idx, _) in &link_nodes {
            let node = &plan.nodes[idx];
            self.execute_node(node, plan, &link_pcm_map, None, &flags_hash)?;
            if let Some(out) = node.outputs.first() {
                final_output = out.clone();
            }
        }

        let stats = BuildStats {
            cache_hits: cache_hits.load(Ordering::Relaxed),
            cache_misses: cache_misses.load(Ordering::Relaxed),
            skipped: link_nodes.len(),
            incremental_skipped: incr_skipped.load(Ordering::Relaxed),
            wall_time_ms: wall_start.elapsed().as_millis() as u64,
            total_compile_time_ms: total_compile_ms.load(Ordering::Relaxed) as u64,
            node_timings: Arc::try_unwrap(node_timings).unwrap_or_default().into_inner().unwrap_or_default(),
        };

        Ok((final_output, stats))
    }

    /// Sequential fallback for single-job mode or trivial plans.
    fn execute_plan_sequential(
        &self,
        plan: &BuildPlan,
    ) -> Result<(PathBuf, BuildStats), CmodError> {
        let wall_start = Instant::now();
        let build_state = BuildState::load(&plan.build_dir);
        let flags_hash = self.flags_hash();
        let mut pcm_map: HashMap<String, PathBuf> = plan.pcm_paths().into_iter().collect();
        pcm_map.extend(self.extra_pcm_paths.clone());
        let mut new_state = BuildState::default();
        let mut final_output = PathBuf::new();
        let mut stats = BuildStats::default();

        for node in &plan.nodes {
            let outcome = self.execute_node(node, plan, &pcm_map, Some(&build_state), &flags_hash)?;
            let ms = outcome.time_ms();
            stats.node_timings.insert(node.id.clone(), ms);
            match outcome {
                NodeOutcome::CacheHit(ms) => {
                    stats.cache_hits += 1;
                    stats.total_compile_time_ms += ms;
                    new_state.record_node(node, &flags_hash);
                }
                NodeOutcome::Compiled(ms) => {
                    stats.cache_misses += 1;
                    stats.total_compile_time_ms += ms;
                    new_state.record_node(node, &flags_hash);
                }
                NodeOutcome::Skipped(ms) => {
                    stats.incremental_skipped += 1;
                    stats.total_compile_time_ms += ms;
                    // Preserve existing state for skipped nodes
                    if let Some(prev) = build_state.nodes.get(&node.id) {
                        new_state.nodes.insert(node.id.clone(), prev.clone());
                    }
                }
                NodeOutcome::Linked(ms) => {
                    stats.skipped += 1;
                    stats.total_compile_time_ms += ms;
                    if let Some(out) = node.outputs.first() {
                        final_output = out.clone();
                    }
                }
            }
        }

        // Save updated build state with node timings
        new_state.node_timings = stats.node_timings.clone();
        let _ = new_state.save(&plan.build_dir);

        stats.wall_time_ms = wall_start.elapsed().as_millis() as u64;
        Ok((final_output, stats))
    }
}

/// Discover C++ module source files in a directory.
///
/// Looks for `.cppm`, `.ixx`, `.mpp` (module interface) and `.cpp` files.
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
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                match ext {
                    "cppm" | "ixx" | "mpp" | "cpp" | "cc" | "cxx" => {
                        sources.push(path.to_path_buf());
                    }
                    _ => {}
                }
            }
        }
    }

    sources.sort();
    Ok(sources)
}

/// Classify a source file as a module interface or implementation based on content.
///
/// Scans the first few lines for `export module` declaration.
pub fn classify_source(path: &Path) -> Result<cmod_core::types::ModuleUnitKind, CmodError> {
    let content = fs::read_to_string(path)?;

    for line in content.lines().take(50) {
        let trimmed = line.trim();
        if trimmed.starts_with("export module") {
            if trimmed.contains(':') {
                return Ok(cmod_core::types::ModuleUnitKind::PartitionUnit);
            }
            return Ok(cmod_core::types::ModuleUnitKind::InterfaceUnit);
        }
        if trimmed.starts_with("module") && !trimmed.starts_with("module;") {
            return Ok(cmod_core::types::ModuleUnitKind::ImplementationUnit);
        }
    }

    // No module declaration found — treat as legacy TU
    Ok(cmod_core::types::ModuleUnitKind::LegacyUnit)
}

/// Extract the module name from an `export module ...;` declaration.
pub fn extract_module_name(path: &Path) -> Result<Option<String>, CmodError> {
    let content = fs::read_to_string(path)?;

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("export module") || trimmed.starts_with("module") {
            // Parse: `export module foo.bar;` or `export module foo:partition;`
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

    #[test]
    fn test_effective_jobs_auto() {
        let backend = crate::compiler::ClangBackend::new("20", cmod_core::types::Profile::Debug);
        let runner = BuildRunner::new(backend, None);
        // auto-detect should be at least 1
        assert!(runner.effective_jobs() >= 1);
    }

    #[test]
    fn test_effective_jobs_explicit() {
        let backend = crate::compiler::ClangBackend::new("20", cmod_core::types::Profile::Debug);
        let runner = BuildRunner::new(backend, None).with_jobs(4);
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
        fs::write(&file, "// Copyright 2024\n// License: MIT\nexport module mymod;\n").unwrap();

        let kind = classify_source(&file).unwrap();
        assert_eq!(kind, cmod_core::types::ModuleUnitKind::InterfaceUnit);
    }
}
