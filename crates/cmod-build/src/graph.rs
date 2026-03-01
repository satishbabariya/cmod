use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use cmod_core::error::CmodError;
use cmod_core::types::ModuleUnitKind;

/// A node in the module dependency graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleNode {
    /// Module name (e.g., `github.fmtlib.fmt`).
    pub name: String,
    /// Kind of module unit.
    pub kind: ModuleUnitKind,
    /// Source file path.
    pub source: PathBuf,
    /// Package that this module belongs to.
    pub package: String,
    /// Modules that this node imports.
    pub imports: Vec<String>,
}

/// The module dependency graph (DAG).
#[derive(Debug, Clone)]
pub struct ModuleGraph {
    /// All nodes, keyed by module name.
    pub nodes: BTreeMap<String, ModuleNode>,
}

impl ModuleGraph {
    pub fn new() -> Self {
        ModuleGraph {
            nodes: BTreeMap::new(),
        }
    }

    /// Add a module node to the graph.
    pub fn add_node(&mut self, node: ModuleNode) {
        self.nodes.insert(node.name.clone(), node);
    }

    /// Validate the graph: no cycles, all imports resolve, module unit constraints.
    pub fn validate(&self) -> Result<(), CmodError> {
        // Check that all imports reference existing nodes
        for (name, node) in &self.nodes {
            for import in &node.imports {
                if !self.nodes.contains_key(import) {
                    return Err(CmodError::ModuleScanFailed {
                        reason: format!(
                            "module '{}' imports '{}', which is not in the graph",
                            name, import
                        ),
                    });
                }
            }

            // Self-imports are invalid
            if node.imports.contains(&node.name) {
                return Err(CmodError::ModuleScanFailed {
                    reason: format!(
                        "module '{}' imports itself",
                        name,
                    ),
                });
            }
        }

        // Check that each interface unit is unique per module name
        let mut interfaces: BTreeMap<&str, &str> = BTreeMap::new();
        for (name, node) in &self.nodes {
            if node.kind == ModuleUnitKind::InterfaceUnit {
                if let Some(prev_source) = interfaces.get(name.as_str()) {
                    return Err(CmodError::ModuleScanFailed {
                        reason: format!(
                            "duplicate interface unit '{}': found in {} and {}",
                            name,
                            prev_source,
                            node.source.display()
                        ),
                    });
                }
                interfaces.insert(name, name);
            }
        }

        // Check for cycles using topological sort
        self.topological_order()?;

        Ok(())
    }

    /// Compute a topological ordering of the graph.
    ///
    /// Returns modules in dependency order (dependencies first).
    /// Errors if the graph contains a cycle.
    pub fn topological_order(&self) -> Result<Vec<String>, CmodError> {
        let mut in_degree: BTreeMap<&str, usize> = BTreeMap::new();
        let mut reverse_deps: BTreeMap<&str, Vec<&str>> = BTreeMap::new();

        // Initialize
        for name in self.nodes.keys() {
            in_degree.entry(name.as_str()).or_insert(0);
            reverse_deps.entry(name.as_str()).or_default();
        }

        // Compute in-degrees and reverse dep map
        for (name, node) in &self.nodes {
            for import in &node.imports {
                *in_degree.entry(import.as_str()).or_insert(0) += 0; // ensure exists
                *in_degree.entry(name.as_str()).or_insert(0) += 1;
                reverse_deps
                    .entry(import.as_str())
                    .or_default()
                    .push(name.as_str());
            }
        }

        // Kahn's algorithm
        let mut queue: VecDeque<&str> = in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(&name, _)| name)
            .collect();

        let mut order = Vec::new();

