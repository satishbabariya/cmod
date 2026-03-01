use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use cmod_core::error::CmodError;
use cmod_core::types::{BuildType, NodeKind, Profile};

use crate::compiler::ClangBackend;
use crate::graph::ModuleGraph;

/// A single step in the build plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildNode {
    /// Content-addressed ID for this node.
    pub id: String,
    /// Kind of build step.
    pub kind: NodeKind,
    /// Module name (for interface/implementation nodes).
    pub module_name: Option<String>,
    /// Source file to compile.
    pub source: Option<PathBuf>,
    /// Node IDs that must complete before this node.
    pub dependencies: Vec<String>,
    /// Output artifact paths.
    pub outputs: Vec<PathBuf>,
}

/// A complete build plan — an ordered sequence of build nodes.
#[derive(Debug, Clone)]
pub struct BuildPlan {
    /// Target triple.
    pub target: String,
    /// Build profile.
    pub profile: Profile,
    /// Build output type.
    pub build_type: BuildType,
    /// Ordered build nodes (topologically sorted, dependencies first).
    pub nodes: Vec<BuildNode>,
    /// Build output directory.
    pub build_dir: PathBuf,
}

impl BuildPlan {
    /// Generate a build plan from a module graph and configuration.
    pub fn from_graph(
        graph: &ModuleGraph,
        build_dir: &PathBuf,
        target: &str,
        profile: Profile,
        build_type: BuildType,
    ) -> Result<Self, CmodError> {
        let order = graph.topological_order()?;
        let mut nodes = Vec::new();

        // Module name → PCM output path (for linking dependency PCMs)
        let mut pcm_paths: BTreeMap<String, PathBuf> = BTreeMap::new();
        let mut obj_paths: Vec<PathBuf> = Vec::new();

        let pcm_dir = build_dir.join("pcm");
        let obj_dir = build_dir.join("obj");

        for module_name in &order {
            let module_node = &graph.nodes[module_name];

            match module_node.kind {
                cmod_core::types::ModuleUnitKind::InterfaceUnit
                | cmod_core::types::ModuleUnitKind::PartitionUnit => {
                    let pcm_path = pcm_dir.join(format!("{}.pcm", sanitize_name(module_name)));
                    let obj_path = obj_dir.join(format!("{}.o", sanitize_name(module_name)));

                    // Dependencies are the PCM build nodes for imported modules
                    let deps: Vec<String> = module_node
                        .imports
                        .iter()
                        .filter_map(|imp| {
                            pcm_paths.get(imp).map(|_| format!("interface:{}", imp))
                        })
                        .collect();

                    let node = BuildNode {
                        id: format!("interface:{}", module_name),
                        kind: NodeKind::Interface,
                        module_name: Some(module_name.clone()),
                        source: Some(module_node.source.clone()),
                        dependencies: deps,
                        outputs: vec![pcm_path.clone(), obj_path.clone()],
                    };

                    pcm_paths.insert(module_name.clone(), pcm_path);
                    obj_paths.push(obj_path);
                    nodes.push(node);
                }

                cmod_core::types::ModuleUnitKind::ImplementationUnit => {
                    let obj_path = obj_dir.join(format!("{}.o", sanitize_name(module_name)));

                    let deps: Vec<String> = module_node
                        .imports
                        .iter()
                        .filter_map(|imp| {
                            pcm_paths.get(imp).map(|_| format!("interface:{}", imp))
                        })
                        .collect();

                    let node = BuildNode {
                        id: format!("impl:{}", module_name),
                        kind: NodeKind::Implementation,
                        module_name: Some(module_name.clone()),
                        source: Some(module_node.source.clone()),
                        dependencies: deps,
                        outputs: vec![obj_path.clone()],
                    };

                    obj_paths.push(obj_path);
                    nodes.push(node);
                }

                cmod_core::types::ModuleUnitKind::LegacyUnit => {
                    let obj_path = obj_dir.join(format!("{}.o", sanitize_name(module_name)));

                    let node = BuildNode {
                        id: format!("object:{}", module_name),
                        kind: NodeKind::Object,
                        module_name: Some(module_name.clone()),
                        source: Some(module_node.source.clone()),
                        dependencies: vec![],
                        outputs: vec![obj_path.clone()],
                    };

                    obj_paths.push(obj_path);
                    nodes.push(node);
                }
            }
        }

        // Add the link node
        let output_name = graph
            .nodes
            .keys()
            .last()
            .cloned()
            .unwrap_or_else(|| "output".to_string());

        let link_output = match build_type {
            BuildType::Binary => build_dir.join(sanitize_name(&output_name)),
            BuildType::StaticLib => {
                build_dir.join(format!("lib{}.a", sanitize_name(&output_name)))
            }
            BuildType::SharedLib => {
                build_dir.join(format!("lib{}.so", sanitize_name(&output_name)))
            }
        };

        let link_deps: Vec<String> = nodes.iter().map(|n| n.id.clone()).collect();

        nodes.push(BuildNode {
            id: "link".to_string(),
            kind: NodeKind::Link,
            module_name: None,
            source: None,
            dependencies: link_deps,
            outputs: vec![link_output],
        });

        Ok(BuildPlan {
            target: target.to_string(),
            profile,
            build_type,
            nodes,
            build_dir: build_dir.clone(),
        })
    }

