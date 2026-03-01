use std::collections::HashMap;
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
    /// Total wall-clock time for the build.
    pub wall_time_ms: u64,
    /// Sum of individual compilation times (may exceed wall_time when parallel).
    pub total_compile_time_ms: u64,
}

/// Build runner that executes a build plan.
pub struct BuildRunner {
    backend: ClangBackend,
    cache: Option<ArtifactCache>,
    /// When true, skip cache lookups and always recompile.
    pub no_cache: bool,
    /// Maximum parallel jobs (0 = auto-detect CPU count).
    pub max_jobs: usize,
}

/// Outcome of executing a single build node.
enum NodeOutcome {
    CacheHit(u64),
    Compiled(u64),
    Linked(u64),
}

impl NodeOutcome {
    fn time_ms(&self) -> u64 {
        match self {
            NodeOutcome::CacheHit(ms) | NodeOutcome::Compiled(ms) | NodeOutcome::Linked(ms) => *ms,
        }
    }
}

impl BuildRunner {
    pub fn new(backend: ClangBackend, cache: Option<ArtifactCache>) -> Self {
        BuildRunner {
            backend,
            cache,
            no_cache: false,
            max_jobs: 0,
        }
    }

    /// Set the maximum parallel jobs.
    pub fn with_jobs(mut self, jobs: usize) -> Self {
        self.max_jobs = jobs;
        self
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
    fn try_cache_restore(
        &self,
        module_id: &str,
        key: &CacheKey,
        node: &BuildNode,
    ) -> bool {
        let cache = match &self.cache {
            Some(c) => c,
            None => return false,
        };

        if self.no_cache || !cache.has(module_id, key) {
            return false;
        }

        // Try to copy each output from cache
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
                        return false;
                    }
                }
                None => return false,
            }
        }

        true
    }

    /// Store a node's outputs into cache after successful compilation.
    fn cache_store(
        &self,
        module_id: &str,
        key: &CacheKey,
        node: &BuildNode,
    ) {
        let cache = match &self.cache {
            Some(c) => c,
            None => return,
        };

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

    /// Execute a single compile/link node. Returns (node_index, compile_time_ms, outcome).
    fn execute_node(
        &self,
        node: &BuildNode,
        plan: &BuildPlan,
        pcm_map: &HashMap<String, PathBuf>,
    ) -> Result<NodeOutcome, CmodError> {
        let start = Instant::now();

        match node.kind {
            NodeKind::Interface => {
                let source = node.source.as_ref().unwrap();
                let pcm_output = &node.outputs[0];
                let obj_output = &node.outputs[1];

                // Try cache first
                if let Some((module_id, key)) = self.compute_cache_key(node, plan) {
                    if self.try_cache_restore(&module_id, &key, node) {
                        eprintln!("  Cached interface: {}", source.display());
                        return Ok(NodeOutcome::CacheHit(start.elapsed().as_millis() as u64));
                    }
                }

                let dep_pcms: Vec<(&str, &Path)> = node
                    .dependencies
                    .iter()
                    .filter_map(|dep_id| {
                        let name = dep_id.strip_prefix("interface:")?;
                        pcm_map.get(name).map(|p| (name, p.as_path()))
                    })
                    .collect();

                if let Some(parent) = pcm_output.parent() {
                    fs::create_dir_all(parent)?;
                }
                if let Some(parent) = obj_output.parent() {
                    fs::create_dir_all(parent)?;
                }

                self.backend
                    .compile_interface(source, pcm_output, obj_output, &dep_pcms)?;

                if let Some((module_id, key)) = self.compute_cache_key(node, plan) {
                    self.cache_store(&module_id, &key, node);
                }

                eprintln!("  Compiled interface: {}", source.display());
                Ok(NodeOutcome::Compiled(start.elapsed().as_millis() as u64))
            }

            NodeKind::Implementation => {
                let source = node.source.as_ref().unwrap();
                let obj_output = &node.outputs[0];

                if let Some((module_id, key)) = self.compute_cache_key(node, plan) {
                    if self.try_cache_restore(&module_id, &key, node) {
                        eprintln!("  Cached impl: {}", source.display());
                        return Ok(NodeOutcome::CacheHit(start.elapsed().as_millis() as u64));
                    }
                }

                let dep_pcms: Vec<(&str, &Path)> = node
                    .dependencies
                    .iter()
                    .filter_map(|dep_id| {
                        let name = dep_id.strip_prefix("interface:")?;
                        pcm_map.get(name).map(|p| (name, p.as_path()))
                    })
                    .collect();

                if let Some(parent) = obj_output.parent() {
                    fs::create_dir_all(parent)?;
                }

                self.backend
                    .compile_implementation(source, obj_output, &dep_pcms)?;

                if let Some((module_id, key)) = self.compute_cache_key(node, plan) {
                    self.cache_store(&module_id, &key, node);
                }

                eprintln!("  Compiled impl: {}", source.display());
                Ok(NodeOutcome::Compiled(start.elapsed().as_millis() as u64))
            }

            NodeKind::Object => {
                let source = node.source.as_ref().unwrap();
                let obj_output = &node.outputs[0];

                if let Some((module_id, key)) = self.compute_cache_key(node, plan) {
                    if self.try_cache_restore(&module_id, &key, node) {
                        eprintln!("  Cached: {}", source.display());
                        return Ok(NodeOutcome::CacheHit(start.elapsed().as_millis() as u64));
                    }
                }

                if let Some(parent) = obj_output.parent() {
                    fs::create_dir_all(parent)?;
                }

                self.backend
                    .compile_implementation(source, obj_output, &[])?;

                if let Some((module_id, key)) = self.compute_cache_key(node, plan) {
                    self.cache_store(&module_id, &key, node);
                }

                eprintln!("  Compiled: {}", source.display());
                Ok(NodeOutcome::Compiled(start.elapsed().as_millis() as u64))
            }

            NodeKind::Link => {
                let output = &node.outputs[0];
                let obj_files = plan.object_paths();
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

        // Compute PCM paths for dependency resolution during compilation
        let pcm_map: Arc<HashMap<String, PathBuf>> = Arc::new(
            plan.pcm_paths().into_iter().collect(),
        );

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
        std::thread::scope(|scope| {
            for _ in 0..jobs {
                let work_rx = work_rx.clone();
                let work_tx = work_tx.clone();
                let completed = Arc::clone(&completed);
                let total_compile_ms = Arc::clone(&total_compile_ms);
                let cache_hits = Arc::clone(&cache_hits);
                let cache_misses = Arc::clone(&cache_misses);
                let in_degree = Arc::clone(&in_degree);
                let dependents = Arc::clone(&dependents);
                let errors = Arc::clone(&errors);
                let pcm_map = Arc::clone(&pcm_map);

                scope.spawn(move || {
                    while let Ok(idx) = work_rx.recv() {
                        // Check if we've had errors — stop processing new work
                        if !errors.lock().unwrap().is_empty() {
                            let c = completed.fetch_add(1, Ordering::SeqCst) + 1;
                            if c >= total {
                                // Close sender to unblock other workers
                            }
                            continue;
                        }

                        let node = &plan.nodes[idx];
                        match self.execute_node(node, plan, &pcm_map) {
                            Ok(outcome) => {
                                total_compile_ms.fetch_add(
                                    outcome.time_ms() as usize,
                                    Ordering::Relaxed,
                                );
                                match outcome {
                                    NodeOutcome::CacheHit(_) => {
                                        cache_hits.fetch_add(1, Ordering::Relaxed);
                                    }
                                    NodeOutcome::Compiled(_) => {
                                        cache_misses.fetch_add(1, Ordering::Relaxed);
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
                        let _ = c; // suppress unused warning
                    }
                });
            }
            // Drop sender on main thread so workers exit when all senders are dropped
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

        // Rebuild pcm_map for link phase (Arc was shared with threads)
        let link_pcm_map: HashMap<String, PathBuf> = plan.pcm_paths().into_iter().collect();

        // Execute link node(s) on main thread
        let mut final_output = PathBuf::new();
        for &(idx, _) in &link_nodes {
            let node = &plan.nodes[idx];
            self.execute_node(node, plan, &link_pcm_map)?;
            if let Some(out) = node.outputs.first() {
                final_output = out.clone();
            }
        }

        let stats = BuildStats {
            cache_hits: cache_hits.load(Ordering::Relaxed),
            cache_misses: cache_misses.load(Ordering::Relaxed),
            skipped: link_nodes.len(),
            wall_time_ms: wall_start.elapsed().as_millis() as u64,
            total_compile_time_ms: total_compile_ms.load(Ordering::Relaxed) as u64,
        };

        Ok((final_output, stats))
    }

    /// Sequential fallback for single-job mode or trivial plans.
    fn execute_plan_sequential(
        &self,
        plan: &BuildPlan,
    ) -> Result<(PathBuf, BuildStats), CmodError> {
        let wall_start = Instant::now();
        let pcm_map: HashMap<String, PathBuf> = plan.pcm_paths().into_iter().collect();
        let mut final_output = PathBuf::new();
        let mut stats = BuildStats::default();

        for node in &plan.nodes {
            let outcome = self.execute_node(node, plan, &pcm_map)?;
            match outcome {
                NodeOutcome::CacheHit(ms) => {
                    stats.cache_hits += 1;
                    stats.total_compile_time_ms += ms;
                }
                NodeOutcome::Compiled(ms) => {
                    stats.cache_misses += 1;
                    stats.total_compile_time_ms += ms;
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
