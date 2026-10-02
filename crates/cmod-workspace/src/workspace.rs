use std::collections::{BTreeMap, HashSet, VecDeque};
use std::path::{Path, PathBuf};

use cmod_core::error::CmodError;
use cmod_core::manifest::{edit_toml_file, sync_string_array, Dependency, Manifest};

/// A resolved workspace member.
#[derive(Debug, Clone)]
pub struct WorkspaceMember {
    /// Member name: its `[package] name`, or, when that is empty, the name
    /// of its directory. A single path component: it names the member's
    /// build directory.
    pub name: String,
    /// Absolute path to the member directory.
    pub path: PathBuf,
    /// The member directory relative to the workspace root, with `/`
    /// separators (`crates/core`).
    pub rel_path: String,
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
    /// For each member, the members it depends on, by index: computed when
    /// the members change through `load`, `add_member` or `remove_member`.
    member_deps: Vec<Vec<usize>>,
}

/// A member `WorkspaceManager::add_member` added.
#[derive(Debug, Clone)]
pub struct AddedMember {
    /// Its name.
    pub name: String,
    /// Its directory relative to the workspace root.
    pub rel_path: String,
    /// Whether its directory was created, with a `cmod.toml` and
    /// `src/lib.cppm`.
    pub scaffolded: bool,
}

/// A member `WorkspaceManager::remove_member` removed.
#[derive(Debug, Clone)]
pub struct RemovedMember {
    /// Its name.
    pub name: String,
    /// Its directory relative to the workspace root.
    pub rel_path: String,
    /// Whether a `[workspace] members` glob still matched it, so it was
    /// added to `exclude`.
    pub excluded: bool,
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

        let mut members: Vec<WorkspaceMember> = Vec::new();

        for (rel_path, member_dir) in
            matched_member_dirs(root, &workspace.members, &workspace.exclude)?
        {
            let member_manifest_path = member_dir.join("cmod.toml");
            if member_manifest_path.exists() {
                let mut member_manifest = Manifest::load(&member_manifest_path)?;

                // Resolve workspace dependency references
                resolve_workspace_deps(&mut member_manifest, &workspace.dependencies);

                let name = member_name(root, &member_dir, &member_manifest)?;
                if let Some(other) = members.iter().find(|m| m.name == name) {
                    return Err(duplicate_member(&name, &other.rel_path, &rel_path));
                }

                members.push(WorkspaceMember {
                    name,
                    path: member_dir,
                    rel_path,
                    manifest: member_manifest,
                });
            }
        }

        let mut ws = WorkspaceManager {
            root: root.to_path_buf(),
            root_manifest,
            members,
            member_deps: Vec::new(),
        };
        ws.member_deps = ws.compute_member_deps();
        Ok(ws)
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

    /// Apply workspace dependency patches to the unified dependency map.
    ///
    /// Patches override dependency sources (e.g., replacing a git dep with a
    /// local path for development). Defined in `[workspace.patch]` in `cmod.toml`.
    pub fn apply_patches(&self, deps: &mut BTreeMap<String, Dependency>) {
        if let Some(ws) = &self.root_manifest.workspace {
            for (name, patch_dep) in &ws.patch {
                if deps.contains_key(name) {
                    deps.insert(name.clone(), patch_dep.clone());
                }
            }
        }
    }

    /// Collect all dependencies with patches applied.
    pub fn all_dependencies_patched(&self) -> Result<BTreeMap<String, Dependency>, CmodError> {
        let mut deps = self.all_dependencies()?;
        self.apply_patches(&mut deps);
        Ok(deps)
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

        // Duplicate member names are refused by `load` and `add_member`.

        // Validate dependencies don't conflict
        self.all_dependencies()?;

        Ok(())
    }

