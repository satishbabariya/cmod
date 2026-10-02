//! `cmod emit-cmake`: a CMakeLists.txt that builds the package with CMake.
//!
//! One target per package: the package itself (or every member of a
//! workspace), its path dependencies and its fetched git dependencies.
//! Module interfaces and partitions go in a `CXX_MODULES` file set, which
//! CMake 3.28+ scans and orders; targets link the targets of the
//! dependencies their manifests declare, which also gives them those
//! dependencies' modules and include directories.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use cmod_build::runner;
use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::manifest::Manifest;
use cmod_core::shell::Shell;
use cmod_core::types::{BuildType, ModuleUnitKind};
use cmod_workspace::WorkspaceManager;

use super::common::{detect_include_dirs, find_dep_on_disk};

/// Run `cmod emit-cmake`.
pub fn run(shell: &Shell) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    let roots: Vec<PathBuf> = if config.manifest.is_workspace() {
        WorkspaceManager::load(&config.root)?
            .members
            .iter()
            .map(|m| m.path.clone())
            .collect()
    } else {
        vec![config.root.clone()]
    };

    let mut graph = PackageGraph::new(&config);
    for root in &roots {
        graph.add(root, true, shell)?;
    }

    let content = graph.render(&config);
    let cmake_path = config.root.join("CMakeLists.txt");
    std::fs::write(&cmake_path, &content)?;

    shell.status("Generated", format!("{}", cmake_path.display()));
    shell.verbose("Targets", format!("{} target(s)", graph.packages.len()));
    Ok(())
}

/// One CMake target.
struct Package {
    /// Target name: the package name, made unique and valid.
    target: String,
    dir: PathBuf,
    kind: TargetKind,
    /// Module interfaces and partitions.
    module_units: Vec<PathBuf>,
    /// Implementation units and plain translation units.
    sources: Vec<PathBuf>,
    include_dirs: Vec<PathBuf>,
    extra_flags: Vec<String>,
    /// Directories of the packages it depends on.
    deps: Vec<PathBuf>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TargetKind {
    Executable,
    Static,
    Shared,
    /// Headers only: nothing to compile.
    Interface,
}

/// Every package reachable from the roots, keyed by directory, in an order
/// where dependencies come first.
struct PackageGraph<'a> {
    root: &'a Config,
    packages: Vec<Package>,
    index: HashMap<PathBuf, usize>,
    taken: HashMap<String, usize>,
}

impl<'a> PackageGraph<'a> {
    fn new(root: &'a Config) -> Self {
        PackageGraph {
            root,
            packages: Vec::new(),
            index: HashMap::new(),
            taken: HashMap::new(),
        }
    }

    /// Add the package in `dir` and, first, everything it depends on.
    /// `top` marks the package or workspace member being exported, which
    /// keeps its own build type; dependencies are linked in, so they are
    /// libraries.
    fn add(&mut self, dir: &Path, top: bool, shell: &Shell) -> Result<(), CmodError> {
        let dir = dir.canonicalize().unwrap_or_else(|_| dir.to_path_buf());
        if self.index.contains_key(&dir) {
            return Ok(());
        }
        // Reserve the slot so a dependency cycle stops here.
        self.index.insert(dir.clone(), usize::MAX);

        // Without its own manifest, `Config::load` would find an enclosing
        // one: such a directory provides headers only.
        let config = if dir.join("cmod.toml").is_file() {
            Some(Config::load(&dir)?)
        } else {
            None
        };

        let mut deps = Vec::new();
        if let Some(ref config) = config {
            for (name, dep) in &config.manifest.dependencies {
                let dep_dir = match dep.path() {
                    Some(path) => Some(dir.join(path)),
                    None => find_dep_on_disk(
                        &self.root.root.join("vendor"),
                        &self.root.deps_dir(),
                        name,
                    ),
                };
                match dep_dir {
                    Some(dep_dir) => {
                        self.add(&dep_dir, false, shell)?;
                        deps.push(dep_dir.canonicalize().unwrap_or(dep_dir));
                    }
                    None => shell.warn(format!(
                        "{}: not checked out, so it has no target; run `cmod build` first",
                        name
                    )),
                }
            }
        }

        let package = self.describe(&dir, config.as_ref(), top, deps)?;
        let slot = self.packages.len();
        self.packages.push(package);
        self.index.insert(dir, slot);
        Ok(())
    }

