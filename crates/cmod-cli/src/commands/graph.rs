use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use cmod_build::graph::{ModuleGraph, ModuleNode};
use cmod_build::incremental::BuildState;
use cmod_build::runner;
use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::shell::Shell;
use cmod_core::types::ModuleUnitKind;

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

/// The key the build plan gives a node's step in the build state (see
/// `cmod_build::plan`): `interface:<module>` for interfaces and partitions,
/// `impl:` or `object:` with the module and the sanitized source path for
/// other units.
fn state_key(node: &ModuleNode) -> String {
    let source = node
        .source
        .display()
        .to_string()
        .replace(['.', ':', '/'], "_");
    match node.kind {
        ModuleUnitKind::InterfaceUnit | ModuleUnitKind::PartitionUnit => {
            format!("interface:{}", node.name)
        }
        ModuleUnitKind::ImplementationUnit => format!("impl:{}:{}", node.name, source),
        ModuleUnitKind::LegacyUnit => format!("object:{}:{}", node.name, source),
    }
}

/// Compute the build status of each node (by node ID) from the build state.
fn compute_node_statuses(
    graph: &ModuleGraph,
    build_state: &BuildState,
) -> BTreeMap<String, NodeStatus> {
    graph
        .nodes
        .iter()
        .map(|(id, node)| {
            let status = match build_state.nodes.get(&state_key(node)) {
                None => NodeStatus::NeverBuilt,
                Some(ns) if !ns.source_hash.is_empty() && !ns.output_hashes.is_empty() => {
                    NodeStatus::UpToDate
                }
                Some(_) => NodeStatus::NeedsRebuild,
            };
            (id.clone(), status)
        })
        .collect()
}

/// A node's build time (by node ID), from the build state's timings.
fn node_timings(graph: &ModuleGraph, timings: &BTreeMap<String, u64>) -> BTreeMap<String, u64> {
    graph
        .nodes
        .iter()
        .filter_map(|(id, node)| timings.get(&state_key(node)).map(|&ms| (id.clone(), ms)))
        .collect()
}

/// How the graph shows a package's units and what they import.
struct GraphView<'a> {
    graph: &'a ModuleGraph,
    /// Node ID → label: the module for interfaces and partitions, the path
    /// relative to the package for other units.
    labels: BTreeMap<String, String>,
    /// Module of a dependency → the dependency providing it.
    dependency_modules: BTreeMap<String, String>,
}

/// What a unit imports: another unit of the package, or a module of a
/// dependency.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
enum Edge {
    Node(String),
    External(String),
}

impl<'a> GraphView<'a> {
    fn new(
        graph: &'a ModuleGraph,
        root: &Path,
        dependency_modules: BTreeMap<String, String>,
    ) -> Self {
        let labels = graph
            .nodes
            .iter()
            .map(|(id, node)| {
                let label = match node.kind {
                    ModuleUnitKind::InterfaceUnit | ModuleUnitKind::PartitionUnit => {
                        node.name.clone()
                    }
                    _ => node
                        .source
                        .strip_prefix(root)
                        .unwrap_or(&node.source)
                        .to_string_lossy()
                        .replace('\\', "/"),
                };
                (id.clone(), label)
            })
            .collect();
        GraphView {
            graph,
            labels,
            dependency_modules,
        }
    }

    fn label<'s>(&'s self, id: &'s str) -> &'s str {
        self.labels.get(id).map(String::as_str).unwrap_or(id)
    }

    /// `module`, with the dependency providing it when known.
    fn external_label(&self, module: &str) -> String {
        match self.dependency_modules.get(module) {
            Some(dep) => format!("{} (dependency {})", module, dep),
            None => format!("{} (dependency)", module),
        }
    }

    /// What node `id` imports, in order: units of the package, then
    /// modules of dependencies.
    fn edges(&self, id: &str) -> Vec<Edge> {
        let node = &self.graph.nodes[id];
        let mut edges: Vec<Edge> = node
            .imports
            .iter()
            .filter_map(|module| self.graph.interface_for(module))
            .map(|provider| Edge::Node(provider.id.clone()))
            .collect();
        edges.extend(
            self.graph
                .external_imports
                .get(id)
                .into_iter()
                .flatten()
                .map(|module| Edge::External(module.clone())),
        );
        edges.dedup();
        edges
    }

