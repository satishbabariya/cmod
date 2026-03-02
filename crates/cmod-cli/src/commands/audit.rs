use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::lockfile::Lockfile;
use cmod_security::audit::{audit_dependencies, Severity};

/// Run `cmod audit` — audit dependencies for security and quality issues.
pub fn run(verbose: bool) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    eprintln!("  Auditing dependencies...");

    let lockfile = if config.lockfile_path.exists() {
        Lockfile::load(&config.lockfile_path)?
    } else if config.manifest.dependencies.is_empty() {
        eprintln!("  No dependencies to audit.");
        return Ok(());
    } else {
        return Err(CmodError::LockfileNotFound);
    };

    let report = audit_dependencies(&config.manifest, &lockfile)?;

    if report.findings.is_empty() {
        eprintln!(
            "  No issues found. {} dependencies audited.",
            lockfile.packages.len()
        );
        return Ok(());
    }

    // Print findings grouped by severity
    let errors: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.severity == Severity::Error)
        .collect();
    let warnings: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.severity == Severity::Warning)
        .collect();
    let infos: Vec<_> = report
        .findings
        .iter()
        .filter(|f| f.severity == Severity::Info)
        .collect();

    if !errors.is_empty() {
        eprintln!("  Errors:");
        for f in &errors {
            eprintln!("    [error] {}: {}", f.package, f.message);
        }
    }

    if !warnings.is_empty() {
        eprintln!("  Warnings:");
        for f in &warnings {
            eprintln!("    [warn] {}: {}", f.package, f.message);
        }
    }

    if verbose && !infos.is_empty() {
        eprintln!("  Info:");
        for f in &infos {
            eprintln!("    [info] {}: {}", f.package, f.message);
        }
    }

    eprintln!(
        "\n  {} error(s), {} warning(s), {} info(s)",
        report.error_count(),
        report.warning_count(),
        infos.len(),
    );

    if report.has_errors() {
        Err(CmodError::SecurityViolation {
            reason: format!("audit found {} error(s)", report.error_count()),
        })
    } else {
        Ok(())
    }
}
