use std::fs;
use std::path::{Path, PathBuf};

use cmod_cache::cache::ArtifactCache;
use cmod_core::error::CmodError;
use cmod_core::types::{Artifact, BuildType, NodeKind, Profile};

use crate::compiler::{ClangBackend, CompilerBackend};
use crate::graph::ModuleGraph;
use crate::plan::BuildPlan;

/// Build runner that executes a build plan.
pub struct BuildRunner {
    backend: ClangBackend,
    #[allow(dead_code)]
    cache: Option<ArtifactCache>,
}

impl BuildRunner {
    pub fn new(backend: ClangBackend, cache: Option<ArtifactCache>) -> Self {
        BuildRunner { backend, cache }
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
        self.execute_plan(&plan)
    }

    /// Execute each node in the build plan sequentially.
    ///
    /// Nodes are already in topological order (dependencies first).
    fn execute_plan(&self, plan: &BuildPlan) -> Result<PathBuf, CmodError> {
        let pcm_map = plan.pcm_paths();
        let mut final_output = PathBuf::new();

        for node in &plan.nodes {
            match node.kind {
                NodeKind::Interface => {
                    let source = node.source.as_ref().unwrap();
                    let pcm_output = &node.outputs[0];
                    let obj_output = &node.outputs[1];

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

                    eprintln!("  Compiled interface: {}", source.display());
                }

                NodeKind::Implementation => {
                    let source = node.source.as_ref().unwrap();
                    let obj_output = &node.outputs[0];

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

                    eprintln!("  Compiled impl: {}", source.display());
                }

                NodeKind::Object => {
                    let source = node.source.as_ref().unwrap();
                    let obj_output = &node.outputs[0];

                    if let Some(parent) = obj_output.parent() {
                        fs::create_dir_all(parent)?;
                    }

                    self.backend
                        .compile_implementation(source, obj_output, &[])?;

                    eprintln!("  Compiled: {}", source.display());
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
                }
            }
        }

        Ok(final_output)
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