    /// The units nothing in the package imports: executables' sources,
    /// implementation units and the interfaces the package exports. Sorted
    /// by label.
    fn entry_points(&self) -> Vec<&'a str> {
        let imported: BTreeSet<&str> = self
            .graph
            .nodes
            .values()
            .flat_map(|n| n.imports.iter().map(String::as_str))
            .collect();
        let mut entries: Vec<&'a str> = self
            .graph
            .nodes
            .iter()
            .filter(|(_, node)| {
                !matches!(
                    node.kind,
                    ModuleUnitKind::InterfaceUnit | ModuleUnitKind::PartitionUnit
                ) || !imported.contains(node.name.as_str())
            })
            .map(|(id, _)| id.as_str())
            .collect();
        entries.sort_by_key(|id| self.label(id).to_string());
        entries
    }

    /// The nodes a graph starts from: those whose label contains `filter`,
    /// or the entry points.
    fn roots(&self, filter: Option<&str>) -> Vec<&'a str> {
        match filter {
            Some(pattern) => {
                let mut roots: Vec<&'a str> = self
                    .graph
                    .nodes
                    .keys()
                    .map(String::as_str)
                    .filter(|id| self.label(id).contains(pattern))
                    .collect();
                roots.sort_by_key(|id| self.label(id).to_string());
                roots
            }
            None => self.entry_points(),
        }
    }
}

/// The modules each dependency of the package provides: the module its
/// manifest names and those its interfaces declare, mapped to the
/// dependency's name. Dependencies not on disk yet are left out.
fn dependency_modules(config: &Config) -> BTreeMap<String, String> {
    let mut modules = BTreeMap::new();
    for (name, dep) in &config.manifest.dependencies {
        let dir = match dep.path() {
            Some(path) => Some(config.root.join(path)),
            None => super::common::find_dep_on_disk(
                &config.root.join("vendor"),
                &config.deps_dir(),
                name,
            ),
        };
        let Some(dep_config) = dir
            .filter(|d| d.join("cmod.toml").is_file())
            .and_then(|d| Config::load(&d).ok())
        else {
            continue;
        };
        if let Some(module) = &dep_config.manifest.module {
            modules.insert(module.name.clone(), name.clone());
        }
        let sources =
            runner::discover_sources_multi(&dep_config.src_dirs(), &dep_config.exclude_patterns())
                .unwrap_or_default();
        for source in sources {
            if matches!(
                runner::classify_source(&source),
                Ok(ModuleUnitKind::InterfaceUnit | ModuleUnitKind::PartitionUnit)
            ) {
                if let Ok(Some(module)) = runner::extract_module_name(&source) {
                    modules.insert(module, name.clone());
                }
            }
        }
    }
    modules
}

