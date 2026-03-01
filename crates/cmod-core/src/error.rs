use thiserror::Error;

/// Exit codes as defined in the CLI specification.
pub const EXIT_SUCCESS: i32 = 0;
pub const EXIT_BUILD_FAILURE: i32 = 1;
pub const EXIT_RESOLUTION_ERROR: i32 = 2;
pub const EXIT_SECURITY_VIOLATION: i32 = 3;

#[derive(Error, Debug)]
pub enum CmodError {
    // Config / manifest errors
    #[error("manifest not found: {path}")]
    ManifestNotFound { path: String },

    #[error("invalid manifest: {reason}")]
    InvalidManifest { reason: String },

    #[error("workspace manifest not found: {path}")]
    WorkspaceManifestNotFound { path: String },

    // Module identity errors
    #[error("invalid module name '{name}': {reason}")]
    InvalidModuleName { name: String, reason: String },

    #[error("reserved module prefix '{prefix}' cannot be used")]
    ReservedModulePrefix { prefix: String },

    #[error("module name '{name}' does not match Git URL '{url}'")]
    ModuleNameMismatch { name: String, url: String },

    // Dependency errors
    #[error("dependency '{name}' not found")]
    DependencyNotFound { name: String },

    #[error("dependency '{name}' already exists")]
    DependencyAlreadyExists { name: String },

    #[error("version conflict for '{name}': {reason}")]
    VersionConflict { name: String, reason: String },

    #[error("circular dependency detected: {cycle}")]
    CircularDependency { cycle: String },

    #[error("unresolvable constraints for '{name}': {reason}")]
    UnresolvableConstraints { name: String, reason: String },

    // Lockfile errors
    #[error("lockfile not found; run `cmod resolve` first")]
    LockfileNotFound,

    #[error("lockfile is outdated; run `cmod resolve` to update")]
    LockfileOutdated,

    #[error("lockfile integrity check failed: {reason}")]
    LockfileIntegrity { reason: String },

    // Build errors
    #[error("build failed: {reason}")]
    BuildFailed { reason: String },

    #[error("compiler not found: {compiler}")]
    CompilerNotFound { compiler: String },

    #[error("module scan failed: {reason}")]
    ModuleScanFailed { reason: String },

    // Git errors
    #[error("git operation failed: {reason}")]
    GitError { reason: String },

    #[error("git repository not found: {url}")]
    GitRepoNotFound { url: String },

    #[error("git ref not found: {reference} in {url}")]
    GitRefNotFound { reference: String, url: String },

    // Cache errors
    #[error("cache error: {reason}")]
    CacheError { reason: String },

    // Security errors
    #[error("security violation: {reason}")]
    SecurityViolation { reason: String },

    // IO / generic
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("{0}")]
    Other(String),
}

impl CmodError {
    /// Return the process exit code for this error category.
    pub fn exit_code(&self) -> i32 {
        match self {
            CmodError::BuildFailed { .. }
            | CmodError::CompilerNotFound { .. }
            | CmodError::ModuleScanFailed { .. } => EXIT_BUILD_FAILURE,

            CmodError::DependencyNotFound { .. }
            | CmodError::DependencyAlreadyExists { .. }
            | CmodError::VersionConflict { .. }
            | CmodError::CircularDependency { .. }
            | CmodError::UnresolvableConstraints { .. }
            | CmodError::LockfileNotFound
            | CmodError::LockfileOutdated
            | CmodError::LockfileIntegrity { .. } => EXIT_RESOLUTION_ERROR,

            CmodError::SecurityViolation { .. } => EXIT_SECURITY_VIOLATION,

            _ => EXIT_BUILD_FAILURE,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_errors_exit_code_1() {
        let errors = vec![
            CmodError::BuildFailed {
                reason: "test".into(),
            },
            CmodError::CompilerNotFound {
                compiler: "clang".into(),
            },
            CmodError::ModuleScanFailed {
                reason: "test".into(),
            },
        ];
        for e in errors {
            assert_eq!(e.exit_code(), EXIT_BUILD_FAILURE, "for {:?}", e);
        }
    }

    #[test]
    fn test_resolution_errors_exit_code_2() {
        let errors: Vec<CmodError> = vec![
            CmodError::DependencyNotFound {
                name: "pkg".into(),
            },
            CmodError::DependencyAlreadyExists {
                name: "pkg".into(),
            },
            CmodError::VersionConflict {
                name: "pkg".into(),
                reason: "test".into(),
            },
            CmodError::CircularDependency {
                cycle: "a -> b -> a".into(),
            },
            CmodError::UnresolvableConstraints {
                name: "pkg".into(),
                reason: "test".into(),
            },
            CmodError::LockfileNotFound,
            CmodError::LockfileOutdated,
            CmodError::LockfileIntegrity {
                reason: "bad".into(),
            },
        ];
        for e in errors {
            assert_eq!(e.exit_code(), EXIT_RESOLUTION_ERROR, "for {:?}", e);
        }
    }

    #[test]
    fn test_error_display() {
        let e = CmodError::ManifestNotFound {
            path: "/tmp/cmod.toml".into(),
        };
        assert_eq!(format!("{}", e), "manifest not found: /tmp/cmod.toml");

        let e = CmodError::CircularDependency {
            cycle: "a -> b -> a".into(),
        };
        assert_eq!(
            format!("{}", e),
            "circular dependency detected: a -> b -> a"
        );
    }

    #[test]
    fn test_io_error_conversion() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "gone");
        let cmod_err: CmodError = io_err.into();
        assert_eq!(cmod_err.exit_code(), EXIT_BUILD_FAILURE);
        assert!(format!("{}", cmod_err).contains("gone"));
    }
}