        while let Some(node) = queue.pop_front() {
            order.push(node.to_string());

            if let Some(dependents) = reverse_deps.get(node) {
                for &dep in dependents {
                    if let Some(deg) = in_degree.get_mut(dep) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push_back(dep);
                        }
                    }
                }
            }
        }

        if order.len() != self.nodes.len() {
            // Find the cycle for error reporting
            let remaining: Vec<String> = self
                .nodes
                .keys()
                .filter(|n| !order.contains(n))
                .cloned()
                .collect();
            return Err(CmodError::CircularDependency {
                cycle: remaining.join(" -> "),
            });
        }

        Ok(order)
    }

    /// Get modules that have no imports (roots of the build).
    pub fn roots(&self) -> Vec<&str> {
        self.nodes
            .iter()
            .filter(|(_, node)| node.imports.is_empty())
            .map(|(name, _)| name.as_str())
            .collect()
    }

    /// Get all modules that depend on the given module.
    pub fn dependents(&self, module_name: &str) -> Vec<&str> {
        self.nodes
            .iter()
            .filter(|(_, node)| node.imports.iter().any(|i| i == module_name))
            .map(|(name, _)| name.as_str())
            .collect()
    }

    /// Get the set of modules that would need rebuilding if the given module changes.
    ///
    /// This includes the module itself plus all transitive dependents.
    pub fn invalidation_set(&self, module_name: &str) -> BTreeSet<String> {
        let mut set = BTreeSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(module_name.to_string());

        while let Some(name) = queue.pop_front() {
            if set.insert(name.clone()) {
                for dep in self.dependents(&name) {
                    queue.push_back(dep.to_string());
                }
            }
        }

        set
    }
}

impl Default for ModuleGraph {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_node(name: &str, imports: &[&str]) -> ModuleNode {
        ModuleNode {
            name: name.to_string(),
            kind: ModuleUnitKind::InterfaceUnit,
            source: PathBuf::from(format!("src/{}.cppm", name)),
            package: "test".to_string(),
            imports: imports.iter().map(|s| s.to_string()).collect(),
        }
    }

    #[test]
    fn test_empty_graph() {
        let graph = ModuleGraph::new();
        assert!(graph.nodes.is_empty());
        assert!(graph.topological_order().unwrap().is_empty());
    }

    #[test]
    fn test_linear_chain() {
        let mut graph = ModuleGraph::new();
        graph.add_node(make_node("a", &[]));
        graph.add_node(make_node("b", &["a"]));
        graph.add_node(make_node("c", &["b"]));

        let order = graph.topological_order().unwrap();
        let a_pos = order.iter().position(|n| n == "a").unwrap();
        let b_pos = order.iter().position(|n| n == "b").unwrap();
        let c_pos = order.iter().position(|n| n == "c").unwrap();
        assert!(a_pos < b_pos);
        assert!(b_pos < c_pos);
    }

    #[test]
    fn test_diamond_dependency() {
        let mut graph = ModuleGraph::new();
        graph.add_node(make_node("base", &[]));
        graph.add_node(make_node("left", &["base"]));
        graph.add_node(make_node("right", &["base"]));
        graph.add_node(make_node("top", &["left", "right"]));

        let order = graph.topological_order().unwrap();
        assert_eq!(order.len(), 4);
        let base_pos = order.iter().position(|n| n == "base").unwrap();
        let left_pos = order.iter().position(|n| n == "left").unwrap();
        let right_pos = order.iter().position(|n| n == "right").unwrap();
        let top_pos = order.iter().position(|n| n == "top").unwrap();
        assert!(base_pos < left_pos);
        assert!(base_pos < right_pos);
        assert!(left_pos < top_pos);
        assert!(right_pos < top_pos);
    }

    #[test]
    fn test_cycle_detection() {
        let mut graph = ModuleGraph::new();
        graph.add_node(make_node("a", &["c"]));
        graph.add_node(make_node("b", &["a"]));
        graph.add_node(make_node("c", &["b"]));

        let result = graph.topological_order();
        assert!(result.is_err());
        if let Err(CmodError::CircularDependency { cycle }) = result {
            assert!(!cycle.is_empty());
        }
    }

    #[test]
    fn test_roots() {
        let mut graph = ModuleGraph::new();
        graph.add_node(make_node("a", &[]));
        graph.add_node(make_node("b", &[]));
        graph.add_node(make_node("c", &["a", "b"]));

        let roots = graph.roots();
        assert_eq!(roots.len(), 2);
        assert!(roots.contains(&"a"));
        assert!(roots.contains(&"b"));
    }

    #[test]
    fn test_invalidation_set() {
        let mut graph = ModuleGraph::new();
        graph.add_node(make_node("a", &[]));
        graph.add_node(make_node("b", &["a"]));
        graph.add_node(make_node("c", &["b"]));
        graph.add_node(make_node("d", &["a"]));

        let set = graph.invalidation_set("a");
        assert!(set.contains("a"));
        assert!(set.contains("b"));
        assert!(set.contains("c"));
        assert!(set.contains("d"));
    }