    /// Compute the build order for workspace members.
    ///
    /// Members that depend on other members (via path deps) must build
    /// after their dependencies. Returns members in topological order.
    pub fn build_order(&self) -> Result<Vec<&WorkspaceMember>, CmodError> {
        let n = self.members.len();
        let mut in_degree = vec![0usize; n];
        let mut dependents = vec![Vec::new(); n];

        for (idx, deps) in self.member_deps().iter().enumerate() {
            for &dep_idx in deps {
                in_degree[idx] += 1;
                dependents[dep_idx].push(idx);
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
            // Each member left over depends on another one left over (or its
            // in-degree would have reached zero): follow those dependencies
            // until one comes back around.
            let left: Vec<usize> = (0..n).filter(|&i| in_degree[i] > 0).collect();
            let depends_on =
                |idx: usize| left.iter().copied().find(|&d| dependents[d].contains(&idx));
            let mut path = left.first().copied().into_iter().collect::<Vec<_>>();
            let mut cycle = Vec::new();
            while let Some(next) = path.last().and_then(|&at| depends_on(at)) {
                if let Some(at) = path.iter().position(|&m| m == next) {
                    cycle = path.split_off(at);
                    cycle.push(next);
                    break;
                }
                path.push(next);
            }
            let names: Vec<&str> = cycle
                .iter()
                .map(|&i| self.members[i].name.as_str())
                .collect();
            return Err(CmodError::CircularDependency {
                cycle: format!(
                    "{} (workspace members' path dependencies)",
                    names.join(" -> ")
                ),
            });
        }

        Ok(order.iter().map(|&i| &self.members[i]).collect())
    }

    /// Get the transitive set of workspace member names that the given member depends on.
    ///
    /// Returns all members (direct + transitive) that must be built before this member.
    /// This is useful for propagating PCMs/objects from upstream members.
    pub fn transitive_member_deps(&self, member_name: &str) -> HashSet<String> {
        let Some(start) = self.members.iter().position(|m| m.name == member_name) else {
            return HashSet::new();
        };
        let deps = self.member_deps();
        let mut seen = HashSet::new();
        let mut queue: VecDeque<usize> = deps[start].iter().copied().collect();
        while let Some(idx) = queue.pop_front() {
            if seen.insert(idx) {
                queue.extend(deps[idx].iter().copied());
            }
        }
        seen.into_iter()
            .map(|idx| self.members[idx].name.clone())
            .collect()
    }

    /// For each member, the members it depends on, by index; recomputed
    /// when `members` was changed directly.
    fn member_deps(&self) -> std::borrow::Cow<'_, [Vec<usize>]> {
        if self.member_deps.len() == self.members.len() {
            std::borrow::Cow::Borrowed(&self.member_deps)
        } else {
            std::borrow::Cow::Owned(self.compute_member_deps())
        }
    }

    /// For each member, the members it depends on through path
    /// dependencies, by index, each once.
    ///
    /// A path dependency names a member when it points at that member's
    /// directory, whatever its key; one whose path does not exist (written
    /// relative to another directory) counts when its key is a member's
    /// name.
    fn compute_member_deps(&self) -> Vec<Vec<usize>> {
        let canonical: Vec<Option<PathBuf>> = self
            .members
            .iter()
            .map(|m| std::fs::canonicalize(&m.path).ok())
            .collect();
        self.members
            .iter()
            .enumerate()
            .map(|(idx, member)| {
                let mut deps = Vec::new();
                for (key, dep) in &member.manifest.dependencies {
                    let Some(path) = dep.path() else { continue };
                    let found = match std::fs::canonicalize(member.path.join(path)) {
                        Ok(target) => canonical.iter().position(|c| c.as_ref() == Some(&target)),
                        Err(_) => self.members.iter().position(|m| m.name == *key),
                    };
                    if let Some(dep_idx) = found {
                        if dep_idx != idx && !deps.contains(&dep_idx) {
                            deps.push(dep_idx);
                        }
                    }
                }
                deps
            })
            .collect()
    }

    /// Whether `dir` is, or is inside, a directory `[workspace] exclude`
    /// names.
    pub fn is_excluded(&self, dir: &Path) -> bool {
        let Some(ws) = &self.root_manifest.workspace else {
            return false;
        };
        excludes(
            &self.root,
            &expand_exclude_patterns(&self.root, &ws.exclude),
            dir,
        )
    }

    /// Write `[workspace] members` and `exclude` to the root `cmod.toml`,
    /// leaving the rest of the file, its comments and the entries kept as
    /// they were.
    fn save_member_lists(&self) -> Result<(), CmodError> {
        let Some(ws) = &self.root_manifest.workspace else {
            return Ok(());
        };
        edit_toml_file(&self.root.join("cmod.toml"), |doc| {
            let table = doc
                .entry("workspace")
                .or_insert_with(toml_edit::table)
                .as_table_like_mut()
                .ok_or_else(|| CmodError::InvalidManifest {
                    reason: "[workspace] is not a table".to_string(),
                })?;
            sync_string_array(table, "members", &ws.members);
            sync_string_array(table, "exclude", &ws.exclude);
            Ok(())
        })
    }

    /// Get the workspace-level version, if set.
    pub fn workspace_version(&self) -> Option<&str> {
        self.root_manifest
            .workspace
            .as_ref()
            .and_then(|ws| ws.version.as_deref())
    }