    fn describe(
        &mut self,
        dir: &Path,
        config: Option<&Config>,
        top: bool,
        deps: Vec<PathBuf>,
    ) -> Result<Package, CmodError> {
        let name = config
            .map(|c| c.manifest.package.name.clone())
            .unwrap_or_else(|| {
                dir.file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_else(|| "dep".to_string())
            });
        let target = self.unique_target(&name);

        let Some(config) = config else {
            let include_dirs = ["include", "inc"]
                .iter()
                .map(|d| dir.join(d))
                .filter(|d| d.is_dir())
                .collect();
            return Ok(Package {
                target,
                dir: dir.to_path_buf(),
                kind: TargetKind::Interface,
                module_units: Vec::new(),
                sources: Vec::new(),
                include_dirs,
                extra_flags: Vec::new(),
                deps,
            });
        };

        let found = runner::discover_sources_multi(&config.src_dirs(), &config.exclude_patterns())?;
        let mut module_units = Vec::new();
        let mut sources = Vec::new();
        for source in runner::filter_included_sources(&found) {
            match runner::classify_source(&source)? {
                ModuleUnitKind::InterfaceUnit | ModuleUnitKind::PartitionUnit => {
                    module_units.push(source)
                }
                ModuleUnitKind::ImplementationUnit | ModuleUnitKind::LegacyUnit => {
                    sources.push(source)
                }
            }
        }

        let build = config.manifest.build.as_ref();
        let build_type = build.and_then(|b| b.build_type).unwrap_or_default();
        let kind = if module_units.is_empty() && sources.is_empty() {
            TargetKind::Interface
        } else {
            match build_type {
                BuildType::SharedLib => TargetKind::Shared,
                BuildType::Binary if top => TargetKind::Executable,
                _ => TargetKind::Static,
            }
        };

        Ok(Package {
            target,
            dir: dir.to_path_buf(),
            kind,
            module_units,
            sources,
            include_dirs: detect_include_dirs(dir, config),
            extra_flags: build.map(|b| b.extra_flags.clone()).unwrap_or_default(),
            deps,
        })
    }