/// Run `cmod graph` — visualize the module dependency graph.
pub fn run(
    format: Option<String>,
    filter: Option<String>,
    status: bool,
    critical_path: bool,
    timing: bool,
    shell: &Shell,
) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    let src_dirs = config.src_dirs();
    let exclude = config.exclude_patterns();
    let sources = runner::discover_sources_multi(&src_dirs, &exclude)?;

    if sources.is_empty() {
        if config.manifest.is_workspace() {
            let members = cmod_workspace::WorkspaceManager::load(&config.root)
                .map(|ws| {
                    ws.members
                        .iter()
                        .map(|m| m.name.clone())
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            shell.warn(format!(
                "workspace root has no sources; the graph is per-member — \
                 cd into a member and run `cmod graph` there (members: {})",
                if members.is_empty() { "none" } else { &members }
            ));
        } else {
            shell.status("Graph", "no source files found");
        }
        return Ok(());
    }

    let graph = super::build::build_module_graph(&sources, &config.manifest.package.name, None)?;
    let view = GraphView::new(&graph, &config.root, dependency_modules(&config));

    let statuses = if status {
        let build_state = BuildState::load(&config.build_dir());
        compute_node_statuses(&graph, &build_state)
    } else {
        BTreeMap::new()
    };

    // Show critical path if requested
    if critical_path {
        // Persisted node timings, by graph node ID as `critical_path` reads
        // them; 1ms per node when no build recorded any.
        let recorded = BuildState::load(&config.build_dir()).node_timings;
        let timings: BTreeMap<String, u64> = if recorded.is_empty() {
            graph.nodes.keys().map(|k| (k.clone(), 1)).collect()
        } else {
            node_timings(&graph, &recorded)
        };
        let path = graph.critical_path(&timings);
        if path.is_empty() {
            shell.status("Critical", "no critical path (empty graph or no timings)");
        } else {
            let total_ms: u64 = path.iter().filter_map(|n| timings.get(n)).sum();
            shell.status(
                "Critical",
                format!("path ({} nodes, {}ms)", path.len(), total_ms),
            );
            for node_id in &path {
                let ms = timings.get(node_id).copied().unwrap_or(0);
                shell.verbose("Node", format!("{} ({}ms)", view.label(node_id), ms));
            }
        }
    }

    // Load timing data if requested
    let timings = if timing {
        node_timings(&graph, &BuildState::load(&config.build_dir()).node_timings)
    } else {
        BTreeMap::new()
    };

    let format = match format.as_deref() {
        Some("dot") => GraphFormat::Dot,
        Some("json") => GraphFormat::Json,
        _ => GraphFormat::Ascii,
    };

    match format {
        GraphFormat::Ascii => print!(
            "{}",
            render_ascii(
                &view,
                &config.manifest.package.name,
                filter.as_deref(),
                &statuses
            )
        ),
        GraphFormat::Dot => print!(
            "{}",
            render_dot(&view, filter.as_deref(), &statuses, &timings)
        ),
        GraphFormat::Json => println!("{}", render_json(&view, &statuses, &timings)?),
    }

    Ok(())
}

/// The graph as a tree, as `cargo tree` prints one: the package, then
/// each entry point (or each unit matching `filter`) with what it imports
/// beneath it. A unit already shown is marked `(*)` instead of repeated.
fn render_ascii(
    view: &GraphView,
    root_name: &str,
    filter: Option<&str>,
    statuses: &BTreeMap<String, NodeStatus>,
) -> String {
    let mut out = format!("{}\n", root_name);
    let roots = view.roots(filter);
    let mut shown = BTreeSet::new();
    for (i, id) in roots.iter().enumerate() {
        let edge = Edge::Node(id.to_string());
        render_ascii_edge(
            view,
            &edge,
            "",
            i + 1 == roots.len(),
            statuses,
            &mut shown,
            &mut out,
        );
    }
    out
}

fn render_ascii_edge(
    view: &GraphView,
    edge: &Edge,
    indent: &str,
    is_last: bool,
    statuses: &BTreeMap<String, NodeStatus>,
    shown: &mut BTreeSet<String>,
    out: &mut String,
) {
    let connector = if is_last { "└── " } else { "├── " };
    let id = match edge {
        Edge::External(module) => {
            out.push_str(&format!(
                "{}{}{}\n",
                indent,
                connector,
                view.external_label(module)
            ));
            return;
        }
        Edge::Node(id) => id,
    };
    let status = statuses
        .get(id)
        .map(|s| format!(" {}", s.ascii_marker()))
        .unwrap_or_default();
    let edges = view.edges(id);
    let repeated = !edges.is_empty() && !shown.insert(id.clone());
    out.push_str(&format!(
        "{}{}{}{}{}\n",
        indent,
        connector,
        view.label(id),
        status,
        if repeated { " (*)" } else { "" }
    ));
    if repeated {
        return;
    }
    let child_indent = format!("{}{}", indent, if is_last { "    " } else { "│   " });
    for (j, child) in edges.iter().enumerate() {
        render_ascii_edge(
            view,
            child,
            &child_indent,
            j + 1 == edges.len(),
            statuses,
            shown,
            out,
        );
    }
}

/// Color-code a node by compilation time for timing visualization.
/// Green for fast (<100ms), yellow for moderate (100-500ms), red for slow (>500ms).
fn timing_color(ms: u64) -> &'static str {
    if ms < 100 {
        "palegreen"
    } else if ms < 500 {
        "lightyellow"
    } else {
        "lightcoral"
    }
}

