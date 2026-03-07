use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use cmod_core::error::CmodError;
use cmod_core::types::ModuleUnitKind;

/// A node in the module dependency graph.
///
/// Each node represents a single translation unit (source file). Multiple nodes
/// can belong to the same logical module — e.g., an interface unit and one or
/// more implementation units for `local.math`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModuleNode {
    /// Unique node ID — typically the source file path (or a synthetic key for
    /// backward-compatible callers that use module-name keys).
    pub id: String,
    /// Module name (e.g., `github.fmtlib.fmt`).
    pub name: String,
    /// Kind of module unit.
    pub kind: ModuleUnitKind,
    /// Source file path.
    pub source: PathBuf,
    /// Package that this module belongs to.
    pub package: String,
    /// Modules that this node imports (logical module names, not node IDs).
    pub imports: Vec<String>,
    /// For partition units, the owning module name (e.g., `local.math` for `local.math:ops`).
    pub partition_of: Option<String>,
}

/// The module dependency graph (DAG).
///
/// Nodes are keyed by unique node ID (source path), not by module name.
/// This allows multiple translation units per logical module (interface +
/// implementation, partitions, etc.).
#[derive(Debug, Clone)]
pub struct ModuleGraph {
    /// All nodes, keyed by unique node ID.
    pub nodes: BTreeMap<String, ModuleNode>,
}

impl ModuleGraph {
    pub fn new() -> Self {
        ModuleGraph {
            nodes: BTreeMap::new(),
        }
    }

    /// Add a module node to the graph using the node's `id` as the key.
    pub fn add_node(&mut self, node: ModuleNode) {
        self.nodes.insert(node.id.clone(), node);
    }

    /// Add a module node keyed by module name (backward-compatible helper).
    ///
    /// The node's `id` field is set to `name` automatically. This works for
    /// graphs where each module has exactly one TU (the legacy layout).
    pub fn add_node_by_name(&mut self, mut node: ModuleNode) {
        node.id = node.name.clone();
        self.nodes.insert(node.name.clone(), node);
    }

    /// Get the interface node for a logical module name, if any.
    pub fn interface_for(&self, module_name: &str) -> Option<&ModuleNode> {
        self.nodes.values().find(|n| {
            n.name == module_name
                && (n.kind == ModuleUnitKind::InterfaceUnit
                    || n.kind == ModuleUnitKind::PartitionUnit)
        })
    }

    /// Get all implementation units for a logical module name.
    pub fn implementations_for(&self, module_name: &str) -> Vec<&ModuleNode> {
        self.nodes
            .values()
            .filter(|n| n.name == module_name && n.kind == ModuleUnitKind::ImplementationUnit)
            .collect()
    }

    /// Get all partition nodes that belong to the given owning module.
    pub fn partitions_of(&self, owning_module: &str) -> Vec<&ModuleNode> {
        self.nodes
            .values()
            .filter(|n| n.partition_of.as_deref() == Some(owning_module))
            .collect()
    }

    /// Collect all unique logical module names in the graph.
    pub fn module_names(&self) -> BTreeSet<String> {
        self.nodes.values().map(|n| n.name.clone()).collect()
    }

    /// Validate the graph: no cycles, all imports resolve, module unit constraints.
    pub fn validate(&self) -> Result<(), CmodError> {
        // Build the set of known logical module names for import validation
        let known_modules = self.module_names();

        for (node_id, node) in &self.nodes {
            for import in &node.imports {
                if !known_modules.contains(import) {
                    return Err(CmodError::ModuleScanFailed {
                        reason: format!(
                            "module '{}' (node '{}') imports '{}', which is not in the graph",
                            node.name, node_id, import
                        ),
                    });
                }
            }

            // Self-imports are invalid (a node importing its own module name is only
            // valid for implementation units importing the interface)
            if node.imports.contains(&node.name) && node.kind != ModuleUnitKind::ImplementationUnit
            {
                return Err(CmodError::ModuleScanFailed {
                    reason: format!("module '{}' imports itself", node.name),
                });
            }
        }

        // Check that each logical module has at most one interface unit
        let mut interfaces: BTreeMap<&str, &str> = BTreeMap::new();
        for node in self.nodes.values() {
            if node.kind == ModuleUnitKind::InterfaceUnit {
                if let Some(prev_id) = interfaces.get(node.name.as_str()) {
                    return Err(CmodError::ModuleScanFailed {
                        reason: format!(
                            "duplicate interface unit for module '{}': found in '{}' and '{}'",
                            node.name,
                            prev_id,
                            node.source.display()
                        ),
                    });
                }
                interfaces.insert(&node.name, &node.id);
            }
        }

        // Check for cycles using topological sort
        self.topological_order()?;

        Ok(())
    }

