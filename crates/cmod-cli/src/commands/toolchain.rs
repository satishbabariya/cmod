use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::shell::Shell;
use cmod_core::types::ToolchainSpec;

/// Run `cmod toolchain show` — display the active toolchain configuration.
pub fn show(shell: &Shell) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;

    let spec = if let Ok(config) = Config::load(&cwd) {
        from_manifest(&config)
    } else {
        ToolchainSpec::default()
    };

    shell.status("Toolchain", "active configuration");
    eprintln!("    Compiler:  {}", spec.compiler);
    if let Some(ref ver) = spec.compiler_version {
        eprintln!("    Version:   {}", ver);
    }
    eprintln!("    Standard:  C++{}", spec.cxx_standard);
    if let Some(ref stdlib) = spec.stdlib {
        eprintln!("    Stdlib:    {}", stdlib);
    }
    eprintln!("    Target:    {}", spec.target);
    eprintln!("    Host:      {}", ToolchainSpec::host_target());
    if spec.is_cross_compiling() {
        eprintln!("    Cross:     yes");
    }
    if let Some(ref sysroot) = spec.sysroot {
        eprintln!("    Sysroot:   {}", sysroot.display());
    }

    shell.verbose("Cache key", spec.cache_key_tuple());

    Ok(())
}

/// Run `cmod toolchain check` — validate the toolchain is available.
pub fn check() -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;

    let spec = if let Ok(config) = Config::load(&cwd) {
        from_manifest(&config)
    } else {
        ToolchainSpec::default()
    };

    eprintln!("  Checking toolchain...");

    spec.validate()?;
    eprintln!(
        "  {} {} is available",
        spec.compiler,
        spec.compiler_version
            .as_deref()
            .unwrap_or("(version unknown)")
    );

    if spec.is_cross_compiling() {
        eprintln!("  Cross-compilation target: {}", spec.target);
        if spec.sysroot.is_none() {
            eprintln!("  Warning: cross-compiling without explicit sysroot");
        }
    }

    eprintln!("  Toolchain OK");
    Ok(())
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