    /// Get the PCM paths map for use during compilation.
    pub fn pcm_paths(&self) -> BTreeMap<String, PathBuf> {
        let mut map = BTreeMap::new();
        for node in &self.nodes {
            if node.kind == NodeKind::Interface {
                if let Some(ref name) = node.module_name {
                    // First output is the PCM
                    if let Some(pcm) = node.outputs.first() {
                        map.insert(name.clone(), pcm.clone());
                    }
                }
            }
        }
        map
    }

    /// Get all object file paths for linking.
    pub fn object_paths(&self) -> Vec<PathBuf> {
        let mut objs = Vec::new();
        for node in &self.nodes {
            match node.kind {
                NodeKind::Interface => {
                    // Second output is the object file
                    if let Some(obj) = node.outputs.get(1) {
                        objs.push(obj.clone());
                    }
                }
                NodeKind::Implementation | NodeKind::Object => {
                    if let Some(obj) = node.outputs.first() {
                        objs.push(obj.clone());
                    }
                }
                NodeKind::Link => {}
            }
        }
        objs
    }

    /// Generate a compile_commands.json-compatible list from this build plan.
    ///
    /// Each entry corresponds to a compilation step (interface, implementation,
    /// or object node) and includes the full clang++ invocation arguments.
    pub fn compile_commands(
        &self,
        backend: &ClangBackend,
        project_root: &Path,
    ) -> Vec<CompileCommand> {
        let pcm_paths = self.pcm_paths();
        let mut commands = Vec::new();

        for node in &self.nodes {
            let source = match node.source.as_ref() {
                Some(s) => s,
                None => continue, // skip link nodes
            };

            if node.kind == NodeKind::Link {
                continue;
            }

            let mut arguments = vec!["clang++".to_string()];
            arguments.extend(backend.common_flags());

            // Add dependency PCM references
            for dep_id in &node.dependencies {
                if let Some(name) = dep_id.strip_prefix("interface:") {
                    if let Some(pcm_path) = pcm_paths.get(name) {
                        arguments.push(format!(
                            "-fmodule-file={}={}",
                            name,
                            pcm_path.display()
                        ));
                    }
                }
            }

            match node.kind {
                NodeKind::Interface => {
                    // For compile_commands, represent the PCM→object step
                    arguments.push("--precompile".to_string());
                    arguments.push("-o".to_string());
                    if let Some(pcm) = node.outputs.first() {
                        arguments.push(pcm.display().to_string());
                    }
                    arguments.push(source.display().to_string());
                }
                NodeKind::Implementation | NodeKind::Object => {
                    arguments.push("-c".to_string());
                    arguments.push("-o".to_string());
                    if let Some(obj) = node.outputs.first() {
                        arguments.push(obj.display().to_string());
                    }
                    arguments.push(source.display().to_string());
                }
                NodeKind::Link => unreachable!(),
            }

            let output = node.outputs.first().cloned().unwrap_or_default();

            commands.push(CompileCommand {
                directory: project_root.display().to_string(),
                file: source.display().to_string(),
                arguments,
                output: output.display().to_string(),
            });
        }

        commands
    }
}

