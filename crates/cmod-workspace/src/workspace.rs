use std::collections::BTreeMap;
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
    pub fn load(root: &Path) -> Result<Self, CmodError> {
        let manifest_path = root.join("cmod.toml");
        let root_manifest = Manifest::load(&manifest_path)?;

        if !root_manifest.is_workspace() {
            return Err(CmodError::WorkspaceManifestNotFound {
                path: manifest_path.display().to_string(),
            });
        }

        let workspace = root_manifest.workspace.as_ref().unwrap();
        let mut members = Vec::new();

        for member_pattern in &workspace.members {
            let member_dir = root.join(member_pattern);
            if member_dir.is_dir() {
                let member_manifest_path = member_dir.join("cmod.toml");
                if member_manifest_path.exists() {
                    let mut member_manifest = Manifest::load(&member_manifest_path)?;

                    // Resolve workspace dependency references
                    resolve_workspace_deps(&mut member_manifest, &workspace.dependencies);

                    members.push(WorkspaceMember {
                        name: member_pattern.clone(),
                        path: member_dir,
                        manifest: member_manifest,
                    });
                }
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
                member_manifest
                    .dependencies
                    .insert(name, ws_dep.clone());
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
        std::fs::write(root.join("core/src/lib.cppm"), "export module local.core;")
            .unwrap();

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
        std::fs::write(root.join("app/src/lib.cppm"), "export module local.app;")
            .unwrap();

        tmp
    }

    #[test]
    fn test_load_workspace() {
        let tmp = setup_workspace();
        let ws = WorkspaceManager::load(tmp.path()).unwrap();
        assert_eq!(ws.members.len(), 2);
        assert_eq!(ws.member_names(), vec!["core", "app"]);
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
}
