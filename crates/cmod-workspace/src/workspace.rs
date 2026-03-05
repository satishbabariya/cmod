use std::collections::{BTreeMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use cmod_core::error::CmodError;
use cmod_core::manifest::{Dependency, Manifest};

/// A resolved workspace member.
#[derive(Debug, Clone)]
pub struct WorkspaceMember {
    /// Member name (directory name).
    pub name: String,
    /// Absolute path to the member directory.
    pub path: PathBuf,
    /// Parsed manifest for this member.
    pub manifest: Manifest,
}

/// Manages workspace/monorepo operations.
///
/// A workspace is defined by a root `cmod.toml` with a `[workspace]` section.
/// It contains multiple member modules that share a single lockfile.
pub struct WorkspaceManager {
    /// Root directory of the workspace.
    pub root: PathBuf,
    /// The root workspace manifest.
    pub root_manifest: Manifest,
    /// Resolved workspace members.
    pub members: Vec<WorkspaceMember>,
}

impl WorkspaceManager {
    /// Load a workspace from a root manifest.
    ///
    /// Member patterns support glob syntax (e.g., `"crates/*"`, `"libs/**"`).
    /// Exclude patterns filter out matching directories.
    pub fn load(root: &Path) -> Result<Self, CmodError> {
        let manifest_path = root.join("cmod.toml");
        let root_manifest = Manifest::load(&manifest_path)?;

        if !root_manifest.is_workspace() {
            return Err(CmodError::WorkspaceManifestNotFound {
                path: manifest_path.display().to_string(),
            });
        }

        let workspace = root_manifest.workspace.as_ref().unwrap();

        // Expand glob patterns and collect all candidate directories
        let expanded = expand_member_patterns(root, &workspace.members)?;

        // Build set of excluded paths
        let excluded = expand_exclude_patterns(root, &workspace.exclude);

        let mut members = Vec::new();

        for (name, member_dir) in expanded {
            // Skip excluded directories
            if excluded.contains(&member_dir) {
                continue;
            }

            let member_manifest_path = member_dir.join("cmod.toml");
            if member_manifest_path.exists() {
                let mut member_manifest = Manifest::load(&member_manifest_path)?;

                // Resolve workspace dependency references
                resolve_workspace_deps(&mut member_manifest, &workspace.dependencies);

                members.push(WorkspaceMember {
                    name,
                    path: member_dir,
                    manifest: member_manifest,
                });
            }
        }

        Ok(WorkspaceManager {
            root: root.to_path_buf(),
            root_manifest,
            members,
        })
    }

    /// Get the path to the shared lockfile.
    pub fn lockfile_path(&self) -> PathBuf {
        self.root.join("cmod.lock")
    }

    /// Collect all dependencies across all members (unified).
    pub fn all_dependencies(&self) -> Result<BTreeMap<String, Dependency>, CmodError> {
        let mut all_deps: BTreeMap<String, Dependency> = BTreeMap::new();

        // Add workspace-level dependencies first
        if let Some(ws) = &self.root_manifest.workspace {
            for (name, dep) in &ws.dependencies {
                all_deps.insert(name.clone(), dep.clone());
            }
        }

        // Add per-member dependencies, checking for conflicts
        for member in &self.members {
            for (name, dep) in &member.manifest.dependencies {
                if let Some(existing) = all_deps.get(name) {
                    // Check for version conflicts
                    let existing_ver = existing.version_req();
                    let new_ver = dep.version_req();
                    if existing_ver != new_ver && !dep.is_workspace() {
                        return Err(CmodError::VersionConflict {
                            name: name.clone(),
                            reason: format!(
                                "member '{}' requires '{}' but workspace has '{}'",
                                member.name,
                                new_ver.unwrap_or("*"),
                                existing_ver.unwrap_or("*"),
                            ),
                        });
                    }
                } else if !dep.is_workspace() {
                    all_deps.insert(name.clone(), dep.clone());
                }
            }
        }

        Ok(all_deps)
    }

    /// Find a member by name.
    pub fn find_member(&self, name: &str) -> Option<&WorkspaceMember> {
        self.members.iter().find(|m| m.name == name)
    }

    /// List all member names.
    pub fn member_names(&self) -> Vec<&str> {
        self.members.iter().map(|m| m.name.as_str()).collect()
    }

    /// Check if the workspace is properly configured.
    pub fn validate(&self) -> Result<(), CmodError> {
        if self.members.is_empty() {
            return Err(CmodError::InvalidManifest {
                reason: "workspace has no members".to_string(),
            });
        }

        // Check for duplicate member names
        let mut seen = std::collections::HashSet::new();
        for member in &self.members {
            if !seen.insert(&member.name) {
                return Err(CmodError::InvalidManifest {
                    reason: format!("duplicate workspace member: {}", member.name),
                });
            }
        }

        // Validate dependencies don't conflict
        self.all_dependencies()?;

        Ok(())
    }

    /// Compute the build order for workspace members.
    ///
    /// Members that depend on other members (via path deps) must build
    /// after their dependencies. Returns members in topological order.
    pub fn build_order(&self) -> Result<Vec<&WorkspaceMember>, CmodError> {
        let member_names: HashSet<&str> = self.members.iter().map(|m| m.name.as_str()).collect();
        let name_to_idx: BTreeMap<&str, usize> = self
            .members
            .iter()
            .enumerate()
            .map(|(i, m)| (m.name.as_str(), i))
            .collect();

        let n = self.members.len();
        let mut in_degree = vec![0usize; n];
        let mut dependents = vec![Vec::new(); n];

        for (idx, member) in self.members.iter().enumerate() {
            for (dep_name, dep) in &member.manifest.dependencies {
                if dep.is_path() && member_names.contains(dep_name.as_str()) {
                    if let Some(&dep_idx) = name_to_idx.get(dep_name.as_str()) {
                        in_degree[idx] += 1;
                        dependents[dep_idx].push(idx);
                    }
                }
            }
        }

        // Kahn's algorithm for topological sort
        let mut queue: Vec<usize> = (0..n).filter(|&i| in_degree[i] == 0).collect();
        let mut order = Vec::with_capacity(n);

        while let Some(idx) = queue.pop() {
            order.push(idx);
            for &dep_idx in &dependents[idx] {
                in_degree[dep_idx] -= 1;
                if in_degree[dep_idx] == 0 {
                    queue.push(dep_idx);
                }
            }
        }

        if order.len() != n {
            return Err(CmodError::CircularDependency {
                cycle: "workspace members have circular path dependencies".to_string(),
            });
        }

        Ok(order.iter().map(|&i| &self.members[i]).collect())
    }

    /// Get the transitive set of workspace member names that the given member depends on.
    ///
    /// Returns all members (direct + transitive) that must be built before this member.
    /// This is useful for propagating PCMs/objects from upstream members.
    pub fn transitive_member_deps(&self, member_name: &str) -> HashSet<String> {
        let member_names: HashSet<&str> = self.members.iter().map(|m| m.name.as_str()).collect();
        let mut result = HashSet::new();
        let mut queue = VecDeque::new();

        // Seed with direct dependencies
        if let Some(member) = self.find_member(member_name) {
            for (dep_name, dep) in &member.manifest.dependencies {
                if dep.is_path() && member_names.contains(dep_name.as_str()) {
                    queue.push_back(dep_name.clone());
                }
            }
        }

        // BFS for transitives
        while let Some(name) = queue.pop_front() {
            if !result.insert(name.clone()) {
                continue;
            }
            if let Some(dep_member) = self.find_member(&name) {
                for (trans_name, trans_dep) in &dep_member.manifest.dependencies {
                    if trans_dep.is_path() && member_names.contains(trans_name.as_str()) {
                        queue.push_back(trans_name.clone());
                    }
                }
            }
        }

        result
    }

    /// Get the workspace-level version, if set.
    pub fn workspace_version(&self) -> Option<&str> {
        self.root_manifest
            .workspace
            .as_ref()
            .and_then(|ws| ws.version.as_deref())
    }

    /// Add a new member to the workspace.
    pub fn add_member(&mut self, name: &str) -> Result<(), CmodError> {
        let member_dir = self.root.join(name);
        if member_dir.exists() {
            return Err(CmodError::InvalidManifest {
                reason: format!("directory '{}' already exists", name),
            });
        }

        // Create member directory structure
        std::fs::create_dir_all(member_dir.join("src"))?;

        // Create member manifest
        let member_manifest = cmod_core::manifest::default_manifest(name);
        member_manifest.save(&member_dir.join("cmod.toml"))?;

        // Create a stub module interface
        std::fs::write(
            member_dir.join("src/lib.cppm"),
            format!("export module local.{};\n", name),
        )?;

        // Update root manifest to include the new member
        if let Some(ws) = &mut self.root_manifest.workspace {
            ws.members.push(name.to_string());
        }
        self.root_manifest.save(&self.root.join("cmod.toml"))?;

        // Add to members list
        self.members.push(WorkspaceMember {
            name: name.to_string(),
            path: member_dir,
            manifest: cmod_core::manifest::default_manifest(name),
        });

        Ok(())
    }
}

/// Expand member patterns, supporting glob syntax.
///
/// Returns a list of (name, absolute_path) pairs for each matching directory.
fn expand_member_patterns(
    root: &Path,
    patterns: &[String],
) -> Result<Vec<(String, PathBuf)>, CmodError> {
    let mut results = Vec::new();

    for pattern in patterns {
        // Check if the pattern contains glob characters
        if pattern.contains('*') || pattern.contains('?') || pattern.contains('[') {
            let glob_pattern = root.join(pattern).display().to_string();
            match glob::glob(&glob_pattern) {
                Ok(paths) => {
                    for entry in paths.flatten() {
                        if entry.is_dir() {
                            let name = entry
                                .strip_prefix(root)
                                .unwrap_or(&entry)
                                .to_string_lossy()
                                .replace('\\', "/");
                            results.push((name, entry));
                        }
                    }
                }
                Err(e) => {
                    return Err(CmodError::InvalidManifest {
                        reason: format!("invalid glob pattern '{}': {}", pattern, e),
                    });
                }
            }
        } else {
            // Literal directory name
            let member_dir = root.join(pattern);
            if member_dir.is_dir() {
                results.push((pattern.clone(), member_dir));
            }
        }
    }

    // Sort by name for deterministic ordering
    results.sort_by(|a, b| a.0.cmp(&b.0));

    // Deduplicate
    results.dedup_by(|a, b| a.1 == b.1);

    Ok(results)
}

/// Expand exclude patterns and return the set of excluded paths.
fn expand_exclude_patterns(root: &Path, patterns: &[String]) -> HashSet<PathBuf> {
    let mut excluded = HashSet::new();

    for pattern in patterns {
        if pattern.contains('*') || pattern.contains('?') || pattern.contains('[') {
            let glob_pattern = root.join(pattern).display().to_string();
            if let Ok(paths) = glob::glob(&glob_pattern) {
                for entry in paths.flatten() {
                    excluded.insert(entry);
                }
            }
        } else {
            excluded.insert(root.join(pattern));
        }
    }

    excluded
}

/// Resolve workspace dependency references in a member manifest.
///
/// When a member has `dep = { workspace = true }`, replace it with the
/// actual dependency from the workspace root.
fn resolve_workspace_deps(
    member_manifest: &mut Manifest,
    workspace_deps: &BTreeMap<String, Dependency>,
) {
    let deps = std::mem::take(&mut member_manifest.dependencies);
    for (name, dep) in deps {
        if dep.is_workspace() {
            if let Some(ws_dep) = workspace_deps.get(&name) {
                member_manifest.dependencies.insert(name, ws_dep.clone());
            } else {
                // Keep the original if workspace dep not found
                member_manifest.dependencies.insert(name, dep);
            }
        } else {
            member_manifest.dependencies.insert(name, dep);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_workspace() -> TempDir {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        // Root manifest
        let root_toml = r#"
[package]
name = "test-workspace"
version = "0.1.0"

[workspace]
members = ["core", "app"]

[workspace.dependencies]
"github.com/fmtlib/fmt" = "^10.2"
"#;
        std::fs::write(root.join("cmod.toml"), root_toml).unwrap();

        // Core member
        std::fs::create_dir_all(root.join("core/src")).unwrap();
        let core_toml = r#"
[package]
name = "core"
version = "0.1.0"

[module]
name = "local.core"
root = "src/lib.cppm"
"#;
        std::fs::write(root.join("core/cmod.toml"), core_toml).unwrap();
        std::fs::write(root.join("core/src/lib.cppm"), "export module local.core;").unwrap();

        // App member
        std::fs::create_dir_all(root.join("app/src")).unwrap();
        let app_toml = r#"
[package]
name = "app"
version = "0.1.0"

[module]
name = "local.app"
root = "src/lib.cppm"

[dependencies]
"github.com/fmtlib/fmt" = { workspace = true }
"#;
        std::fs::write(root.join("app/cmod.toml"), app_toml).unwrap();
        std::fs::write(root.join("app/src/lib.cppm"), "export module local.app;").unwrap();

        tmp
    }

    #[test]
    fn test_load_workspace() {
        let tmp = setup_workspace();
        let ws = WorkspaceManager::load(tmp.path()).unwrap();
        assert_eq!(ws.members.len(), 2);
        let names = ws.member_names();
        assert!(names.contains(&"core"));
        assert!(names.contains(&"app"));
    }

    #[test]
    fn test_workspace_dependency_resolution() {
        let tmp = setup_workspace();
        let ws = WorkspaceManager::load(tmp.path()).unwrap();

        // The "app" member should have its workspace dep resolved
        let app = ws.find_member("app").unwrap();
        let fmt_dep = app.manifest.dependencies.get("github.com/fmtlib/fmt");
        assert!(fmt_dep.is_some());
        // Should now be the resolved version, not { workspace = true }
        assert!(!fmt_dep.unwrap().is_workspace());
    }

    #[test]
    fn test_all_dependencies() {
        let tmp = setup_workspace();
        let ws = WorkspaceManager::load(tmp.path()).unwrap();
        let all = ws.all_dependencies().unwrap();
        assert!(all.contains_key("github.com/fmtlib/fmt"));
    }

    #[test]
    fn test_not_a_workspace() {
        let tmp = TempDir::new().unwrap();
        let toml = r#"
[package]
name = "not-workspace"
version = "0.1.0"
"#;
        std::fs::write(tmp.path().join("cmod.toml"), toml).unwrap();

        let result = WorkspaceManager::load(tmp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_glob_member_patterns() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        // Root manifest with glob pattern
        let root_toml = r#"
[package]
name = "glob-workspace"
version = "0.1.0"

[workspace]
members = ["crates/*"]
"#;
        std::fs::write(root.join("cmod.toml"), root_toml).unwrap();

        // Create member dirs matching the glob
        for name in &["alpha", "beta", "gamma"] {
            let dir = root.join("crates").join(name);
            std::fs::create_dir_all(dir.join("src")).unwrap();
            let member_toml = format!("[package]\nname = \"{}\"\nversion = \"0.1.0\"\n", name);
            std::fs::write(dir.join("cmod.toml"), member_toml).unwrap();
        }

        let ws = WorkspaceManager::load(root).unwrap();
        assert_eq!(ws.members.len(), 3);
        let names = ws.member_names();
        assert!(names.contains(&"crates/alpha"));
        assert!(names.contains(&"crates/beta"));
        assert!(names.contains(&"crates/gamma"));
    }

    #[test]
    fn test_exclude_patterns() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        let root_toml = r#"
[package]
name = "exclude-workspace"
version = "0.1.0"

[workspace]
members = ["crates/*"]
exclude = ["crates/experimental"]
"#;
        std::fs::write(root.join("cmod.toml"), root_toml).unwrap();

        for name in &["stable", "experimental"] {
            let dir = root.join("crates").join(name);
            std::fs::create_dir_all(dir.join("src")).unwrap();
            let member_toml = format!("[package]\nname = \"{}\"\nversion = \"0.1.0\"\n", name);
            std::fs::write(dir.join("cmod.toml"), member_toml).unwrap();
        }

        let ws = WorkspaceManager::load(root).unwrap();
        assert_eq!(ws.members.len(), 1);
        assert_eq!(ws.members[0].name, "crates/stable");
    }

    #[test]
    fn test_build_order_no_deps() {
        let tmp = setup_workspace();
        let ws = WorkspaceManager::load(tmp.path()).unwrap();
        let order = ws.build_order().unwrap();
        assert_eq!(order.len(), 2);
    }

    #[test]
    fn test_build_order_with_deps() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        let root_toml = r#"
[package]
name = "ordered-workspace"
version = "0.1.0"

[workspace]
members = ["app", "core"]
"#;
        std::fs::write(root.join("cmod.toml"), root_toml).unwrap();

        // core has no deps
        let core_dir = root.join("core");
        std::fs::create_dir_all(core_dir.join("src")).unwrap();
        std::fs::write(
            core_dir.join("cmod.toml"),
            "[package]\nname = \"core\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        // app depends on core via path
        let app_dir = root.join("app");
        std::fs::create_dir_all(app_dir.join("src")).unwrap();
        let app_toml = r#"
[package]
name = "app"
version = "0.1.0"

[dependencies]
core = { path = "./core" }
"#;
        std::fs::write(app_dir.join("cmod.toml"), app_toml).unwrap();

        let ws = WorkspaceManager::load(root).unwrap();
        let order = ws.build_order().unwrap();
        assert_eq!(order.len(), 2);
        // core must come before app
        assert_eq!(order[0].name, "core");
        assert_eq!(order[1].name, "app");
    }

    #[test]
    fn test_workspace_version() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        let root_toml = r#"
[package]
name = "versioned-workspace"
version = "0.1.0"

[workspace]
version = "2.0.0"
members = []
"#;
        std::fs::write(root.join("cmod.toml"), root_toml).unwrap();

        let ws = WorkspaceManager::load(root).unwrap();
        assert_eq!(ws.workspace_version(), Some("2.0.0"));
    }

    #[test]
    fn test_transitive_member_deps() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();

        // A → B → C (app depends on lib, lib depends on core)
        let root_toml = r#"
[package]
name = "transitive-workspace"
version = "0.1.0"

[workspace]
members = ["core", "lib", "app"]
"#;
        std::fs::write(root.join("cmod.toml"), root_toml).unwrap();

        // core: no deps
        let core_dir = root.join("core");
        std::fs::create_dir_all(core_dir.join("src")).unwrap();
        std::fs::write(
            core_dir.join("cmod.toml"),
            "[package]\nname = \"core\"\nversion = \"0.1.0\"\n",
        )
        .unwrap();

        // lib depends on core
        let lib_dir = root.join("lib");
        std::fs::create_dir_all(lib_dir.join("src")).unwrap();
        let lib_toml = r#"
[package]
name = "lib"
version = "0.1.0"

[dependencies]
core = { path = "./core" }
"#;
        std::fs::write(lib_dir.join("cmod.toml"), lib_toml).unwrap();

        // app depends on lib
        let app_dir = root.join("app");
        std::fs::create_dir_all(app_dir.join("src")).unwrap();
        let app_toml = r#"
[package]
name = "app"
version = "0.1.0"

[dependencies]
lib = { path = "./lib" }
"#;
        std::fs::write(app_dir.join("cmod.toml"), app_toml).unwrap();

        let ws = WorkspaceManager::load(root).unwrap();

        // core has no transitive deps
        let core_deps = ws.transitive_member_deps("core");
        assert!(core_deps.is_empty());

        // lib depends on core
        let lib_deps = ws.transitive_member_deps("lib");
        assert_eq!(lib_deps.len(), 1);
        assert!(lib_deps.contains("core"));

        // app transitively depends on both lib and core
        let app_deps = ws.transitive_member_deps("app");
        assert_eq!(app_deps.len(), 2);
        assert!(app_deps.contains("lib"));
        assert!(app_deps.contains("core"));

        // non-existent member returns empty
        let unknown_deps = ws.transitive_member_deps("nonexistent");
        assert!(unknown_deps.is_empty());
    }

    #[test]
    fn test_expand_member_patterns_literal() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::create_dir_all(root.join("mylib")).unwrap();

        let result = expand_member_patterns(root, &["mylib".to_string()]).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].0, "mylib");
    }

    #[test]
    fn test_expand_member_patterns_nonexistent() {
        let tmp = TempDir::new().unwrap();
        let result = expand_member_patterns(tmp.path(), &["nonexistent".to_string()]).unwrap();
        assert!(result.is_empty());
    }
}