/// `text` inside a DOT double-quoted string.
fn dot_quote(text: &str) -> String {
    text.replace('\\', "\\\\").replace('"', "\\\"")
}

/// The graph in DOT format for Graphviz: one node per unit, labelled as in
/// the tree, and one per dependency module imported (dashed), with an edge
/// from each unit to what it imports. With `filter`, the units matching it
/// and what they import.
fn render_dot(
    view: &GraphView,
    filter: Option<&str>,
    statuses: &BTreeMap<String, NodeStatus>,
    timings: &BTreeMap<String, u64>,
) -> String {
    let mut out = String::from(
        "digraph modules {\n  rankdir=BT;\n  node [shape=box, style=\"rounded,filled\"];\n",
    );

    // The units to show and their edges.
    let ids: Vec<&str> = match filter {
        Some(_) => view.roots(filter),
        None => view.graph.nodes.keys().map(String::as_str).collect(),
    };
    let mut externals = BTreeSet::new();
    let mut edges = Vec::new();
    for id in &ids {
        for edge in view.edges(id) {
            let target = match &edge {
                Edge::Node(target) => view.label(target).to_string(),
                Edge::External(module) => {
                    externals.insert(module.clone());
                    module.clone()
                }
            };
            edges.push((view.label(id).to_string(), target));
        }
    }
    // With a filter, the units imported are shown too.
    let mut shown: BTreeSet<&str> = ids.iter().copied().collect();
    for id in &ids {
        for edge in view.edges(id) {
            if let Edge::Node(target) = edge {
                if let Some((key, _)) = view.graph.nodes.get_key_value(&target) {
                    shown.insert(key.as_str());
                }
            }
        }
    }

    for id in shown {
        let node = &view.graph.nodes[id];
        let shape = match node.kind {
            ModuleUnitKind::InterfaceUnit | ModuleUnitKind::PartitionUnit => "box",
            ModuleUnitKind::ImplementationUnit => "ellipse",
            ModuleUnitKind::LegacyUnit => "diamond",
        };
        let node_timing = timings.get(id).copied();
        let fill_color = match node_timing {
            Some(ms) => timing_color(ms),
            None => statuses.get(id).map(|s| s.dot_color()).unwrap_or("white"),
        };
        let status_label = statuses
            .get(id)
            .map(|s| format!("\\n[{}]", s.label()))
            .unwrap_or_default();
        let timing_label = node_timing
            .map(|ms| format!("\\n{}ms", ms))
            .unwrap_or_default();
        let label = dot_quote(view.label(id));
        out.push_str(&format!(
            "  \"{}\" [shape={}, fillcolor=\"{}\", label=\"{}\\n({:?}){}{}\"];\n",
            label, shape, fill_color, label, node.kind, status_label, timing_label
        ));
    }
    for module in &externals {
        out.push_str(&format!(
            "  \"{}\" [shape=box, style=\"rounded,dashed\", label=\"{}\"];\n",
            dot_quote(module),
            dot_quote(&view.external_label(module))
        ));
    }
    for (from, to) in edges {
        out.push_str(&format!(
            "  \"{}\" -> \"{}\";\n",
            dot_quote(&from),
            dot_quote(&to)
        ));
    }
    out.push_str("}\n");
    out
}

