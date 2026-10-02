use std::path::Path;

use cmod_build::compiler::{make_backend, BackendConfig, ClangBackend, CompilerBackend};
use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::shell::Shell;
use cmod_core::types::{Compiler, ToolchainSpec};

/// Run `cmod toolchain show` — display the active toolchain configuration.
pub fn show(shell: &Shell) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;

    let spec = if let Ok(config) = Config::load(&cwd) {
        from_manifest(&config)
    } else {
        ToolchainSpec::default()
    };

    shell.status("Toolchain", "active configuration");
    shell.status("Compiler", &spec.compiler);
    if let Some(ref ver) = spec.compiler_version {
        shell.status("Requires", format!("version {}", ver));
    }
    let backend = resolved_backend(Config::load(&cwd).ok().as_ref());
    let version = backend.version();
    shell.status(
        "Resolved",
        format!(
            "{} ({})",
            backend.compiler_path().display(),
            if version.is_empty() {
                "not found"
            } else {
                &version
            }
        ),
    );
    shell.status("Standard", format!("C++{}", spec.cxx_standard));
    if let Some(ref stdlib) = spec.stdlib {
        shell.status("Stdlib", stdlib);
    }
    shell.status("Target", &spec.target);
    shell.status("Host", ToolchainSpec::host_target());
    if spec.is_cross_compiling() {
        shell.status("Cross", "yes");
    }
    if let Some(ref sysroot) = spec.sysroot {
        shell.status("Sysroot", sysroot.display());
    }

    shell.verbose("Cache key", spec.cache_key_tuple());

    Ok(())
}

/// Run `cmod toolchain check` — validate the toolchain a build would use:
/// the compiler it runs (`CXX`, or the one found on `PATH`) must run and
/// satisfy `[toolchain] version`; one that cannot build modules, and a
/// missing dependency scanner, are warned about.
pub fn check(shell: &Shell) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd).ok();

    let spec = config.as_ref().map(from_manifest).unwrap_or_default();

    shell.status("Checking", "toolchain...");

    let backend = resolved_backend(config.as_ref());
    let path = backend.compiler_path().to_path_buf();
    let version = backend.version();
    if version.is_empty() {
        return Err(CmodError::CompilerNotFound {
            compiler: format!(
                "{} ({} did not run; set CXX to the compiler to use)",
                path.display(),
                spec.compiler
            ),
        });
    }
    shell.status(
        "Compiler",
        format!("{} {} ({})", spec.compiler, version, path.display()),
    );

    if let Some(config) = &config {
        if let Some(problem) = unmet_version(config, &version) {
            return Err(CmodError::Other(problem));
        }
    }

    let banner = version_banner(&path);
    if let Some(problem) = module_support_problem(&spec.compiler, &version, banner.as_deref()) {
        shell.warn(problem);
    }

    if spec.compiler == Compiler::Clang {
        let scanner = clang_backend(config.as_ref()).scan_deps_path;
        match version_banner(&scanner) {
            Some(_) => shell.status("Scanner", scanner.display()),
            None => shell.warn(format!(
                "clang-scan-deps not found at {}: imports are read from the source text, so an `import` the preprocessor removes still counts (set SCAN_DEPS to it)",
                scanner.display()
            )),
        }
    }

    if spec.is_cross_compiling() {
        shell.status("Cross", format!("target: {}", spec.target));
        if spec.sysroot.is_none() {
            shell.warn("cross-compiling without explicit sysroot");
        }
    }

    shell.status("Finished", "toolchain OK");
    Ok(())
}

/// The compiler backend a build of the package in `config` (or, outside a
/// package, a default Clang build) would use.
fn resolved_backend(config: Option<&Config>) -> Box<dyn CompilerBackend> {
    let (backend_cfg, kind) = backend_config(config);
    make_backend(kind, &backend_cfg)
        .unwrap_or_else(|_| Box::new(ClangBackend::from_config(&backend_cfg)))
}

/// The Clang backend a build would use, for its scanner.
fn clang_backend(config: Option<&Config>) -> ClangBackend {
    ClangBackend::from_config(&backend_config(config).0)
}

fn backend_config(config: Option<&Config>) -> (BackendConfig, Compiler) {
    match config {
        Some(config) => {
            let (backend_cfg, kind, _) = super::build::setup_compiler(config, &[]);
            (backend_cfg, kind)
        }
        None => (
            BackendConfig {
                cxx_standard: "20".to_string(),
                ..Default::default()
            },
            Compiler::Clang,
        ),
    }
}

/// The first line `<path> --version` prints, if it runs.
fn version_banner(path: &Path) -> Option<String> {
    let output = std::process::Command::new(path)
        .arg("--version")
        .output()
        .ok()
        .filter(|o| o.status.success())?;
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .next()
        .map(|line| line.trim().to_string())
}

/// `version` (`18.1.3`, `14.2.0`, `19.40.33811`) as a semantic version:
/// its first three numeric components, missing ones as 0.
fn compiler_semver(version: &str) -> Option<semver::Version> {
    let numeric: String = version
        .chars()
        .take_while(|c| c.is_ascii_digit() || *c == '.')
        .collect();
    let mut parts = numeric
        .split('.')
        .filter(|p| !p.is_empty())
        .map(|p| p.parse::<u64>().ok());
    let major = parts.next()??;
    let minor = parts.next().flatten().unwrap_or(0);
    let patch = parts.next().flatten().unwrap_or(0);
    Some(semver::Version::new(major, minor, patch))
}

