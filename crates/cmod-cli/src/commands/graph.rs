use std::collections::{BTreeMap, BTreeSet};

use cmod_build::graph::ModuleGraph;
use cmod_build::incremental::BuildState;
use cmod_build::runner;
use cmod_core::config::Config;
use cmod_core::error::CmodError;

/// Output format for the graph command.
pub enum GraphFormat {
    Ascii,
    Dot,
    Json,
}

/// Status of a module in the build graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
enum NodeStatus {
    UpToDate,
    NeedsRebuild,
    NeverBuilt,
}

impl NodeStatus {
    fn ascii_marker(self) -> &'static str {
        match self {
            NodeStatus::UpToDate => "[ok]",
            NodeStatus::NeedsRebuild => "[!!]",
            NodeStatus::NeverBuilt => "[??]",
        }
    }

    fn dot_color(self) -> &'static str {
        match self {
            NodeStatus::UpToDate => "palegreen",
            NodeStatus::NeedsRebuild => "lightyellow",
            NodeStatus::NeverBuilt => "lightgray",
        }
    }

    fn label(self) -> &'static str {
        match self {
            NodeStatus::UpToDate => "up-to-date",
            NodeStatus::NeedsRebuild => "needs-rebuild",
            NodeStatus::NeverBuilt => "never-built",
        }
    }
}

/// Compute build status for each module from the build state.
fn compute_node_statuses(graph: &ModuleGraph, build_state: &BuildState) -> BTreeMap<String, NodeStatus> {
    let mut statuses = BTreeMap::new();

    for name in graph.nodes.keys() {
        let interface_id = format!("interface:{}", name);
        let impl_id = format!("impl:{}", name);
        let obj_id = format!("object:{}", name);

        let has_state = build_state.nodes.contains_key(&interface_id)
            || build_state.nodes.contains_key(&impl_id)
            || build_state.nodes.contains_key(&obj_id);

        if !has_state {
            statuses.insert(name.clone(), NodeStatus::NeverBuilt);
            continue;
        }

        let node_state = build_state.nodes.get(&interface_id)
            .or_else(|| build_state.nodes.get(&impl_id))
            .or_else(|| build_state.nodes.get(&obj_id));

        if let Some(ns) = node_state {
            if !ns.source_hash.is_empty() && !ns.output_hashes.is_empty() {
                statuses.insert(name.clone(), NodeStatus::UpToDate);
            } else {
                statuses.insert(name.clone(), NodeStatus::NeedsRebuild);
            }
        } else {
            statuses.insert(name.clone(), NodeStatus::NeverBuilt);
        }
    }

    statuses
}

/// Run `cmod graph` — visualize the module dependency graph.
pub fn run(format: Option<String>, filter: Option<String>, status: bool) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    let src_dir = config.src_dir();
    let sources = runner::discover_sources(&src_dir)?;

    if sources.is_empty() {
        eprintln!("  No source files found.");
        return Ok(());
    }

    let graph = build_module_graph(&sources, &config.manifest.package.name)?;

    let statuses = if status {
        let build_state = BuildState::load(&config.build_dir());
        compute_node_statuses(&graph, &build_state)
    } else {
        BTreeMap::new()
    };

    let format = match format.as_deref() {
        Some("dot") => GraphFormat::Dot,
        Some("json") => GraphFormat::Json,
        _ => GraphFormat::Ascii,
    };

    match format {
        GraphFormat::Ascii => print_ascii(&graph, &config.manifest.package.name, filter.as_deref(), &statuses),
        GraphFormat::Dot => print_dot(&graph, filter.as_deref(), &statuses),
        GraphFormat::Json => print_json(&graph, &statuses)?,
    }

    Ok(())
}

/// Print the graph as an ASCII tree.
fn print_ascii(graph: &ModuleGraph, root_name: &str, filter: Option<&str>, statuses: &BTreeMap<String, NodeStatus>) {
    let order = match graph.topological_order() {
        Ok(o) => o,
        Err(e) => {
            eprintln!("  Error: {}", e);
            return;
        }
    };

    println!("{}", root_name);

    let roots = graph.roots();
    let total = roots.len();

    for (i, root) in roots.iter().enumerate() {
        let is_last = i == total - 1;
        print_ascii_node(graph, root, "", is_last, filter, &mut BTreeSet::new(), &order, statuses);
    }
}

