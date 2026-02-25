use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use cmod_core::error::CmodError;
use cmod_core::types::{BuildType, NodeKind, Profile};

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
    }
}