    /// Add a member to the workspace.
    ///
    /// With `must_scaffold`, the caller explicitly asserts a new member is
    /// being created: the target directory must not exist at all.
    ///
    /// Without it, behavior is inferred from what exists at `<root>/<name>`:
    ///
    /// 1. **Dir exists with a `cmod.toml`** → register it (no scaffolding).
    /// 2. **Dir exists without a `cmod.toml`** → reject (ambiguous; caller
    ///    can delete the dir and retry, or add the manifest manually).
    /// 3. **Dir does not exist** → scaffold `src/lib.cppm` + `cmod.toml`.
    pub fn add_member(
        &mut self,
        name: &str,
        must_scaffold: bool,
    ) -> Result<AddedMember, CmodError> {
        // Early-reject names that would be unsafe to use as path components.
        let escapes = Path::new(name).components().any(|c| {
            matches!(
                c,
                std::path::Component::ParentDir
                    | std::path::Component::RootDir
                    | std::path::Component::Prefix(_)
            )
        });
        if name.is_empty() || escapes || name.starts_with('/') || name.starts_with('\\') {
            return Err(CmodError::InvalidManifest {
                reason: format!("invalid member name '{}'", name),
            });
        }

        let member_dir = self.root.join(name);
        let manifest_path = member_dir.join("cmod.toml");
        let rel_path = relative_path(&self.root, &member_dir);

        if let Some(member) = self.members.iter().find(|m| m.rel_path == rel_path) {
            return Err(CmodError::InvalidManifest {
                reason: format!(
                    "'{}' is already a workspace member (named '{}')",
                    name, member.name
                ),
            });
        }

        // Adding a member undoes an exclude entry naming just its directory;
        // one covering more is the user's to change.
        let (patterns, mut exclude) = match &self.root_manifest.workspace {
            Some(ws) => (ws.members.clone(), ws.exclude.clone()),
            None => (Vec::new(), Vec::new()),
        };
        exclude.retain(|ex| !entry_names(&self.root, ex, &rel_path));
        if excludes(
            &self.root,
            &expand_exclude_patterns(&self.root, &exclude),
            &member_dir,
        ) {
            return Err(CmodError::InvalidManifest {
                reason: format!(
                    "'{}' is excluded by [workspace] exclude; remove the entry covering it first",
                    name
                ),
            });
        }

        if must_scaffold && member_dir.exists() {
            return Err(CmodError::InvalidManifest {
                reason: format!(
                    "cannot scaffold '{}': directory already exists; \
                     omit --scaffold to register an existing member",
                    name
                ),
            });
        }

        let scaffold = !member_dir.exists();
        let member_manifest = if scaffold {
            // A member at `libs/util` is the package `util`, whose module
            // is `local.util`.
            let package = rel_path.rsplit('/').next().unwrap_or(&rel_path);
            if !cmod_core::types::is_usable_cpp_identifier(&package.replace('-', "_")) {
                return Err(CmodError::InvalidManifest {
                    reason: format!(
                        "cannot scaffold '{}': '{}' cannot name a C++ module and namespace; \
                         use letters, digits, '_' or '-', not starting with a digit or naming a keyword",
                        name, package
                    ),
                });
            }
            cmod_core::manifest::default_manifest(package)
        } else {
            if !member_dir.is_dir() {
                return Err(CmodError::InvalidManifest {
                    reason: format!("'{}' exists but is not a directory", name),
                });
            }
            if !manifest_path.exists() {
                return Err(CmodError::InvalidManifest {
                    reason: format!(
                        "'{}' exists but has no cmod.toml; add a manifest first or remove the directory",
                        name
                    ),
                });
            }
            Manifest::load(&manifest_path)?
        };

        let member_name = member_name(&self.root, &member_dir, &member_manifest)?;
        if let Some(member) = self.members.iter().find(|m| m.name == member_name) {
            return Err(duplicate_member(&member_name, &member.rel_path, &rel_path));
        }

        if scaffold {
            std::fs::create_dir_all(member_dir.join("src"))?;
            member_manifest.save(&manifest_path)?;
            let module = member_manifest
                .module
                .as_ref()
                .map(|m| m.name.as_str())
                .unwrap_or(&member_name);
            std::fs::write(
                member_dir.join("src/lib.cppm"),
                format!("export module {};\n", module),
            )?;
        }

        // A pattern may match it already (once its exclude entry is gone).
        let matched = expand_member_patterns(&self.root, &patterns)?
            .iter()
            .any(|(rel, _)| *rel == rel_path);
        if let Some(ws) = &mut self.root_manifest.workspace {
            ws.exclude = exclude;
            if !matched {
                // An entry is a glob when it has glob characters: escape them.
                ws.members.push(if is_glob(&rel_path) {
                    glob::Pattern::escape(&rel_path)
                } else {
                    rel_path.clone()
                });
            }
        }
        self.save_member_lists()?;

        self.members.push(WorkspaceMember {
            name: member_name.clone(),
            path: member_dir,
            rel_path: rel_path.clone(),
            manifest: member_manifest,
        });
        self.member_deps = self.compute_member_deps();

        Ok(AddedMember {
            name: member_name,
            rel_path,
            scaffolded: scaffold,
        })
    }