    /// Compute a topological ordering of the graph.
    ///
    /// Returns node IDs in dependency order (dependencies first).
    /// Errors if the graph contains a cycle.
    ///
    /// Implementation units are placed right after their interface unit to ensure
    /// correct PCM availability.
    pub fn topological_order(&self) -> Result<Vec<String>, CmodError> {
        // Build a module-level dependency graph first, then expand to nodes.
        // This ensures that all TUs of a depended-upon module come before
        // any TU that depends on it.

        let module_names = self.module_names();
        let mut mod_in_degree: BTreeMap<&str, usize> = BTreeMap::new();
        let mut mod_reverse_deps: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();

        for name in &module_names {
            mod_in_degree.entry(name.as_str()).or_insert(0);
            mod_reverse_deps.entry(name.as_str()).or_default();
        }

        // Compute module-level edges: module A depends on module B if any
        // TU of A imports B (excluding self-imports from impl→interface).
        for node in self.nodes.values() {
            for import in &node.imports {
                if import != &node.name
                    && module_names.contains(import)
                    && mod_reverse_deps
                        .entry(import.as_str())
                        .or_default()
                        .insert(node.name.as_str())
                {
                    *mod_in_degree.entry(node.name.as_str()).or_insert(0) += 1;
                }
            }
        }

        // Kahn's algorithm on module-level graph
        let mut queue: VecDeque<&str> = mod_in_degree
            .iter()
            .filter(|(_, &deg)| deg == 0)
            .map(|(&name, _)| name)
            .collect();

        let mut mod_order = Vec::new();

        while let Some(module) = queue.pop_front() {
            mod_order.push(module);

            if let Some(dependents) = mod_reverse_deps.get(module) {
                for &dep in dependents {
                    if let Some(deg) = mod_in_degree.get_mut(dep) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push_back(dep);
                        }
                    }
                }
            }
        }

        if mod_order.len() != module_names.len() {
            let remaining: Vec<String> = module_names
                .iter()
                .filter(|n| !mod_order.contains(&n.as_str()))
                .cloned()
                .collect();
            return Err(CmodError::CircularDependency {
                cycle: remaining.join(" -> "),
            });
        }

        // Expand module order to node IDs:
        // For each module, emit partition units first, then interface, then impls.
        let mut order = Vec::new();
        for module_name in &mod_order {
            let mut partitions = Vec::new();
            let mut interface = Vec::new();
            let mut impls = Vec::new();
            let mut legacy = Vec::new();

            for node in self.nodes.values() {
                if node.name.as_str() != *module_name {
                    continue;
                }
                match node.kind {
                    ModuleUnitKind::PartitionUnit => partitions.push(node.id.clone()),
                    ModuleUnitKind::InterfaceUnit => interface.push(node.id.clone()),
                    ModuleUnitKind::ImplementationUnit => impls.push(node.id.clone()),
                    ModuleUnitKind::LegacyUnit => legacy.push(node.id.clone()),
                }
            }

            // Sort within each category for determinism
            partitions.sort();
            interface.sort();
            impls.sort();
            legacy.sort();

            // Partitions → Interface → Implementations → Legacy
            order.extend(partitions);
            order.extend(interface);
            order.extend(impls);
            order.extend(legacy);
        }

        Ok(order)
    }

    /// Get node IDs that have no imports (roots of the build).
    pub fn roots(&self) -> Vec<&str> {
        self.nodes
            .iter()
            .filter(|(_, node)| node.imports.is_empty())
            .map(|(id, _)| id.as_str())
            .collect()
    }

    /// Get all node IDs that depend on the given module name.
    pub fn dependents(&self, module_name: &str) -> Vec<&str> {
        self.nodes
            .iter()
            .filter(|(_, node)| node.imports.iter().any(|i| i == module_name))
            .map(|(id, _)| id.as_str())
            .collect()
    }

    /// Compute the critical path through the graph using node timings.
    ///
    /// Returns the sequence of node IDs forming the longest path
    /// (by total compile time), which determines the minimum build time.
    pub fn critical_path(&self, timings: &BTreeMap<String, u64>) -> Vec<String> {
        let order = match self.topological_order() {
            Ok(o) => o,
            Err(_) => return vec![],
        };

        // dp[node_id] = (longest_path_time_to_this_node, predecessor)
        let mut dp: BTreeMap<&str, (u64, Option<&str>)> = BTreeMap::new();

        for node_id in &order {
            let node_time = timings.get(node_id.as_str()).copied().unwrap_or(0);
            let node = match self.nodes.get(node_id.as_str()) {
                Some(n) => n,
                None => continue,
            };

            // Find max incoming path — imports are module names, find all node IDs
            // for those modules and pick the best predecessor.
            let mut best_time = 0u64;
            let mut best_pred: Option<&str> = None;
            for import in &node.imports {
                // Find the last node emitted for the imported module
                for (id, n) in &self.nodes {
                    if n.name == *import {
                        if let Some(&(t, _)) = dp.get(id.as_str()) {
                            if t > best_time {
                                best_time = t;
                                best_pred = Some(id.as_str());
                            }
                        }
                    }
                }
            }

            let total = best_time + node_time;
            dp.insert(node_id.as_str(), (total, best_pred));
        }

        // Find the endpoint with the longest path
        let endpoint = dp.iter().max_by_key(|(_, (t, _))| *t).map(|(k, _)| *k);

        // Trace back
        let mut path = Vec::new();
        let mut current = endpoint;
        while let Some(node) = current {
            path.push(node.to_string());
            current = dp.get(node).and_then(|(_, pred)| *pred);
        }
        path.reverse();
        path
    }

    /// Return a topological ordering biased by critical-path priority.
    ///
    /// Nodes on the longest weighted path (by `timings`) are scheduled first
    /// among ready nodes, allowing the build engine to start critical work
    /// earlier and reduce overall wall-clock time.
    pub fn critical_path_order(
        &self,
        timings: &BTreeMap<String, u64>,
    ) -> Result<Vec<String>, CmodError> {
        let order = self.topological_order()?;

        // Compute longest-path-to-sink (bottom-up weight) for each node.
        // Process in reverse topological order.
        let mut weight: BTreeMap<&str, u64> = BTreeMap::new();

        for node_id in order.iter().rev() {
            let node_time = timings.get(node_id.as_str()).copied().unwrap_or(1);
            let node = match self.nodes.get(node_id.as_str()) {
                Some(n) => n,
                None => {
                    weight.insert(node_id.as_str(), node_time);
                    continue;
                }
            };

            // Find the maximum weight among dependents (successors)
            let max_successor_weight = self
                .dependents(&node.name)
                .iter()
                .filter_map(|dep_id| weight.get(dep_id))
                .copied()
                .max()
                .unwrap_or(0);

            weight.insert(node_id.as_str(), node_time + max_successor_weight);
        }

        // Sort the topological order by weight (descending) while preserving
        // dependency constraints. We use a priority-based Kahn's algorithm.
        let module_names = self.module_names();
        let mut mod_in_degree: BTreeMap<&str, usize> = BTreeMap::new();
        let mut mod_reverse_deps: BTreeMap<&str, BTreeSet<&str>> = BTreeMap::new();

        for name in &module_names {
            mod_in_degree.entry(name.as_str()).or_insert(0);
            mod_reverse_deps.entry(name.as_str()).or_default();
        }

        let mut edge_set: BTreeMap<(&str, &str), bool> = BTreeMap::new();
        for node in self.nodes.values() {
            for import in &node.imports {
                if import != &node.name && module_names.contains(import) {
                    let key = (import.as_str(), node.name.as_str());
                    if let std::collections::btree_map::Entry::Vacant(e) = edge_set.entry(key) {
                        e.insert(true);
                        mod_reverse_deps
                            .entry(import.as_str())
                            .or_default()
                            .insert(node.name.as_str());
                        *mod_in_degree.entry(node.name.as_str()).or_insert(0) += 1;
                    }
                }
            }
        }

        // Use a BinaryHeap to always pick the highest-weight ready module
        use std::collections::BinaryHeap;
        let mut heap: BinaryHeap<(u64, &str)> = BinaryHeap::new();
        for (&name, &deg) in &mod_in_degree {
            if deg == 0 {
                let w = self
                    .nodes
                    .values()
                    .filter(|n| n.name.as_str() == name)
                    .filter_map(|n| weight.get(n.id.as_str()))
                    .copied()
                    .max()
                    .unwrap_or(0);
                heap.push((w, name));
            }
        }

        let mut result = Vec::new();
        while let Some((_, module)) = heap.pop() {
            // Expand this module into its node IDs (partition→interface→impl→legacy)
            let mut partitions = Vec::new();
            let mut interface = Vec::new();
            let mut impls = Vec::new();
            let mut legacy = Vec::new();

            for node in self.nodes.values() {
                if node.name.as_str() != module {
                    continue;
                }
                match node.kind {
                    ModuleUnitKind::PartitionUnit => partitions.push(node.id.clone()),
                    ModuleUnitKind::InterfaceUnit => interface.push(node.id.clone()),
                    ModuleUnitKind::ImplementationUnit => impls.push(node.id.clone()),
                    ModuleUnitKind::LegacyUnit => legacy.push(node.id.clone()),
                }
            }
            partitions.sort();
            interface.sort();
            impls.sort();
            legacy.sort();
            result.extend(partitions);
            result.extend(interface);
            result.extend(impls);
            result.extend(legacy);

            if let Some(dependents) = mod_reverse_deps.get(module) {
                for &dep in dependents {
                    if let Some(deg) = mod_in_degree.get_mut(dep) {
                        *deg -= 1;
                        if *deg == 0 {
                            let w = self
                                .nodes
                                .values()
                                .filter(|n| n.name.as_str() == dep)
                                .filter_map(|n| weight.get(n.id.as_str()))
                                .copied()
                                .max()
                                .unwrap_or(0);
                            heap.push((w, dep));
                        }
                    }
                }
            }
        }

        Ok(result)
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
                    // Use the module name, not the node ID
                    if let Some(dep_node) = self.nodes.get(dep) {
                        queue.push_back(dep_node.name.clone());
                    }
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

    /// Helper: create a node keyed by module name (single-TU-per-module style).
    fn make_node(name: &str, imports: &[&str]) -> ModuleNode {
        ModuleNode {
            id: name.to_string(),
            name: name.to_string(),
            kind: ModuleUnitKind::InterfaceUnit,
            source: PathBuf::from(format!("src/{}.cppm", name)),
            package: "test".to_string(),
            imports: imports.iter().map(|s| s.to_string()).collect(),
            partition_of: None,
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
        graph.add_node(make_node("a", &["a"]));

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
            id: "src/a.cppm".to_string(),
            name: "mymod".to_string(),
            kind: ModuleUnitKind::InterfaceUnit,
            source: PathBuf::from("src/a.cppm"),
            package: "test".to_string(),
            imports: vec![],
            partition_of: None,
        });
        // Single interface validates OK.
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
    fn test_critical_path_order_diamond() {
        // Diamond: base → left, base → right, left → top, right → top
        // Timings: base=1, left=10, right=2, top=1
        // Critical path: base → left → top (weight 12)
        // So left should be scheduled before right among ready nodes.
        let mut graph = ModuleGraph::new();
        graph.add_node(make_node("base", &[]));
        graph.add_node(make_node("left", &["base"]));
        graph.add_node(make_node("right", &["base"]));
        graph.add_node(make_node("top", &["left", "right"]));

        let mut timings = BTreeMap::new();
        timings.insert("base".to_string(), 1);
        timings.insert("left".to_string(), 10);
        timings.insert("right".to_string(), 2);
        timings.insert("top".to_string(), 1);

        let order = graph.critical_path_order(&timings).unwrap();
        assert_eq!(order.len(), 4);

        // base must come first
        assert_eq!(order[0], "base");
        // left should come before right (higher critical-path weight)
        let left_pos = order.iter().position(|n| n == "left").unwrap();
        let right_pos = order.iter().position(|n| n == "right").unwrap();
        assert!(left_pos < right_pos);
        // top must come last
        assert_eq!(order[3], "top");
    }

    #[test]
    fn test_default() {
        let graph = ModuleGraph::default();
        assert!(graph.nodes.is_empty());
    }

    // ── Multi-TU tests (new for Phase 1.5) ─────────────────────

    #[test]
    fn test_multi_tu_interface_and_impl() {
        let mut graph = ModuleGraph::new();

        // Interface unit
        graph.add_node(ModuleNode {
            id: "src/lib.cppm".to_string(),
            name: "mymod".to_string(),
            kind: ModuleUnitKind::InterfaceUnit,
            source: PathBuf::from("src/lib.cppm"),
            package: "test".to_string(),
            imports: vec![],
            partition_of: None,
        });

        // Implementation unit (imports its own interface)
        graph.add_node(ModuleNode {
            id: "src/lib.cpp".to_string(),
            name: "mymod".to_string(),
            kind: ModuleUnitKind::ImplementationUnit,
            source: PathBuf::from("src/lib.cpp"),
            package: "test".to_string(),
            imports: vec!["mymod".to_string()],
            partition_of: None,
        });

        // Both nodes should exist (no collision)
        assert_eq!(graph.nodes.len(), 2);
        assert!(graph.validate().is_ok());

        let order = graph.topological_order().unwrap();
        assert_eq!(order.len(), 2);
        // Interface should come before implementation
        let iface_pos = order.iter().position(|n| n == "src/lib.cppm").unwrap();
        let impl_pos = order.iter().position(|n| n == "src/lib.cpp").unwrap();
        assert!(iface_pos < impl_pos);
    }

    #[test]
    fn test_duplicate_interface_detected() {
        let mut graph = ModuleGraph::new();
        graph.add_node(ModuleNode {
            id: "src/a.cppm".to_string(),
            name: "mymod".to_string(),
            kind: ModuleUnitKind::InterfaceUnit,
            source: PathBuf::from("src/a.cppm"),
            package: "test".to_string(),
            imports: vec![],
            partition_of: None,
        });
        graph.add_node(ModuleNode {
            id: "src/b.cppm".to_string(),
            name: "mymod".to_string(),
            kind: ModuleUnitKind::InterfaceUnit,
            source: PathBuf::from("src/b.cppm"),
            package: "test".to_string(),
            imports: vec![],
            partition_of: None,
        });

        let result = graph.validate();
        assert!(result.is_err());
        if let Err(CmodError::ModuleScanFailed { reason }) = result {
            assert!(reason.contains("duplicate interface unit"));
        }
    }

    #[test]
    fn test_partitions_with_owning_module() {
        let mut graph = ModuleGraph::new();
        graph.add_node(ModuleNode {
            id: "src/ops.cppm".to_string(),
            name: "math:ops".to_string(),
            kind: ModuleUnitKind::PartitionUnit,
            source: PathBuf::from("src/ops.cppm"),
            package: "test".to_string(),
            imports: vec![],
            partition_of: Some("math".to_string()),
        });
        graph.add_node(ModuleNode {
            id: "src/math.cppm".to_string(),
            name: "math".to_string(),
            kind: ModuleUnitKind::InterfaceUnit,
            source: PathBuf::from("src/math.cppm"),
            package: "test".to_string(),
            imports: vec!["math:ops".to_string()],
            partition_of: None,
        });

        assert!(graph.validate().is_ok());

        let partitions = graph.partitions_of("math");
        assert_eq!(partitions.len(), 1);
        assert_eq!(partitions[0].name, "math:ops");
    }

    #[test]
    fn test_interface_for_and_implementations_for() {
        let mut graph = ModuleGraph::new();
        graph.add_node(ModuleNode {
            id: "src/lib.cppm".to_string(),
            name: "mymod".to_string(),
            kind: ModuleUnitKind::InterfaceUnit,
            source: PathBuf::from("src/lib.cppm"),
            package: "test".to_string(),
            imports: vec![],
            partition_of: None,
        });
        graph.add_node(ModuleNode {
            id: "src/impl1.cpp".to_string(),
            name: "mymod".to_string(),
            kind: ModuleUnitKind::ImplementationUnit,
            source: PathBuf::from("src/impl1.cpp"),
            package: "test".to_string(),
            imports: vec!["mymod".to_string()],
            partition_of: None,
        });
        graph.add_node(ModuleNode {
            id: "src/impl2.cpp".to_string(),
            name: "mymod".to_string(),
            kind: ModuleUnitKind::ImplementationUnit,
            source: PathBuf::from("src/impl2.cpp"),
            package: "test".to_string(),
            imports: vec!["mymod".to_string()],
            partition_of: None,
        });

        assert!(graph.interface_for("mymod").is_some());
        assert_eq!(graph.implementations_for("mymod").len(), 2);
    }

    #[test]
    fn test_module_names() {
        let mut graph = ModuleGraph::new();
        graph.add_node(ModuleNode {
            id: "src/lib.cppm".to_string(),
            name: "mymod".to_string(),
            kind: ModuleUnitKind::InterfaceUnit,
            source: PathBuf::from("src/lib.cppm"),
            package: "test".to_string(),
            imports: vec![],
            partition_of: None,
        });
        graph.add_node(ModuleNode {
            id: "src/lib.cpp".to_string(),
            name: "mymod".to_string(),
            kind: ModuleUnitKind::ImplementationUnit,
            source: PathBuf::from("src/lib.cpp"),
            package: "test".to_string(),
            imports: vec!["mymod".to_string()],
            partition_of: None,
        });

        let names = graph.module_names();
        assert_eq!(names.len(), 1);
        assert!(names.contains("mymod"));
    }
}