fn print_ascii_node(
    graph: &ModuleGraph,
    name: &str,
    indent: &str,
    is_last: bool,
    filter: Option<&str>,
    visited: &mut BTreeSet<String>,
    _order: &[String],
    statuses: &BTreeMap<String, NodeStatus>,
) {
    if let Some(pattern) = filter {
        if !name.contains(pattern) {
            return;
        }
    }

    let connector = if is_last { "└── " } else { "├── " };
    let child_indent = if is_last {
        format!("{}    ", indent)
    } else {
        format!("{}│   ", indent)
    };

    let node = &graph.nodes[name];
    let kind_label = format!("{:?}", node.kind);
    let status_str = statuses
        .get(name)
        .map(|s| format!(" {}", s.ascii_marker()))
        .unwrap_or_default();
    println!("{}{}{} ({}){}", indent, connector, name, kind_label, status_str);

    if !visited.insert(name.to_string()) {
        return;
    }

    let dependents = graph.dependents(name);
    let total = dependents.len();

    for (j, dep) in dependents.iter().enumerate() {
        let dep_is_last = j == total - 1;
        print_ascii_node(graph, dep, &child_indent, dep_is_last, filter, visited, _order, statuses);
    }

    visited.remove(name);
}

/// Print the graph in DOT format for Graphviz.
fn print_dot(graph: &ModuleGraph, filter: Option<&str>, statuses: &BTreeMap<String, NodeStatus>) {
    println!("digraph modules {{");
    println!("  rankdir=BT;");
    println!("  node [shape=box, style=\"rounded,filled\"];");

    for (name, node) in &graph.nodes {
        if let Some(pattern) = filter {
            if !name.contains(pattern) {
                continue;
            }
        }

        let shape = match node.kind {
            cmod_core::types::ModuleUnitKind::InterfaceUnit => "box",
            cmod_core::types::ModuleUnitKind::PartitionUnit => "box",
            cmod_core::types::ModuleUnitKind::ImplementationUnit => "ellipse",
            cmod_core::types::ModuleUnitKind::LegacyUnit => "diamond",
        };

        let fill_color = statuses
            .get(name)
            .map(|s| s.dot_color())
            .unwrap_or("white");

        let status_label = statuses
            .get(name)
            .map(|s| format!("\\n[{}]", s.label()))
            .unwrap_or_default();

        println!(
            "  \"{}\" [shape={}, fillcolor=\"{}\", label=\"{}\\n({:?}){}\"];",
            name, shape, fill_color, name, node.kind, status_label
        );
    }

    for (name, node) in &graph.nodes {
        if let Some(pattern) = filter {
            if !name.contains(pattern) {
                continue;
            }
        }

        for import in &node.imports {
            println!("  \"{}\" -> \"{}\";", name, import);
        }
    }

    println!("}}");
}

/// Print the graph as JSON, optionally with status annotations.
fn print_json(graph: &ModuleGraph, statuses: &BTreeMap<String, NodeStatus>) -> Result<(), CmodError> {
    if statuses.is_empty() {
        let json = serde_json::to_string_pretty(&graph.nodes).map_err(|e| {
            CmodError::Other(format!("failed to serialize graph: {}", e))
        })?;
        println!("{}", json);
    } else {
        // Build an enhanced JSON with status annotations
        let mut entries: BTreeMap<String, serde_json::Value> = BTreeMap::new();
        for (name, node) in &graph.nodes {
            let mut map = serde_json::to_value(node).unwrap_or_default();
            if let Some(status) = statuses.get(name) {
                if let serde_json::Value::Object(ref mut obj) = map {
                    obj.insert("status".to_string(), serde_json::to_value(status.label()).unwrap());
                }
            }
            entries.insert(name.clone(), map);
        }
        let json = serde_json::to_string_pretty(&entries).map_err(|e| {
            CmodError::Other(format!("failed to serialize graph: {}", e))
        })?;
        println!("{}", json);
    }
    Ok(())
}