    /// Remove the member named `name_or_dir`, or else whose directory
    /// relative to the root it is, from the workspace. The directory stays.
    ///
    /// The `members` entries naming its directory are dropped; when a glob
    /// still matches it, it is added to `exclude`, unless that would
    /// exclude other members inside it.
    pub fn remove_member(&mut self, name_or_dir: &str) -> Result<RemovedMember, CmodError> {
        let dir = relative_path(&self.root, &self.root.join(name_or_dir));
        let Some(idx) = self
            .members
            .iter()
            .position(|m| m.name == name_or_dir)
            .or_else(|| self.members.iter().position(|m| m.rel_path == dir))
        else {
            return Err(CmodError::InvalidManifest {
                reason: format!("member '{}' not found in workspace", name_or_dir),
            });
        };
        let rel_path = self.members[idx].rel_path.clone();

        let mut excluded = false;
        if let Some(ws) = &self.root_manifest.workspace {
            let mut ws = ws.clone();
            ws.members
                .retain(|entry| !entry_names(&self.root, entry, &rel_path));
            let still_matched = matched_member_dirs(&self.root, &ws.members, &ws.exclude)?
                .iter()
                .any(|(rel, _)| *rel == rel_path);
            if still_matched {
                ws.exclude.push(if is_glob(&rel_path) {
                    glob::Pattern::escape(&rel_path)
                } else {
                    rel_path.clone()
                });
                excluded = true;
            }
            // Members inside it would go too: theirs to exclude, not ours.
            let remaining = matched_member_dirs(&self.root, &ws.members, &ws.exclude)?;
            let lost: Vec<&str> = self
                .members
                .iter()
                .filter(|m| m.rel_path != rel_path)
                .filter(|m| !remaining.iter().any(|(rel, _)| *rel == m.rel_path))
                .map(|m| m.name.as_str())
                .collect();
            if !lost.is_empty() {
                return Err(CmodError::InvalidManifest {
                    reason: format!(
                        "removing '{}' would also remove {}, inside it; edit [workspace] in cmod.toml instead",
                        name_or_dir,
                        lost.join(", ")
                    ),
                });
            }
            self.root_manifest.workspace = Some(ws);
        }
        self.save_member_lists()?;
        let member = self.members.remove(idx);
        self.member_deps = self.compute_member_deps();

        Ok(RemovedMember {
            name: member.name,
            rel_path: member.rel_path,
            excluded,
        })
    }
}

