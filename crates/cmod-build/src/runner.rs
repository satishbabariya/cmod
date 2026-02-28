use std::fs;
use std::path::{Path, PathBuf};

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

    /// Execute each node in the build plan sequentially.
    ///
    /// Nodes are already in topological order (dependencies first).
    fn execute_plan(&self, plan: &BuildPlan) -> Result<(PathBuf, BuildStats), CmodError> {
        let pcm_map = plan.pcm_paths();
        let mut final_output = PathBuf::new();
        let mut stats = BuildStats::default();

        for node in &plan.nodes {
            match node.kind {
                NodeKind::Interface => {
                    let source = node.source.as_ref().unwrap();
                    let pcm_output = &node.outputs[0];
                    let obj_output = &node.outputs[1];

                    // Try cache first
                    if let Some((module_id, key)) = self.compute_cache_key(node, plan) {
                        if self.try_cache_restore(&module_id, &key, node) {
                            eprintln!("  Cached interface: {}", source.display());
                            stats.cache_hits += 1;
                            continue;
                        }
                    }

                    // Build dep PCM references
                    let dep_pcms: Vec<(&str, &Path)> = node
                        .dependencies
                        .iter()
                        .filter_map(|dep_id| {
                            let name = dep_id.strip_prefix("interface:")?;
                            pcm_map.get(name).map(|p| (name, p.as_path()))
                        })
                        .collect();

                    // Ensure parent dirs exist
                    if let Some(parent) = pcm_output.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    if let Some(parent) = obj_output.parent() {
                        fs::create_dir_all(parent)?;
                    }

                    self.backend
                        .compile_interface(source, pcm_output, obj_output, &dep_pcms)?;

                    // Store in cache
                    if let Some((module_id, key)) = self.compute_cache_key(node, plan) {
                        self.cache_store(&module_id, &key, node);
                    }

                    eprintln!("  Compiled interface: {}", source.display());
                    stats.cache_misses += 1;
                }

                NodeKind::Implementation => {
                    let source = node.source.as_ref().unwrap();
                    let obj_output = &node.outputs[0];

                    // Try cache first
                    if let Some((module_id, key)) = self.compute_cache_key(node, plan) {
                        if self.try_cache_restore(&module_id, &key, node) {
                            eprintln!("  Cached impl: {}", source.display());
                            stats.cache_hits += 1;
                            continue;
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

                    // Store in cache
                    if let Some((module_id, key)) = self.compute_cache_key(node, plan) {
                        self.cache_store(&module_id, &key, node);
                    }

                    eprintln!("  Compiled impl: {}", source.display());
                    stats.cache_misses += 1;
                }

                NodeKind::Object => {
                    let source = node.source.as_ref().unwrap();
                    let obj_output = &node.outputs[0];

                    // Try cache first
                    if let Some((module_id, key)) = self.compute_cache_key(node, plan) {
                        if self.try_cache_restore(&module_id, &key, node) {
                            eprintln!("  Cached: {}", source.display());
                            stats.cache_hits += 1;
                            continue;
                        }
                    }

                    if let Some(parent) = obj_output.parent() {
                        fs::create_dir_all(parent)?;
                    }

                    self.backend
                        .compile_implementation(source, obj_output, &[])?;

                    // Store in cache
                    if let Some((module_id, key)) = self.compute_cache_key(node, plan) {
                        self.cache_store(&module_id, &key, node);
                    }

                    eprintln!("  Compiled: {}", source.display());
                    stats.cache_misses += 1;
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
                    final_output = output.clone();
                    stats.skipped += 1;
                }
            }
        }

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
    fn test_classify_module_preamble_with_comments() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("test.cppm");
        // Module declaration should be found even with leading comments
        fs::write(&file, "// Copyright 2024\n// License: MIT\nexport module mymod;\n").unwrap();

        let kind = classify_source(&file).unwrap();
        assert_eq!(kind, cmod_core::types::ModuleUnitKind::InterfaceUnit);
    }
}