/// The graph as JSON: each unit keyed by its label, with its fields, the
/// dependency modules it imports (`external_imports`), and with `--status`
/// and `--timing` its `status` and `build_time_ms`.
fn render_json(
    view: &GraphView,
    statuses: &BTreeMap<String, NodeStatus>,
    timings: &BTreeMap<String, u64>,
) -> Result<String, CmodError> {
    let mut entries: BTreeMap<String, serde_json::Value> = BTreeMap::new();
    for (id, node) in &view.graph.nodes {
        let mut value = serde_json::to_value(node)
            .map_err(|e| CmodError::Other(format!("failed to serialize graph: {}", e)))?;
        if let serde_json::Value::Object(ref mut obj) = value {
            if let Some(external) = view.graph.external_imports.get(id) {
                obj.insert("external_imports".to_string(), serde_json::json!(external));
            }
            if let Some(status) = statuses.get(id) {
                obj.insert("status".to_string(), serde_json::json!(status.label()));
            }
            if let Some(&ms) = timings.get(id) {
                obj.insert("build_time_ms".to_string(), serde_json::json!(ms));
            }
        }
        entries.insert(view.label(id).to_string(), value);
    }
    serde_json::to_string_pretty(&entries)
        .map_err(|e| CmodError::Other(format!("failed to serialize graph: {}", e)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmod_build::incremental::NodeState;
    use cmod_core::types::{BuildType, Profile};
    use std::path::PathBuf;

    fn node(path: &str, name: &str, kind: ModuleUnitKind, imports: &[&str]) -> ModuleNode {
        ModuleNode {
            id: format!("/pkg/{}", path),
            name: name.to_string(),
            kind,
            source: PathBuf::from(format!("/pkg/{}", path)),
            package: "pkg".to_string(),
            imports: imports.iter().map(|s| s.to_string()).collect(),
            partition_of: None,
        }
    }

    /// A library `geo` with a partition and an implementation unit, used
    /// by `main.cpp` and `tool.cpp`, and importing `fmt` from a dependency.
    fn sample_graph() -> ModuleGraph {
        let mut graph = ModuleGraph::new();
        graph.add_node(node(
            "src/geo.cppm",
            "geo",
            ModuleUnitKind::InterfaceUnit,
            &["geo:vec"],
        ));
        graph.add_node(node(
            "src/vec.cppm",
            "geo:vec",
            ModuleUnitKind::PartitionUnit,
            &[],
        ));
        graph.add_node(node(
            "src/geo.cpp",
            "geo",
            ModuleUnitKind::ImplementationUnit,
            &[],
        ));
        graph.add_node(node(
            "src/main.cpp",
            "main",
            ModuleUnitKind::LegacyUnit,
            &["geo"],
        ));
        graph.add_node(node(
            "src/tool.cpp",
            "tool",
            ModuleUnitKind::LegacyUnit,
            &["geo"],
        ));
        graph
            .external_imports
            .insert("/pkg/src/geo.cppm".to_string(), vec!["fmt".to_string()]);
        graph
    }

    fn view(graph: &ModuleGraph) -> GraphView<'_> {
        let deps = BTreeMap::from([("fmt".to_string(), "github.com/fmtlib/fmt".to_string())]);
        GraphView::new(graph, Path::new("/pkg"), deps)
    }

    #[test]
    fn test_ascii_tree_follows_imports() {
        let graph = sample_graph();
        let out = render_ascii(&view(&graph), "pkg", None, &BTreeMap::new());
        assert_eq!(
            out,
            "pkg
├── src/geo.cpp
├── src/main.cpp
│   └── geo
│       ├── geo:vec
│       └── fmt (dependency github.com/fmtlib/fmt)
└── src/tool.cpp
    └── geo (*)
"
        );
    }

    #[test]
    fn test_ascii_filter_starts_from_matching_units() {
        let graph = sample_graph();
        let out = render_ascii(&view(&graph), "pkg", Some("vec"), &BTreeMap::new());
        assert_eq!(out, "pkg\n└── geo:vec\n");
    }

    #[test]
    fn test_dot_has_edges_and_dependency_nodes() {
        let graph = sample_graph();
        let out = render_dot(&view(&graph), None, &BTreeMap::new(), &BTreeMap::new());
        assert!(out.contains("\"src/main.cpp\" -> \"geo\";"), "{}", out);
        assert!(out.contains("\"geo\" -> \"geo:vec\";"), "{}", out);
        assert!(out.contains("\"geo\" -> \"fmt\";"), "{}", out);
        assert!(
            out.contains("\"fmt\" [shape=box, style=\"rounded,dashed\", label=\"fmt (dependency github.com/fmtlib/fmt)\"];"),
            "{}",
            out
        );
        // With a filter: the matching unit and what it imports.
        let out = render_dot(
            &view(&graph),
            Some("main"),
            &BTreeMap::new(),
            &BTreeMap::new(),
        );
        assert!(out.contains("\"src/main.cpp\" [shape=diamond"), "{}", out);
        assert!(out.contains("\"geo\" [shape=box"), "{}", out);
        assert!(!out.contains("tool"), "{}", out);
    }

    #[test]
    fn test_dot_quotes_labels() {
        assert_eq!(dot_quote(r#"a"b\c"#), r#"a\"b\\c"#);
    }

    #[test]
    fn test_json_keys_units_by_label() {
        let graph = sample_graph();
        let statuses = BTreeMap::from([("/pkg/src/geo.cppm".to_string(), NodeStatus::UpToDate)]);
        let timings = BTreeMap::from([("/pkg/src/geo.cppm".to_string(), 120u64)]);
        let json: serde_json::Value =
            serde_json::from_str(&render_json(&view(&graph), &statuses, &timings).unwrap())
                .unwrap();
        assert_eq!(json["geo"]["name"], "geo");
        assert_eq!(json["geo"]["status"], "up-to-date");
        assert_eq!(json["geo"]["build_time_ms"], 120);
        assert_eq!(json["geo"]["external_imports"], serde_json::json!(["fmt"]));
        assert_eq!(json["src/main.cpp"]["imports"], serde_json::json!(["geo"]));
        assert!(json.get("src/geo.cpp").is_some());
    }

    /// The keys the graph reads statuses and timings by are the build
    /// plan's node IDs.
    #[test]
    fn test_state_keys_are_the_build_plans_node_ids() {
        let mut graph = sample_graph();
        graph.external_imports.clear();
        let plan = cmod_build::plan::BuildPlan::from_graph(
            &graph,
            Path::new("/pkg/build"),
            "x86_64-unknown-linux-gnu",
            Profile::Debug,
            BuildType::Binary,
            Some("pkg"),
            "pcm",
        )
        .unwrap();
        let plan_ids: BTreeSet<String> = plan.nodes.iter().map(|n| n.id.clone()).collect();
        for node in graph.nodes.values() {
            assert!(
                plan_ids.contains(&state_key(node)),
                "{} not among {:?}",
                state_key(node),
                plan_ids
            );
        }
    }

    #[test]
    fn test_statuses_and_timings_by_node() {
        let graph = sample_graph();
        let main = &graph.nodes["/pkg/src/main.cpp"];
        let mut state = BuildState::default();
        state.nodes.insert(
            state_key(main),
            NodeState {
                source_hash: "abc".to_string(),
                dep_hashes: vec![],
                flags_hash: "flags".to_string(),
                output_hashes: vec![("main.o".to_string(), "h".to_string())],
                mtime: None,
                headers: None,
            },
        );
        state.nodes.insert(
            "interface:geo".to_string(),
            NodeState {
                source_hash: "abc".to_string(),
                dep_hashes: vec![],
                flags_hash: "flags".to_string(),
                output_hashes: vec![],
                mtime: None,
                headers: None,
            },
        );
        let statuses = compute_node_statuses(&graph, &state);
        assert_eq!(statuses["/pkg/src/main.cpp"], NodeStatus::UpToDate);
        assert_eq!(statuses["/pkg/src/geo.cppm"], NodeStatus::NeedsRebuild);
        assert_eq!(statuses["/pkg/src/tool.cpp"], NodeStatus::NeverBuilt);

        let timings = BTreeMap::from([("interface:geo".to_string(), 42u64)]);
        assert_eq!(
            node_timings(&graph, &timings),
            BTreeMap::from([("/pkg/src/geo.cppm".to_string(), 42u64)])
        );
    }

    #[test]
    fn test_timing_color_thresholds() {
        assert_eq!(timing_color(0), "palegreen");
        assert_eq!(timing_color(99), "palegreen");
        assert_eq!(timing_color(100), "lightyellow");
        assert_eq!(timing_color(499), "lightyellow");
        assert_eq!(timing_color(500), "lightcoral");
        assert_eq!(timing_color(5000), "lightcoral");
    }
}