/// Expand member patterns, supporting glob syntax.
///
/// Returns a (relative path, absolute path) pair for each matching
/// directory, sorted by relative path, each directory once.
fn expand_member_patterns(
    root: &Path,
    patterns: &[String],
) -> Result<Vec<(String, PathBuf)>, CmodError> {
    let mut results = Vec::new();

    for pattern in patterns {
        // Check if the pattern contains glob characters
        if is_glob(pattern) {
            let glob_pattern = root.join(pattern).display().to_string();
            match glob::glob(&glob_pattern) {
                Ok(paths) => {
                    for entry in paths.flatten() {
                        if entry.is_dir() {
                            results.push((relative_path(root, &entry), entry));
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
                results.push((relative_path(root, &member_dir), member_dir));
            }
        }
    }

    // Sort for deterministic ordering, then drop directories two patterns
    // both matched
    results.sort_by(|a, b| a.0.cmp(&b.0));
    results.dedup_by(|a, b| a.0 == b.0);

    Ok(results)
}

/// The directories `patterns` match, as (relative path, absolute path),
/// leaving out those `exclude` covers.
fn matched_member_dirs(
    root: &Path,
    patterns: &[String],
    exclude: &[String],
) -> Result<Vec<(String, PathBuf)>, CmodError> {
    let excluded = expand_exclude_patterns(root, exclude);
    let mut dirs = expand_member_patterns(root, patterns)?;
    dirs.retain(|(_, dir)| !excludes(root, &excluded, dir));
    Ok(dirs)
}

/// Whether `dir` is, or is inside, one of the `excluded` directories. An
/// entry naming the root itself (`.`) excludes only the root.
fn excludes(root: &Path, excluded: &HashSet<PathBuf>, dir: &Path) -> bool {
    excluded.iter().any(|ex| {
        if ex == root {
            dir == root
        } else {
            dir.starts_with(ex)
        }
    })
}

/// `path` relative to `root`, with `/` separators and no `.` components:
/// how a member's directory is spelled (`libs/a` for `./libs/a/`).
pub fn relative_path(root: &Path, path: &Path) -> String {
    let rel = path.strip_prefix(root).unwrap_or(path);
    rel.components()
        .filter(|c| !matches!(c, std::path::Component::CurDir))
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/")
}

/// Whether a `members` or `exclude` entry is a glob pattern.
fn is_glob(entry: &str) -> bool {
    entry.contains('*') || entry.contains('?') || entry.contains('[')
}

/// Whether the `members` or `exclude` entry `entry` names just the
/// directory `rel_path` (as spelled or, being a glob, escaped).
fn entry_names(root: &Path, entry: &str, rel_path: &str) -> bool {
    let spelled = relative_path(root, &root.join(entry));
    spelled == rel_path || (is_glob(rel_path) && spelled == glob::Pattern::escape(rel_path))
}

/// A member's name: its `[package] name`, or, when that is empty, the
/// name of its directory (of the root, for the root).
fn member_name(root: &Path, dir: &Path, manifest: &Manifest) -> Result<String, CmodError> {
    let name = &manifest.package.name;
    if name.is_empty() {
        let dir = std::fs::canonicalize(dir).unwrap_or_else(|_| dir.to_path_buf());
        return Ok(dir
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_else(|| "root".to_string()));
    }
    let usable =
        cmod_core::types::is_acceptable_package_name(name) && !name.contains(['/', '\\', ':']);
    if !usable {
        return Err(CmodError::InvalidManifest {
            reason: format!(
                "workspace member {} is named '{}', which cannot name its build directory; \
                 use a name without '/', '\\', ':' or a leading '.'",
                relative_path(root, dir),
                name
            ),
        });
    }
    Ok(name.clone())
}

/// The error for two workspace members sharing a package name.
fn duplicate_member(name: &str, first: &str, second: &str) -> CmodError {
    CmodError::InvalidManifest {
        reason: format!(
            "two workspace members are named '{}': {} and {}; \
             rename one in its cmod.toml",
            name, first, second
        ),
    }
}

/// Expand exclude patterns and return the set of excluded paths.
fn expand_exclude_patterns(root: &Path, patterns: &[String]) -> HashSet<PathBuf> {
    let mut excluded = HashSet::new();

    for pattern in patterns {
        if is_glob(pattern) {
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
        assert_eq!(names, ["alpha", "beta", "gamma"]);
        assert_eq!(ws.members[0].rel_path, "crates/alpha");
    }

    /// Write a member at `root/rel` named `name`, with `deps` lines under
    /// `[dependencies]`.
    fn write_member(root: &Path, rel: &str, name: &str, deps: &str) {
        let dir = root.join(rel);
        std::fs::create_dir_all(dir.join("src")).unwrap();
        std::fs::write(
            dir.join("cmod.toml"),
            format!("[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n\n[dependencies]\n{deps}"),
        )
        .unwrap();
    }

    fn write_root(root: &Path, workspace: &str) {
        std::fs::write(
            root.join("cmod.toml"),
            format!("[package]\nname = \"ws\"\nversion = \"0.1.0\"\n\n[workspace]\n{workspace}"),
        )
        .unwrap();
    }

    #[test]
    fn test_glob_members_link_up_through_path_dependencies() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_root(root, "members = [\"crates/*\"]\n");
        write_member(root, "crates/b", "b", "a = { path = \"../a\" }\n");
        write_member(root, "crates/a", "a", "");
        // The key need not be the package name: the path decides.
        write_member(root, "crates/c", "c", "base = { path = \"../b\" }\n");

        let ws = WorkspaceManager::load(root).unwrap();
        let order: Vec<&str> = ws
            .build_order()
            .unwrap()
            .iter()
            .map(|m| m.name.as_str())
            .collect();
        assert_eq!(order, ["a", "b", "c"]);
        let mut deps: Vec<String> = ws.transitive_member_deps("c").into_iter().collect();
        deps.sort();
        assert_eq!(deps, ["a", "b"]);
    }

    #[test]
    fn test_members_sharing_a_package_name_are_an_error() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_root(root, "members = [\"libs/*\", \"tools/core\"]\n");
        write_member(root, "libs/core", "core", "");
        write_member(root, "tools/core", "core", "");

        let err = WorkspaceManager::load(root).err().unwrap().to_string();
        assert!(
            err.contains("two workspace members are named 'core': libs/core and tools/core"),
            "{}",
            err
        );
    }

    #[test]
    fn test_a_member_matched_twice_is_listed_once() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_root(
            root,
            "members = [\"crates/*\", \"./crates/a\", \"crates/a/\"]\n",
        );
        write_member(root, "crates/a", "a", "");

        let ws = WorkspaceManager::load(root).unwrap();
        assert_eq!(ws.member_names(), ["a"]);
    }

    #[test]
    fn test_exclude_covers_the_directories_inside() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_root(
            root,
            "members = [\"libs/*\", \"libs/vendor/*\"]\nexclude = [\"libs/vendor\"]\n",
        );
        write_member(root, "libs/core", "core", "");
        write_member(root, "libs/vendor/zlib", "zlib", "");

        let ws = WorkspaceManager::load(root).unwrap();
        assert_eq!(ws.member_names(), ["core"]);
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
        assert_eq!(ws.members[0].name, "stable");
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
    fn test_build_order_names_the_cycle() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::write(
            root.join("cmod.toml"),
            "[package]\nname = \"cyclic\"\nversion = \"0.1.0\"\n\n\
             [workspace]\nmembers = [\"a\", \"b\", \"app\"]\n",
        )
        .unwrap();
        for (name, deps) in [("a", &["b"][..]), ("b", &["a"]), ("app", &["a"])] {
            let dir = root.join(name);
            std::fs::create_dir_all(dir.join("src")).unwrap();
            let deps: String = deps
                .iter()
                .map(|d| format!("{d} = {{ path = \"../{d}\" }}\n"))
                .collect();
            std::fs::write(
                dir.join("cmod.toml"),
                format!(
                    "[package]\nname = \"{name}\"\nversion = \"0.1.0\"\n\n[dependencies]\n{deps}"
                ),
            )
            .unwrap();
        }

        let ws = WorkspaceManager::load(root).unwrap();
        match ws.build_order() {
            Err(CmodError::CircularDependency { cycle }) => {
                assert!(
                    cycle.starts_with("a -> b -> a") || cycle.starts_with("b -> a -> b"),
                    "{}",
                    cycle
                );
            }
            other => panic!("expected a cycle, got {:?}", other.map(|o| o.len())),
        }
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

    // --- add_member scaffold intent (#41) ---

    #[test]
    fn test_add_member_scaffold_creates_new() {
        let tmp = setup_workspace();
        let mut ws = WorkspaceManager::load(tmp.path()).unwrap();

        ws.add_member("newlib", true).unwrap();

        assert!(tmp.path().join("newlib/cmod.toml").exists());
        assert!(tmp.path().join("newlib/src/lib.cppm").exists());
        assert!(ws.members.iter().any(|m| m.name == "newlib"));
    }

    #[test]
    fn test_add_member_scaffold_rejects_existing_dir() {
        let tmp = setup_workspace();
        let mut ws = WorkspaceManager::load(tmp.path()).unwrap();

        // Existing directory (even with a manifest) must be rejected when
        // the caller explicitly asked to scaffold a new member.
        let existing = tmp.path().join("preexisting");
        std::fs::create_dir_all(existing.join("src")).unwrap();
        cmod_core::manifest::default_manifest("preexisting")
            .save(&existing.join("cmod.toml"))
            .unwrap();

        let err = ws.add_member("preexisting", true).unwrap_err();
        assert!(
            err.to_string().contains("already exists"),
            "error should explain the directory already exists, got: {}",
            err
        );
        // Nothing was registered
        assert!(!ws.members.iter().any(|m| m.name == "preexisting"));
    }

    #[test]
    fn test_add_member_without_scaffold_keeps_inference() {
        let tmp = setup_workspace();
        let mut ws = WorkspaceManager::load(tmp.path()).unwrap();

        // Existing member dir with manifest → registered, not re-scaffolded
        let existing = tmp.path().join("existing");
        std::fs::create_dir_all(existing.join("src")).unwrap();
        cmod_core::manifest::default_manifest("existing")
            .save(&existing.join("cmod.toml"))
            .unwrap();
        ws.add_member("existing", false).unwrap();
        assert!(ws.members.iter().any(|m| m.name == "existing"));

        // Missing dir → scaffolded (unchanged legacy inference)
        ws.add_member("fresh", false).unwrap();
        assert!(tmp.path().join("fresh/cmod.toml").exists());
    }

    #[test]
    fn test_add_member_names_a_nested_member_by_its_directory() {
        let tmp = setup_workspace();
        let mut ws = WorkspaceManager::load(tmp.path()).unwrap();

        ws.add_member("libs/my-util", true).unwrap();
        let member = ws.find_member("my-util").unwrap();
        assert_eq!(member.rel_path, "libs/my-util");
        let lib = std::fs::read_to_string(tmp.path().join("libs/my-util/src/lib.cppm")).unwrap();
        assert_eq!(lib, "export module local.my_util;\n");

        let reloaded = WorkspaceManager::load(tmp.path()).unwrap();
        assert!(reloaded.find_member("my-util").is_some());
    }

    #[test]
    fn test_add_member_undoes_only_an_exclude_naming_it() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_root(
            root,
            "members = [\"crates/*\"]\nexclude = [\"crates/a/\", \"vendor\"]\n",
        );
        write_member(root, "crates/a", "a", "");
        write_member(root, "vendor/zlib", "zlib", "");
        let mut ws = WorkspaceManager::load(root).unwrap();
        assert!(ws.members.is_empty());

        let err = ws.add_member("vendor/zlib", false).unwrap_err().to_string();
        assert!(err.contains("excluded by [workspace] exclude"), "{}", err);

        let mut ws = WorkspaceManager::load(root).unwrap();
        ws.add_member("crates/a", false).unwrap();
        let reloaded = WorkspaceManager::load(root).unwrap();
        assert_eq!(reloaded.member_names(), ["a"]);
        let exclude = &reloaded.root_manifest.workspace.as_ref().unwrap().exclude;
        assert_eq!(exclude, &["vendor"]);
    }

    #[test]
    fn test_remove_member_keeps_a_glob_member_out() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_root(root, "members = [\"crates/*\", \"./crates/b/\"]\n");
        write_member(root, "crates/a", "a", "");
        write_member(root, "crates/b", "b", "");

        let mut ws = WorkspaceManager::load(root).unwrap();
        let removed = ws.remove_member("./crates/b").unwrap();
        assert_eq!((removed.name.as_str(), removed.excluded), ("b", true));
        let reloaded = WorkspaceManager::load(root).unwrap();
        assert_eq!(reloaded.member_names(), ["a"]);
        let ws_config = reloaded.root_manifest.workspace.as_ref().unwrap();
        assert_eq!(ws_config.members, ["crates/*"]);
        assert_eq!(ws_config.exclude, ["crates/b"]);

        // Adding it back drops the exclude entry and adds no members entry,
        // so removing it again excludes it again.
        let mut ws = reloaded;
        ws.add_member("crates/b", false).unwrap();
        let ws_config = ws.root_manifest.workspace.as_ref().unwrap();
        assert_eq!(ws_config.members, ["crates/*"]);
        assert!(ws_config.exclude.is_empty());
        assert!(ws.remove_member("b").unwrap().excluded);
        assert_eq!(WorkspaceManager::load(root).unwrap().member_names(), ["a"]);
    }

    #[test]
    fn test_remove_member_drops_a_literal_entry() {
        let tmp = setup_workspace();
        let mut ws = WorkspaceManager::load(tmp.path()).unwrap();
        let removed = ws.remove_member("core").unwrap();
        assert!(!removed.excluded);
        let reloaded = WorkspaceManager::load(tmp.path()).unwrap();
        assert_eq!(reloaded.member_names(), ["app"]);
        assert!(ws.remove_member("ghost").is_err());
    }

    #[test]
    fn test_a_failed_add_changes_nothing() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_root(root, "members = [\"core\"]\nexclude = [\"crates/a\"]\n");
        write_member(root, "core", "a", "");
        write_member(root, "crates/a", "a", "");
        let mut ws = WorkspaceManager::load(root).unwrap();

        // Its exclude entry would go, but its name is taken.
        let err = ws.add_member("crates/a", false).unwrap_err().to_string();
        assert!(
            err.contains("two workspace members are named 'a'"),
            "{}",
            err
        );
        ws.add_member("fresh", false).unwrap();
        let ws_config = ws.root_manifest.workspace.as_ref().unwrap();
        assert_eq!(ws_config.exclude, ["crates/a"]);
        assert_eq!(ws_config.members, ["core", "fresh"]);
    }

    #[test]
    fn test_add_member_refuses_to_scaffold_an_unusable_module_name() {
        let tmp = setup_workspace();
        let mut ws = WorkspaceManager::load(tmp.path()).unwrap();
        for name in ["libs/2d", "libs/export", "std"] {
            let err = ws.add_member(name, true).unwrap_err().to_string();
            assert!(err.contains("cannot name a C++ module"), "{}", err);
            assert!(!tmp.path().join(name).exists());
        }
    }

    #[test]
    fn test_an_external_path_dependency_is_not_a_member_of_the_same_name() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_root(root, "members = [\"core\", \"app\"]\n");
        write_member(root, "core", "core", "app = { path = \"../app\" }\n");
        write_member(
            root,
            "app",
            "app",
            "core = { path = \"../third_party/core\" }\n",
        );
        write_member(root, "third_party/core", "core", "");

        let ws = WorkspaceManager::load(root).unwrap();
        assert!(ws.transitive_member_deps("app").is_empty());
        let order: Vec<&str> = ws
            .build_order()
            .unwrap()
            .iter()
            .map(|m| m.name.as_str())
            .collect();
        assert_eq!(order, ["app", "core"]);
    }

    #[test]
    fn test_excluding_the_root_excludes_only_the_root() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_root(root, "members = [\".\", \"libs/*\"]\nexclude = [\".\"]\n");
        write_member(root, "libs/a", "a", "");

        let ws = WorkspaceManager::load(root).unwrap();
        assert_eq!(ws.member_names(), ["a"]);
    }

    #[test]
    fn test_remove_member_prefers_a_name_to_a_directory() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_root(root, "members = [\"core\", \"libs/core\"]\n");
        write_member(root, "core", "base", "");
        write_member(root, "libs/core", "core", "");

        let mut ws = WorkspaceManager::load(root).unwrap();
        assert_eq!(ws.remove_member("core").unwrap().rel_path, "libs/core");
        assert_eq!(
            WorkspaceManager::load(root).unwrap().member_names(),
            ["base"]
        );
    }

    #[test]
    fn test_remove_member_will_not_exclude_the_members_inside_it() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_root(root, "members = [\"libs/**\"]\n");
        write_member(root, "libs/a", "a", "");
        write_member(root, "libs/a/sub", "sub", "");
        let before = std::fs::read_to_string(root.join("cmod.toml")).unwrap();

        let mut ws = WorkspaceManager::load(root).unwrap();
        let err = ws.remove_member("a").unwrap_err().to_string();
        assert!(err.contains("would also remove sub"), "{}", err);
        assert_eq!(ws.member_names(), ["a", "sub"]);
        let after = std::fs::read_to_string(root.join("cmod.toml")).unwrap();
        assert_eq!(before, after);
    }

    #[test]
    fn test_a_directory_with_glob_characters_is_added_literally() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_root(root, "members = []\n");
        write_member(root, "libs/[gl]", "gl", "");
        write_member(root, "libs/g", "g", "");

        let mut ws = WorkspaceManager::load(root).unwrap();
        ws.add_member("libs/[gl]", false).unwrap();
        let mut reloaded = WorkspaceManager::load(root).unwrap();
        assert_eq!(reloaded.member_names(), ["gl"]);
        assert!(!reloaded.remove_member("gl").unwrap().excluded);
        let ws_config = reloaded.root_manifest.workspace.as_ref().unwrap();
        assert!(ws_config.members.is_empty(), "{:?}", ws_config.members);
    }

    #[test]
    fn test_a_member_name_must_name_a_directory() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_root(root, "members = [\"a\"]\n");
        write_member(root, "a", "../evil", "");
        let err = WorkspaceManager::load(root).err().unwrap().to_string();
        assert!(err.contains("is named '../evil'"), "{}", err);

        // An empty name is the directory's.
        write_member(root, "a", "", "");
        assert_eq!(WorkspaceManager::load(root).unwrap().member_names(), ["a"]);
    }

    #[test]
    fn test_adding_and_removing_members_keeps_the_root_manifest_as_written() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        let manifest = "# The monorepo.\n[package]\nname = \"ws\"\nversion = \"0.1.0\"\n\n\
                        [workspace]\nmembers = [\n    \"core\", # the core\n]\n";
        std::fs::write(root.join("cmod.toml"), manifest).unwrap();
        write_member(root, "core", "core", "");
        write_member(root, "app", "app", "");

        let mut ws = WorkspaceManager::load(root).unwrap();
        ws.add_member("app", false).unwrap();
        let written = std::fs::read_to_string(root.join("cmod.toml")).unwrap();
        assert!(
            written.starts_with("# The monorepo.\n[package]\n"),
            "{}",
            written
        );
        assert!(
            written.contains("members = [\n    \"core\", # the core\n    \"app\",\n]\n"),
            "{}",
            written
        );
        assert!(!written.contains("[dependencies]"), "{}", written);

        ws.remove_member("app").unwrap();
        let written = std::fs::read_to_string(root.join("cmod.toml")).unwrap();
        assert!(
            written.contains("members = [\n    \"core\", # the core\n]\n"),
            "{}",
            written
        );
        assert!(!written.contains("exclude"), "{}", written);
        assert_eq!(
            WorkspaceManager::load(root).unwrap().member_names(),
            ["core"]
        );
    }

    #[test]
    fn test_member_lists_keep_crlf_line_endings() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        std::fs::write(
            root.join("cmod.toml"),
            "[package]\r\nname = \"ws\"\r\nversion = \"0.1.0\"\r\n\r\n[workspace]\r\nmembers = [\r\n  \"core\",\r\n]\r\n",
        )
        .unwrap();
        write_member(root, "core", "core", "");
        write_member(root, "v1..2", "app", "");
        let mut ws = WorkspaceManager::load(root).unwrap();
        ws.add_member("v1..2", false).unwrap();
        let written = std::fs::read_to_string(root.join("cmod.toml")).unwrap();
        assert_eq!(
            written,
            "[package]\r\nname = \"ws\"\r\nversion = \"0.1.0\"\r\n\r\n[workspace]\r\nmembers = [\r\n  \"core\",\r\n  \"v1..2\",\r\n]\r\n"
        );
        assert!(ws.add_member("../outside", false).is_err());
    }

    #[test]
    fn test_add_member_rejects_a_directory_already_a_member() {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path();
        write_root(root, "members = [\"crates/*\"]\n");
        write_member(root, "crates/a", "a", "");
        write_member(root, "other/a", "a", "");
        let mut ws = WorkspaceManager::load(root).unwrap();

        let err = ws.add_member("crates/a", false).unwrap_err().to_string();
        assert!(
            err.contains("already a workspace member (named 'a')"),
            "{}",
            err
        );
        let err = ws.add_member("other/a", false).unwrap_err().to_string();
        assert!(
            err.contains("two workspace members are named 'a'"),
            "{}",
            err
        );
        let manifest = std::fs::read_to_string(root.join("cmod.toml")).unwrap();
        assert!(!manifest.contains("other/a"), "{}", manifest);
    }
}
