use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::lockfile::Lockfile;
use cmod_core::shell::{Shell, Verbosity};
use cmod_security::audit::{audit_dependencies, Severity};

/// Run `cmod audit` — audit dependencies for security and quality issues.
pub fn run(shell: &Shell) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    shell.status("Auditing", "dependencies...");

    let lockfile = if config.lockfile_path.exists() {
        Lockfile::load(&config.lockfile_path)?
    } else if config.manifest.dependencies.is_empty() {
        shell.status("Audited", "no dependencies to audit");
        return Ok(());
    } else {
        return Err(CmodError::LockfileNotFound);
    };

    let report = audit_dependencies(&config.manifest, &lockfile)?;

    if report.findings.is_empty() {
        shell.status(
            "Audited",
            format!("no issues found ({} dependencies)", lockfile.packages.len()),
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

    for f in &errors {
        shell.error(format!("{}: {}", f.package, f.message));
    }

    for f in &warnings {
        shell.warn(format!("{}: {}", f.package, f.message));
    }

    if shell.verbosity() == Verbosity::Verbose {
        for f in &infos {
            shell.note(format!("{}: {}", f.package, f.message));
        }
    }

    shell.status(
        "Audited",
        format!(
            "{} error(s), {} warning(s), {} info(s)",
            report.error_count(),
            report.warning_count(),
            infos.len(),
        ),
    );

    if report.has_errors() {
        Err(CmodError::SecurityViolation {
            reason: format!("audit found {} error(s)", report.error_count()),
        })
    } else {
        Ok(())
    }
}
