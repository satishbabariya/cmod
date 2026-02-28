use std::collections::BTreeSet;

use cmod_build::graph::ModuleGraph;
use cmod_build::runner;
use cmod_core::config::Config;
use cmod_core::error::CmodError;

/// Output format for the graph command.
pub enum GraphFormat {
    Ascii,
    Dot,
    Json,
}

/// Run `cmod graph` — visualize the module dependency graph.
pub fn run(format: Option<String>, filter: Option<String>) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    let src_dir = config.src_dir();
    let sources = runner::discover_sources(&src_dir)?;

    if sources.is_empty() {
        eprintln!("  No source files found.");
        return Ok(());
    }

    let graph = build_module_graph(&sources, &config.manifest.package.name)?;

    let format = match format.as_deref() {
        Some("dot") => GraphFormat::Dot,
        Some("json") => GraphFormat::Json,
        _ => GraphFormat::Ascii,
    };

    match format {
        GraphFormat::Ascii => print_ascii(&graph, &config.manifest.package.name, filter.as_deref()),
        GraphFormat::Dot => print_dot(&graph, filter.as_deref()),
        GraphFormat::Json => print_json(&graph)?,
    }

    Ok(())
}

/// Print the graph as an ASCII tree.
fn print_ascii(graph: &ModuleGraph, root_name: &str, filter: Option<&str>) {
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
        print_ascii_node(graph, root, "", is_last, filter, &mut BTreeSet::new(), &order);
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
) {
    // Skip nodes that don't match the filter
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
    println!("{}{}{} ({})", indent, connector, name, kind_label);

    if !visited.insert(name.to_string()) {
        return; // Already visited
    }

    // Find dependents (modules that import this one)
    let dependents = graph.dependents(name);
    let total = dependents.len();

    for (j, dep) in dependents.iter().enumerate() {
        let dep_is_last = j == total - 1;
        print_ascii_node(graph, dep, &child_indent, dep_is_last, filter, visited, _order);
    }

    visited.remove(name);
}

/// Print the graph in DOT format for Graphviz.
fn print_dot(graph: &ModuleGraph, filter: Option<&str>) {
    println!("digraph modules {{");
    println!("  rankdir=BT;");
    println!("  node [shape=box, style=rounded];");

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
        println!("  \"{}\" [shape={}, label=\"{}\\n({:?})\"];", name, shape, name, node.kind);
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

/// Print the graph as JSON.
fn print_json(graph: &ModuleGraph) -> Result<(), CmodError> {
    let json = serde_json::to_string_pretty(&graph.nodes).map_err(|e| {
        CmodError::Other(format!("failed to serialize graph: {}", e))
    })?;
    println!("{}", json);
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

    // Filter imports to only include modules that exist in the graph
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
        // Just verify it doesn't panic
        print_dot(&graph, None);
    }

    #[test]
    fn test_print_dot_with_filter() {
        let graph = make_graph();
        print_dot(&graph, Some("base"));
    }

    #[test]
    fn test_print_json_output() {
        let graph = make_graph();
        let result = print_json(&graph);
        assert!(result.is_ok());
    }

    #[test]
    fn test_print_ascii_output() {
        let graph = make_graph();
        print_ascii(&graph, "test_project", None);
    }

    #[test]
    fn test_extract_imports() {
        let tmp = tempfile::TempDir::new().unwrap();
        let file = tmp.path().join("test.cppm");
        std::fs::write(&file, "export module mymod;\nimport base;\nimport utils;\n").unwrap();

        let imports = extract_imports(&file).unwrap();
        assert_eq!(imports, vec!["base", "utils"]);
    }
}