/// Build a ModuleGraph from discovered source files.
fn build_module_graph(
    sources: &[std::path::PathBuf],
    package_name: &str,
) -> Result<ModuleGraph, CmodError> {
    use cmod_build::graph::ModuleNode;

    let mut graph = ModuleGraph::new();

    for source in sources {
        let kind = runner::classify_source(source)?;
        let module_name = runner::extract_module_name(source)?
            .unwrap_or_else(|| {
                source
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string()
            });

        let imports = extract_imports(source)?;

        graph.add_node(ModuleNode {
            name: module_name,
            kind,
            source: source.clone(),
            package: package_name.to_string(),
            imports,
        });
    }

    let known: BTreeSet<String> = graph.nodes.keys().cloned().collect();
    for node in graph.nodes.values_mut() {
        node.imports.retain(|imp| known.contains(imp));
    }

    Ok(graph)
}

fn extract_imports(path: &std::path::Path) -> Result<Vec<String>, CmodError> {
    let content = std::fs::read_to_string(path)?;
    let mut imports = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("import ") && trimmed.ends_with(';') {
            let module_name = trimmed
                .trim_start_matches("import ")
                .trim_end_matches(';')
                .trim();
            if !module_name.starts_with('<') && !module_name.starts_with('"') {
                imports.push(module_name.to_string());
            }
        }
    }

    Ok(imports)
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmod_build::graph::ModuleNode;
    use cmod_build::incremental::NodeState;
    use cmod_core::types::ModuleUnitKind;
    use std::path::PathBuf;

    fn make_graph() -> ModuleGraph {
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
        graph
    }

    #[test]
    fn test_print_dot_output() {
        let graph = make_graph();
        print_dot(&graph, None, &BTreeMap::new());
    }

    #[test]
    fn test_print_dot_with_filter() {
        let graph = make_graph();
        print_dot(&graph, Some("base"), &BTreeMap::new());
    }

    #[test]
    fn test_print_json_output() {
        let graph = make_graph();
        let result = print_json(&graph, &BTreeMap::new());
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_ascii_output() {
        let graph = make_graph();
        print_ascii(&graph, "test_project", None, &BTreeMap::new());
    }

    #[test]
    fn test_extract_imports() {
        let tmp = tempfile::TempDir::new().unwrap();
        let file = tmp.path().join("test.cppm");
        std::fs::write(&file, "export module mymod;\nimport base;\nimport utils;\n").unwrap();

        let imports = extract_imports(&file).unwrap();
        assert_eq!(imports, vec!["base", "utils"]);
    }

    #[test]
    fn test_compute_node_statuses_never_built() {
        let graph = make_graph();
        let state = BuildState::default();
        let statuses = compute_node_statuses(&graph, &state);

        assert_eq!(statuses["base"], NodeStatus::NeverBuilt);
        assert_eq!(statuses["app"], NodeStatus::NeverBuilt);
    }

    #[test]
    fn test_compute_node_statuses_up_to_date() {
        let graph = make_graph();
        let mut state = BuildState::default();
        state.nodes.insert(
            "interface:base".to_string(),
            NodeState {
                source_hash: "abc123".to_string(),
                dep_hashes: vec![],
                flags_hash: "flags".to_string(),
                output_hashes: vec![("base.pcm".to_string(), "hash1".to_string())],
            },
        );

        let statuses = compute_node_statuses(&graph, &state);
        assert_eq!(statuses["base"], NodeStatus::UpToDate);
        assert_eq!(statuses["app"], NodeStatus::NeverBuilt);
    }

    #[test]
    fn test_print_dot_with_status() {
        let graph = make_graph();
        let mut statuses = BTreeMap::new();
        statuses.insert("base".to_string(), NodeStatus::UpToDate);
        statuses.insert("app".to_string(), NodeStatus::NeedsRebuild);
        // Just verify it doesn't panic
        print_dot(&graph, None, &statuses);
    }

    #[test]
    fn test_print_json_with_status() {
        let graph = make_graph();
        let mut statuses = BTreeMap::new();
        statuses.insert("base".to_string(), NodeStatus::UpToDate);
        statuses.insert("app".to_string(), NodeStatus::NeverBuilt);
        let result = print_json(&graph, &statuses);
        assert!(result.is_ok());
    }
}
