use std::collections::{BTreeMap, BTreeSet};

use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::lockfile::{Lockfile, LockedPackage};
use cmod_resolver::Resolver;

/// Run `cmod deps` — display the dependency graph.
pub fn run(tree: bool, why: Option<String>, conflicts: bool) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    let lockfile = Lockfile::load(&config.lockfile_path).map_err(|_| {
        CmodError::LockfileNotFound
    })?;

    // --why <dep>: explain why a dependency is included
    if let Some(dep_name) = why {
        let reasons = Resolver::explain_dep(&lockfile, &dep_name);
        if reasons.is_empty() {
            eprintln!("  '{}' is not in the dependency graph.", dep_name);
        } else {
            for reason in &reasons {
                println!("  {}", reason);
            }
        }
        return Ok(());
    }

    // --conflicts: show shared transitive deps
    if conflicts {
        let found = Resolver::check_conflicts(&lockfile);
        if found.is_empty() {
            eprintln!("  No version conflicts detected.");
        } else {
            for c in &found {
                println!(
                    "  {} v{} — required by: {}",
                    c.package,
                    c.resolved_version,
                    c.requesters.join(", ")
                );
            }
        }
        return Ok(());
    }

    if lockfile.is_empty() {
        eprintln!("  No dependencies.");
        return Ok(());
    }

    if tree {
        print_tree(&config.manifest.package.name, &lockfile);
    } else {
        print_flat(&lockfile);
    }

    Ok(())
}

/// Print dependencies as a flat list.
fn print_flat(lockfile: &Lockfile) {
    for pkg in &lockfile.packages {
        let source_info = if let Some(ref commit) = pkg.commit {
            format!("{}#{}", pkg.repo.as_deref().unwrap_or("?"), &commit[..8.min(commit.len())])
        } else {
            "local".to_string()
        };
        println!("{} v{} ({})", pkg.name, pkg.version, source_info);
    }
    println!();
    println!("{} dependencies total.", lockfile.packages.len());
}

/// Print dependencies as an indented tree with box-drawing characters.
fn print_tree(root_name: &str, lockfile: &Lockfile) {
    // Build an index of packages by name
    let pkg_map: BTreeMap<&str, &LockedPackage> = lockfile
        .packages
        .iter()
        .map(|p| (p.name.as_str(), p))
        .collect();

    // Find root-level deps (not a transitive dep of any other package)
    let all_transitive: BTreeSet<&str> = lockfile
        .packages
        .iter()
        .flat_map(|p| p.deps.iter().map(|d| d.as_str()))
        .collect();

    let root_deps: Vec<&LockedPackage> = lockfile
        .packages
        .iter()
        .filter(|p| !all_transitive.contains(p.name.as_str()))
        .collect();

    println!("{} v{}", root_name, "0.0.0");

    let total = root_deps.len();
    for (i, pkg) in root_deps.iter().enumerate() {
        let is_last = i == total - 1;
        print_tree_node(pkg, &pkg_map, "", is_last, &mut BTreeSet::new());
    }
}

/// Recursively print a tree node.
fn print_tree_node(
    pkg: &LockedPackage,
    pkg_map: &BTreeMap<&str, &LockedPackage>,
    indent: &str,
    is_last: bool,
    visited: &mut BTreeSet<String>,
) {
    let connector = if is_last { "└── " } else { "├── " };
    let child_indent = if is_last {
        format!("{}    ", indent)
    } else {
        format!("{}│   ", indent)
    };

    println!("{}{}{} v{}", indent, connector, pkg.name, pkg.version);

    // Avoid infinite recursion on cycles
    if !visited.insert(pkg.name.clone()) {
        return;
    }

    let total = pkg.deps.len();
    for (j, dep_name) in pkg.deps.iter().enumerate() {
        let dep_is_last = j == total - 1;
        if let Some(dep_pkg) = pkg_map.get(dep_name.as_str()) {
            print_tree_node(dep_pkg, pkg_map, &child_indent, dep_is_last, visited);
        } else {
            let dep_connector = if dep_is_last { "└── " } else { "├── " };
            println!("{}{}{} (unresolved)", child_indent, dep_connector, dep_name);
        }
    }

    visited.remove(&pkg.name);
}
