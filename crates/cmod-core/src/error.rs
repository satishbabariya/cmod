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

            _ => EXIT_BUILD_FAILURE,
        }
    }
}