/// Why compiler version `detected` does not meet `[toolchain] version`, if
/// it does not. The constraint reads as Cargo reads one: `18` is any 18.x,
/// `18.1.0` that or a later 18.x, `=18.1.8` exactly that, `>=17, <20` a
/// range.
pub(crate) fn unmet_version(config: &Config, detected: &str) -> Option<String> {
    let toolchain = config.manifest.toolchain.as_ref()?;
    let wanted = toolchain.version.as_deref()?;
    let compiler = toolchain.compiler.clone().unwrap_or(Compiler::Clang);
    let requirement = match semver::VersionReq::parse(wanted.trim()) {
        Ok(requirement) => requirement,
        Err(e) => {
            return Some(format!(
                "[toolchain] version = \"{}\" is not a version constraint: {}",
                wanted, e
            ))
        }
    };
    match compiler_semver(detected) {
        Some(version) if requirement.matches(&version) => None,
        Some(_) => Some(format!(
            "{} {} does not satisfy [toolchain] version = \"{}\"; set CXX to a compiler that does",
            compiler, detected, wanted
        )),
        None => Some(format!(
            "could not tell the {} version (\"{}\") to check [toolchain] version = \"{}\"",
            compiler, detected, wanted
        )),
    }
}

/// Why a compiler cannot build C++20 modules as cmod drives them, if so:
/// Apple's clang, or GCC before 14.
fn module_support_problem(
    compiler: &Compiler,
    version: &str,
    banner: Option<&str>,
) -> Option<String> {
    let major = compiler_semver(version).map(|v| v.major);
    match compiler {
        Compiler::Clang if banner.is_some_and(|b| b.starts_with("Apple clang")) => Some(
            "Apple clang cannot build C++20 modules; install LLVM (`brew install llvm`) and set CXX to its clang++"
                .to_string(),
        ),
        Compiler::Gcc if major.is_some_and(|m| m < 14) => Some(format!(
            "GCC {} cannot build C++20 modules with cmod; GCC 14 or later is needed",
            version
        )),
        _ => None,
    }
}

/// Build a ToolchainSpec from a Config's manifest.
fn from_manifest(config: &Config) -> ToolchainSpec {
    let mut spec = ToolchainSpec::default();

    if let Some(ref tc) = config.manifest.toolchain {
        if let Some(ref compiler) = tc.compiler {
            spec.compiler = compiler.clone();
        }
        if let Some(ref ver) = tc.version {
            spec.compiler_version = Some(ver.clone());
        }
        if let Some(ref std) = tc.cxx_standard {
            spec.cxx_standard = std.clone();
        }
        if let Some(ref stdlib) = tc.stdlib {
            spec.stdlib = Some(stdlib.clone());
        }
        if let Some(ref target) = tc.target {
            spec.target = target.clone();
        }
        if let Some(ref sysroot) = tc.sysroot {
            spec.sysroot = Some(sysroot.clone());
        }
    }

    if let Some(ref target) = config.target {
        spec.target = target.clone();
    }

    spec
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with(toolchain: &str) -> (tempfile::TempDir, Config) {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("cmod.toml"),
            format!(
                "[package]\nname = \"p\"\nversion = \"0.1.0\"\n\n[toolchain]\n{}\n",
                toolchain
            ),
        )
        .unwrap();
        let config = Config::load(tmp.path()).unwrap();
        (tmp, config)
    }

    #[test]
    fn test_compiler_semver_reads_compiler_versions() {
        assert_eq!(
            compiler_semver("18.1.3"),
            Some(semver::Version::new(18, 1, 3))
        );
        assert_eq!(compiler_semver("14"), Some(semver::Version::new(14, 0, 0)));
        assert_eq!(
            compiler_semver("19.40.33811.2"),
            Some(semver::Version::new(19, 40, 33811))
        );
        assert_eq!(
            compiler_semver("21.0.0git"),
            Some(semver::Version::new(21, 0, 0))
        );
        assert_eq!(compiler_semver(""), None);
    }

    #[test]
    fn test_unmet_version_reads_constraints_as_cargo_does() {
        let (_tmp, config) = config_with("compiler = \"clang\"\nversion = \"18\"");
        assert_eq!(unmet_version(&config, "18.1.3"), None);
        let problem = unmet_version(&config, "19.1.0").unwrap();
        assert!(
            problem.contains("clang 19.1.0 does not satisfy"),
            "{}",
            problem
        );

        let (_tmp, config) = config_with("version = \">=17, <19\"");
        assert_eq!(unmet_version(&config, "17.0.6"), None);
        assert!(unmet_version(&config, "19.0.0").is_some());

        let (_tmp, config) = config_with("version = \"=18.1.8\"");
        assert!(unmet_version(&config, "18.1.3").is_some());

        let (_tmp, config) = config_with("version = \"latest\"");
        assert!(unmet_version(&config, "18.1.3")
            .unwrap()
            .contains("is not a version constraint"));

        // No constraint, nothing to meet.
        let (_tmp, config) = config_with("compiler = \"clang\"");
        assert_eq!(unmet_version(&config, "1.0.0"), None);
    }

    #[test]
    fn test_module_support_problem() {
        assert!(module_support_problem(
            &Compiler::Clang,
            "17.0.0",
            Some("Apple clang version 17.0.0 (clang-1700.0.13.3)")
        )
        .unwrap()
        .contains("Apple clang"));
        assert_eq!(
            module_support_problem(
                &Compiler::Clang,
                "18.1.3",
                Some("Ubuntu clang version 18.1.3")
            ),
            None
        );
        assert!(module_support_problem(&Compiler::Gcc, "13.2.0", None).is_some());
        assert_eq!(module_support_problem(&Compiler::Gcc, "14.2.0", None), None);
    }
}