/// A single entry in a compile_commands.json compilation database.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompileCommand {
    /// The working directory for the compilation.
    pub directory: String,
    /// The source file being compiled.
    pub file: String,
    /// The full compiler invocation as an argument list.
    pub arguments: Vec<String>,
    /// The output file path.
    pub output: String,
}

/// Sanitize a module name for use as a filename.
fn sanitize_name(name: &str) -> String {
    name.replace('.', "_").replace(':', "_").replace('/', "_")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::{ModuleGraph, ModuleNode};
    use cmod_core::types::ModuleUnitKind;

    #[test]
    fn test_build_plan_single_module() {
        let mut graph = ModuleGraph::new();
        graph.add_node(ModuleNode {
            name: "mymod".to_string(),
            kind: ModuleUnitKind::InterfaceUnit,
            source: PathBuf::from("src/lib.cppm"),
            package: "test".to_string(),
            imports: vec![],
        });

        let plan = BuildPlan::from_graph(
            &graph,
            &PathBuf::from("/tmp/build"),
            "x86_64-unknown-linux-gnu",
            Profile::Debug,
            BuildType::Binary,
        )
        .unwrap();

        // Should have: 1 interface node + 1 link node
        assert_eq!(plan.nodes.len(), 2);
        assert_eq!(plan.nodes[0].kind, NodeKind::Interface);
        assert_eq!(plan.nodes[1].kind, NodeKind::Link);
    }

    #[test]
    fn test_build_plan_with_deps() {
        let mut graph = ModuleGraph::new();
        graph.add_node(ModuleNode {
            name: "base".to_string(),
            kind: ModuleUnitKind::InterfaceUnit,
            source: PathBuf::from("src/base.cppm"),
            package: "test".to_string(),
            imports: vec![],
        });
        graph.add_node(ModuleNode {
            name: "app".to_string(),
            kind: ModuleUnitKind::InterfaceUnit,
            source: PathBuf::from("src/app.cppm"),
            package: "test".to_string(),
            imports: vec!["base".to_string()],
        });

        let plan = BuildPlan::from_graph(
            &graph,
            &PathBuf::from("/tmp/build"),
            "x86_64-unknown-linux-gnu",
            Profile::Debug,
            BuildType::Binary,
        )
        .unwrap();

        // base (interface) + app (interface) + link
        assert_eq!(plan.nodes.len(), 3);
        // base should come before app
        assert_eq!(plan.nodes[0].module_name.as_deref(), Some("base"));
        assert_eq!(plan.nodes[1].module_name.as_deref(), Some("app"));
        // app should depend on base
        assert!(plan.nodes[1]
            .dependencies
            .contains(&"interface:base".to_string()));
    }

    #[test]
    fn test_pcm_paths() {
        let mut graph = ModuleGraph::new();
        graph.add_node(ModuleNode {
            name: "mod_a".to_string(),
            kind: ModuleUnitKind::InterfaceUnit,
            source: PathBuf::from("src/a.cppm"),
            package: "test".to_string(),
            imports: vec![],
        });

        let plan = BuildPlan::from_graph(
            &graph,
            &PathBuf::from("/tmp/build"),
            "x86_64-unknown-linux-gnu",
            Profile::Debug,
            BuildType::Binary,
        )
        .unwrap();

        let pcms = plan.pcm_paths();
        assert!(pcms.contains_key("mod_a"));
    }

    #[test]
    fn test_sanitize_name() {
        assert_eq!(sanitize_name("github.fmtlib.fmt"), "github_fmtlib_fmt");
        assert_eq!(sanitize_name("mod:partition"), "mod_partition");
        assert_eq!(sanitize_name("simple"), "simple");
        assert_eq!(sanitize_name("a/b/c"), "a_b_c");
    }

    #[test]
    fn test_build_plan_static_lib() {
        let mut graph = ModuleGraph::new();
        graph.add_node(ModuleNode {
            name: "mylib".to_string(),
            kind: ModuleUnitKind::InterfaceUnit,
            source: PathBuf::from("src/lib.cppm"),
            package: "test".to_string(),
            imports: vec![],
        });

        let plan = BuildPlan::from_graph(
            &graph,
            &PathBuf::from("/tmp/build"),
            "x86_64-unknown-linux-gnu",
            Profile::Release,
            BuildType::StaticLib,
        )
        .unwrap();

        assert_eq!(plan.build_type, BuildType::StaticLib);
        assert_eq!(plan.profile, Profile::Release);
        // Link node output should be a .a file
        let link_node = plan.nodes.last().unwrap();
        assert_eq!(link_node.kind, NodeKind::Link);
        let output_path = link_node.outputs[0].to_str().unwrap();
        assert!(output_path.ends_with(".a"), "Expected .a output: {}", output_path);
    }

    #[test]
    fn test_build_plan_shared_lib() {
        let mut graph = ModuleGraph::new();
        graph.add_node(ModuleNode {
            name: "mylib".to_string(),
            kind: ModuleUnitKind::InterfaceUnit,
            source: PathBuf::from("src/lib.cppm"),
            package: "test".to_string(),
            imports: vec![],
        });

        let plan = BuildPlan::from_graph(
            &graph,
            &PathBuf::from("/tmp/build"),
            "x86_64-unknown-linux-gnu",
            Profile::Debug,
            BuildType::SharedLib,
        )
        .unwrap();

        let link_node = plan.nodes.last().unwrap();
        let output_path = link_node.outputs[0].to_str().unwrap();
        assert!(output_path.ends_with(".so"), "Expected .so output: {}", output_path);
    }

    #[test]
    fn test_build_plan_implementation_unit() {
        let mut graph = ModuleGraph::new();
        graph.add_node(ModuleNode {
            name: "iface".to_string(),
            kind: ModuleUnitKind::InterfaceUnit,
            source: PathBuf::from("src/iface.cppm"),
            package: "test".to_string(),
            imports: vec![],
        });
        graph.add_node(ModuleNode {
            name: "impl_unit".to_string(),
            kind: ModuleUnitKind::ImplementationUnit,
            source: PathBuf::from("src/impl.cpp"),
            package: "test".to_string(),
            imports: vec!["iface".to_string()],
        });

        let plan = BuildPlan::from_graph(
            &graph,
            &PathBuf::from("/tmp/build"),
            "x86_64-unknown-linux-gnu",
            Profile::Debug,
            BuildType::Binary,
        )
        .unwrap();

        // interface + implementation + link = 3 nodes
        assert_eq!(plan.nodes.len(), 3);
        assert_eq!(plan.nodes[0].kind, NodeKind::Interface);
        assert_eq!(plan.nodes[1].kind, NodeKind::Implementation);
        assert_eq!(plan.nodes[2].kind, NodeKind::Link);

        // Implementation should depend on interface
        assert!(plan.nodes[1].dependencies.contains(&"interface:iface".to_string()));
    }

    #[test]
    fn test_build_plan_legacy_unit() {
        let mut graph = ModuleGraph::new();
        graph.add_node(ModuleNode {
            name: "main".to_string(),
            kind: ModuleUnitKind::LegacyUnit,
            source: PathBuf::from("src/main.cpp"),
            package: "test".to_string(),
            imports: vec![],
        });

        let plan = BuildPlan::from_graph(
            &graph,
            &PathBuf::from("/tmp/build"),
            "x86_64-unknown-linux-gnu",
            Profile::Debug,
            BuildType::Binary,
        )
        .unwrap();

        assert_eq!(plan.nodes.len(), 2); // object + link
        assert_eq!(plan.nodes[0].kind, NodeKind::Object);
        assert!(plan.nodes[0].dependencies.is_empty());
    }

    #[test]
    fn test_build_plan_object_paths() {
        let mut graph = ModuleGraph::new();
        graph.add_node(ModuleNode {
            name: "mod_a".to_string(),
            kind: ModuleUnitKind::InterfaceUnit,
            source: PathBuf::from("src/a.cppm"),
            package: "test".to_string(),
            imports: vec![],
        });
        graph.add_node(ModuleNode {
            name: "mod_b".to_string(),
            kind: ModuleUnitKind::ImplementationUnit,
            source: PathBuf::from("src/b.cpp"),
            package: "test".to_string(),
            imports: vec!["mod_a".to_string()],
        });

        let plan = BuildPlan::from_graph(
            &graph,
            &PathBuf::from("/tmp/build"),
            "x86_64-unknown-linux-gnu",
            Profile::Debug,
            BuildType::Binary,
        )
        .unwrap();

        let objs = plan.object_paths();
        assert_eq!(objs.len(), 2); // one from interface, one from impl
    }

    #[test]
    fn test_build_plan_diamond_deps() {
        let mut graph = ModuleGraph::new();
        graph.add_node(ModuleNode {
            name: "base".to_string(),
            kind: ModuleUnitKind::InterfaceUnit,
            source: PathBuf::from("src/base.cppm"),
            package: "test".to_string(),
            imports: vec![],
        });
        graph.add_node(ModuleNode {
            name: "left".to_string(),
            kind: ModuleUnitKind::InterfaceUnit,
            source: PathBuf::from("src/left.cppm"),
            package: "test".to_string(),
            imports: vec!["base".to_string()],
        });
        graph.add_node(ModuleNode {
            name: "right".to_string(),
            kind: ModuleUnitKind::InterfaceUnit,
            source: PathBuf::from("src/right.cppm"),
            package: "test".to_string(),
            imports: vec!["base".to_string()],
        });
        graph.add_node(ModuleNode {
            name: "top".to_string(),
            kind: ModuleUnitKind::InterfaceUnit,
            source: PathBuf::from("src/top.cppm"),
            package: "test".to_string(),
            imports: vec!["left".to_string(), "right".to_string()],
        });

        let plan = BuildPlan::from_graph(
            &graph,
            &PathBuf::from("/tmp/build"),
            "x86_64-unknown-linux-gnu",
            Profile::Debug,
            BuildType::Binary,
        )
        .unwrap();

        // 4 interface nodes + 1 link = 5
        assert_eq!(plan.nodes.len(), 5);

        // top depends on left and right
        let top_node = plan.nodes.iter().find(|n| n.module_name.as_deref() == Some("top")).unwrap();
        assert!(top_node.dependencies.contains(&"interface:left".to_string()));
        assert!(top_node.dependencies.contains(&"interface:right".to_string()));
    }

    #[test]
    fn test_compile_commands_generation() {
        let mut graph = ModuleGraph::new();
        graph.add_node(ModuleNode {
            name: "base".to_string(),
            kind: ModuleUnitKind::InterfaceUnit,
            source: PathBuf::from("src/base.cppm"),
            package: "test".to_string(),
            imports: vec![],
        });
        graph.add_node(ModuleNode {
            name: "app".to_string(),
            kind: ModuleUnitKind::ImplementationUnit,
            source: PathBuf::from("src/app.cpp"),
            package: "test".to_string(),
            imports: vec!["base".to_string()],
        });

        let plan = BuildPlan::from_graph(
            &graph,
            &PathBuf::from("/tmp/build"),
            "x86_64-unknown-linux-gnu",
            Profile::Debug,
            BuildType::Binary,
        )
        .unwrap();

        let backend = ClangBackend::new("20", Profile::Debug);
        let commands = plan.compile_commands(&backend, Path::new("/project"));

        // Should have 2 entries (interface + implementation), not the link node
        assert_eq!(commands.len(), 2);
        assert_eq!(commands[0].directory, "/project");
        assert_eq!(commands[0].file, "src/base.cppm");
        assert!(commands[0].arguments.contains(&"--precompile".to_string()));
        assert_eq!(commands[1].file, "src/app.cpp");
        assert!(commands[1].arguments.contains(&"-c".to_string()));
    }

    #[test]
    fn test_compile_commands_json_roundtrip() {
        let mut graph = ModuleGraph::new();
        graph.add_node(ModuleNode {
            name: "mymod".to_string(),
            kind: ModuleUnitKind::InterfaceUnit,
            source: PathBuf::from("src/lib.cppm"),
            package: "test".to_string(),
            imports: vec![],
        });

        let plan = BuildPlan::from_graph(
            &graph,
            &PathBuf::from("/tmp/build"),
            "x86_64-unknown-linux-gnu",
            Profile::Debug,
            BuildType::Binary,
        )
        .unwrap();

        let backend = ClangBackend::new("20", Profile::Debug);
        let commands = plan.compile_commands(&backend, Path::new("/project"));

        let json = serde_json::to_string_pretty(&commands).unwrap();
        let parsed: Vec<CompileCommand> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.len(), 1);
        assert_eq!(parsed[0].file, "src/lib.cppm");
    }

    #[test]
    fn test_compile_commands_skips_link_node() {
        let mut graph = ModuleGraph::new();
        graph.add_node(ModuleNode {
            name: "main".to_string(),
            kind: ModuleUnitKind::LegacyUnit,
            source: PathBuf::from("src/main.cpp"),
            package: "test".to_string(),
            imports: vec![],
        });

        let plan = BuildPlan::from_graph(
            &graph,
            &PathBuf::from("/tmp/build"),
            "x86_64-unknown-linux-gnu",
            Profile::Debug,
            BuildType::Binary,
        )
        .unwrap();

        // plan has 2 nodes: object + link
        assert_eq!(plan.nodes.len(), 2);

        let backend = ClangBackend::new("20", Profile::Debug);
        let commands = plan.compile_commands(&backend, Path::new("/project"));

        // Should only have 1 entry (the object node, not the link)
        assert_eq!(commands.len(), 1);
        assert_eq!(commands[0].file, "src/main.cpp");
    }

    #[test]
    fn test_compile_commands_includes_dep_pcm_refs() {
        let mut graph = ModuleGraph::new();
        graph.add_node(ModuleNode {
            name: "base".to_string(),
            kind: ModuleUnitKind::InterfaceUnit,
            source: PathBuf::from("src/base.cppm"),
            package: "test".to_string(),
            imports: vec![],
        });
        graph.add_node(ModuleNode {
            name: "derived".to_string(),
            kind: ModuleUnitKind::InterfaceUnit,
            source: PathBuf::from("src/derived.cppm"),
            package: "test".to_string(),
            imports: vec!["base".to_string()],
        });

        let plan = BuildPlan::from_graph(
            &graph,
            &PathBuf::from("/tmp/build"),
            "x86_64-unknown-linux-gnu",
            Profile::Debug,
            BuildType::Binary,
        )
        .unwrap();

        let backend = ClangBackend::new("20", Profile::Debug);
        let commands = plan.compile_commands(&backend, Path::new("/project"));

        assert_eq!(commands.len(), 2);
        // The derived module command should have a -fmodule-file reference to base
        let derived_cmd = &commands[1];
        assert!(
            derived_cmd.arguments.iter().any(|a| a.starts_with("-fmodule-file=base=")),
            "derived command should reference base PCM"
        );
    }
}