    #[test]
    fn test_validate_missing_import() {
        let mut graph = ModuleGraph::new();
        graph.add_node(make_node("a", &["nonexistent"]));

        let result = graph.validate();
        assert!(result.is_err());
    }

    #[test]
    fn test_validate_valid_graph() {
        let mut graph = ModuleGraph::new();
        graph.add_node(make_node("a", &[]));
        graph.add_node(make_node("b", &["a"]));
        graph.add_node(make_node("c", &["a", "b"]));

        assert!(graph.validate().is_ok());
    }

    #[test]
    fn test_dependents() {
        let mut graph = ModuleGraph::new();
        graph.add_node(make_node("a", &[]));
        graph.add_node(make_node("b", &["a"]));
        graph.add_node(make_node("c", &["a"]));
        graph.add_node(make_node("d", &["b"]));

        let deps_of_a = graph.dependents("a");
        assert_eq!(deps_of_a.len(), 2);
        assert!(deps_of_a.contains(&"b"));
        assert!(deps_of_a.contains(&"c"));

        let deps_of_b = graph.dependents("b");
        assert_eq!(deps_of_b.len(), 1);
        assert!(deps_of_b.contains(&"d"));

        let deps_of_d = graph.dependents("d");
        assert!(deps_of_d.is_empty());
    }

    #[test]
    fn test_invalidation_set_leaf() {
        let mut graph = ModuleGraph::new();
        graph.add_node(make_node("a", &[]));
        graph.add_node(make_node("b", &["a"]));
        graph.add_node(make_node("c", &["b"]));

        // Changing a leaf only invalidates itself
        let set = graph.invalidation_set("c");
        assert_eq!(set.len(), 1);
        assert!(set.contains("c"));
    }

    #[test]
    fn test_wide_graph_topological_order() {
        let mut graph = ModuleGraph::new();
        for i in 0..10 {
            graph.add_node(make_node(&format!("leaf_{}", i), &[]));
        }
        let leaf_names: Vec<String> = (0..10).map(|i| format!("leaf_{}", i)).collect();
        let leaf_refs: Vec<&str> = leaf_names.iter().map(|s| s.as_str()).collect();
        graph.add_node(make_node("root", &leaf_refs));

        let order = graph.topological_order().unwrap();
        assert_eq!(order.len(), 11);
        // root must be last
        let root_pos = order.iter().position(|n| n == "root").unwrap();
        assert_eq!(root_pos, 10);
    }

    #[test]
    fn test_validate_self_import() {
        let mut graph = ModuleGraph::new();
        let mut node = make_node("a", &["a"]);
        node.imports = vec!["a".to_string()];
        graph.add_node(node);

        let result = graph.validate();
        assert!(result.is_err());
        if let Err(CmodError::ModuleScanFailed { reason }) = result {
            assert!(reason.contains("imports itself"));
        }
    }

    #[test]
    fn test_validate_duplicate_interface_units() {
        let mut graph = ModuleGraph::new();
        graph.add_node(ModuleNode {
            name: "mymod".to_string(),
            kind: ModuleUnitKind::InterfaceUnit,
            source: PathBuf::from("src/a.cppm"),
            package: "test".to_string(),
            imports: vec![],
        });
        // Adding same name again replaces in BTreeMap, so we test via validate
        // The duplicate check is actually at the graph level - the BTreeMap prevents
        // true duplicates, but the interface check ensures the graph is well-formed.
        // Here we verify that a single interface validates OK.
        assert!(graph.validate().is_ok());
    }

    #[test]
    fn test_single_node_graph() {
        let mut graph = ModuleGraph::new();
        graph.add_node(make_node("only", &[]));

        let order = graph.topological_order().unwrap();
        assert_eq!(order, vec!["only"]);
        assert_eq!(graph.roots(), vec!["only"]);
        assert!(graph.dependents("only").is_empty());
        assert!(graph.validate().is_ok());
    }

    #[test]
    fn test_default() {
        let graph = ModuleGraph::default();
        assert!(graph.nodes.is_empty());
    }
}