    /// `name` as a CMake target name, unlike any target before it.
    fn unique_target(&mut self, name: &str) -> String {
        let base: String = name
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || "_.+-".contains(c) {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let count = self.taken.entry(base.clone()).or_insert(0);
        *count += 1;
        if *count == 1 {
            base
        } else {
            format!("{}_{}", base, count)
        }
    }

    fn render(&self, config: &Config) -> String {
        let manifest: &Manifest = &config.manifest;
        let cxx_standard = manifest
            .toolchain
            .as_ref()
            .and_then(|tc| tc.cxx_standard.clone())
            .unwrap_or_else(|| "20".to_string());
        let mut out = String::new();
        out.push_str("# Generated by cmod emit-cmake: do not edit manually.\n");
        out.push_str("# Needs CMake 3.28+ and a generator that supports C++20 modules (Ninja).\n");
        out.push_str("cmake_minimum_required(VERSION 3.28)\n");
        out.push_str(&format!(
            "project({} VERSION {} LANGUAGES CXX)\n\n",
            manifest.package.name,
            cmake_version(&manifest.package.version)
        ));
        out.push_str(&format!("set(CMAKE_CXX_STANDARD {})\n", cxx_standard));
        out.push_str("set(CMAKE_CXX_STANDARD_REQUIRED ON)\n");
        // Static libraries linked into a shared one need PIC too.
        if self.packages.iter().any(|p| p.kind == TargetKind::Shared) {
            out.push_str("set(CMAKE_POSITION_INDEPENDENT_CODE ON)\n");
        }

        let targets: BTreeMap<&Path, &str> = self
            .packages
            .iter()
            .map(|p| (p.dir.as_path(), p.target.as_str()))
            .collect();
        for package in &self.packages {
            out.push('\n');
            self.render_package(&mut out, package, &targets);
        }
        out
    }

    fn render_package(&self, out: &mut String, p: &Package, targets: &BTreeMap<&Path, &str>) {
        let rel = |path: &Path| cmake_path(path, &self.root.root);
        out.push_str(&format!("# {}\n", rel(&p.dir)));
        let (decl, visibility) = match p.kind {
            TargetKind::Executable => (format!("add_executable({})", p.target), "PRIVATE"),
            TargetKind::Static => (format!("add_library({} STATIC)", p.target), "PUBLIC"),
            TargetKind::Shared => (format!("add_library({} SHARED)", p.target), "PUBLIC"),
            TargetKind::Interface => (format!("add_library({} INTERFACE)", p.target), "INTERFACE"),
        };
        out.push_str(&decl);
        out.push('\n');

        if !p.module_units.is_empty() || !p.sources.is_empty() {
            out.push_str(&format!("target_sources({}\n", p.target));
            if !p.module_units.is_empty() {
                out.push_str(&format!(
                    "  {} FILE_SET CXX_MODULES BASE_DIRS {} FILES\n",
                    visibility,
                    rel(&p.dir)
                ));
                for unit in &p.module_units {
                    out.push_str(&format!("    {}\n", rel(unit)));
                }
            }
            if !p.sources.is_empty() {
                out.push_str("  PRIVATE\n");
                for source in &p.sources {
                    out.push_str(&format!("    {}\n", rel(source)));
                }
            }
            out.push_str(")\n");
        }

        if !p.include_dirs.is_empty() {
            let dirs: Vec<String> = p.include_dirs.iter().map(|d| rel(d)).collect();
            out.push_str(&format!(
                "target_include_directories({} {} {})\n",
                p.target,
                visibility,
                dirs.join(" ")
            ));
        }
        if !p.extra_flags.is_empty() && p.kind != TargetKind::Interface {
            out.push_str(&format!(
                "target_compile_options({} PRIVATE {})\n",
                p.target,
                p.extra_flags
                    .iter()
                    .map(|f| cmake_quote(f))
                    .collect::<Vec<_>>()
                    .join(" ")
            ));
        }
        let deps: Vec<&str> = p
            .deps
            .iter()
            .filter_map(|d| targets.get(d.as_path()).copied())
            .collect();
        if !deps.is_empty() {
            let link = if p.kind == TargetKind::Interface {
                "INTERFACE"
            } else {
                "PUBLIC"
            };
            out.push_str(&format!(
                "target_link_libraries({} {} {})\n",
                p.target,
                link,
                deps.join(" ")
            ));
        }
    }
}

/// `path` as CMake should see it: relative to the generated file's
/// directory when inside it, with forward slashes.
fn cmake_path(path: &Path, root: &Path) -> String {
    let root = root.canonicalize().unwrap_or_else(|_| root.to_path_buf());
    let shown = path
        .canonicalize()
        .unwrap_or_else(|_| path.to_path_buf())
        .strip_prefix(&root)
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| path.to_path_buf());
    let text = shown.to_string_lossy().replace('\\', "/");
    if text.is_empty() {
        ".".to_string()
    } else {
        cmake_quote(&text)
    }
}

/// Quote `arg` when CMake would split or expand it.
fn cmake_quote(arg: &str) -> String {
    if arg
        .chars()
        .any(|c| c.is_whitespace() || "\"#;()$\\".contains(c))
    {
        format!(
            "\"{}\"",
            arg.replace('\\', "\\\\")
                .replace('"', "\\\"")
                .replace('$', "\\$")
        )
    } else {
        arg.to_string()
    }
}

/// CMake's `project(VERSION)` takes only numbers: `0.1.0-alpha.2` becomes
/// `0.1.0`.
fn cmake_version(version: &str) -> String {
    let numeric: Vec<&str> = version
        .split(['-', '+'])
        .next()
        .unwrap_or("0")
        .split('.')
        .take_while(|part| !part.is_empty() && part.chars().all(|c| c.is_ascii_digit()))
        .take(4)
        .collect();
    if numeric.is_empty() {
        "0".to_string()
    } else {
        numeric.join(".")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cmake_version() {
        assert_eq!(cmake_version("0.1.0"), "0.1.0");
        assert_eq!(cmake_version("0.1.0-alpha.8"), "0.1.0");
        assert_eq!(cmake_version("1.2+build"), "1.2");
        assert_eq!(cmake_version("v1"), "0");
    }

    #[test]
    fn test_cmake_quote() {
        assert_eq!(cmake_quote("-Wall"), "-Wall");
        assert_eq!(cmake_quote("-DNAME=a b"), "\"-DNAME=a b\"");
        assert_eq!(cmake_quote("$x"), "\"\\$x\"");
    }
}
