//! `cmod registry` — registry index maintenance commands.
//!
//! `validate` is the gate the cmod-registry validation Action runs on
//! submission PRs (#79): the exact governance rules from
//! `validate_for_publishing`, plus the no-deletions policy when a base
//! revision is supplied.

use std::path::Path;

use cmod_core::error::CmodError;
use cmod_core::shell::Shell;
use cmod_resolver::registry::{validate_index, validate_index_against_base, GovernancePolicy};
use cmod_resolver::RegistryIndex;

/// Run `cmod registry validate <path> [--against <base>]`.
pub fn validate(path: &str, against: Option<&str>, shell: &Shell) -> Result<(), CmodError> {
    let index = RegistryIndex::load(Path::new(path))?;

    let mut violations = validate_index(&index, &GovernancePolicy::default());

    if let Some(base_path) = against {
        let base = RegistryIndex::load(Path::new(base_path))?;
        violations.extend(validate_index_against_base(&index, &base));
    }

    if violations.is_empty() {
        shell.status(
            "Valid",
            format!("{} ({} modules)", path, index.modules.len()),
        );
        return Ok(());
    }

    for v in &violations {
        shell.error(v);
    }
    Err(CmodError::Other(format!(
        "{} registry policy violation(s) in {}",
        violations.len(),
        path
    )))
}
