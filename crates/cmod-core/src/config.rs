use std::path::{Path, PathBuf};

use crate::error::CmodError;
use crate::manifest::Manifest;
use crate::shell::Verbosity;
use crate::types::Profile;

/// Global context for a cmod session.
///
/// Holds resolved paths, the parsed manifest, and runtime configuration.
pub struct Config {
    /// Path to the project root (directory containing cmod.toml).
    pub root: PathBuf,

    /// Path to the manifest file.
    pub manifest_path: PathBuf,

    /// Parsed manifest.
    pub manifest: Manifest,

    /// Path to the lockfile.
    pub lockfile_path: PathBuf,

    /// Build profile.
    pub profile: Profile,

    /// Target triple override (from CLI or manifest).
    pub target: Option<String>,

    /// Whether to use the lockfile strictly (--locked).
    pub locked: bool,

    /// Whether to allow network access.
    pub offline: bool,

    /// Output verbosity level.
    pub verbosity: Verbosity,

    /// Explicitly enabled features (from --features CLI flag).
    pub enabled_features: Vec<String>,

    /// Whether default features are disabled (from --no-default-features).
    pub no_default_features: bool,

    /// Skip build cache lookups.
    pub no_cache: bool,
}

impl Config {
    /// Load configuration from the current working directory (or a specified path).
    pub fn load(start_dir: &Path) -> Result<Self, CmodError> {
        let manifest_path =
            Manifest::find(start_dir).ok_or_else(|| CmodError::ManifestNotFound {
                path: start_dir.join("cmod.toml").display().to_string(),
            })?;

        let root = manifest_path.parent().unwrap_or(start_dir).to_path_buf();

        let manifest = Manifest::load(&manifest_path)?;
        let lockfile_path = root.join("cmod.lock");

        Ok(Config {
            root,
            manifest_path,
            manifest,
            lockfile_path,
            profile: Profile::Debug,
            target: None,
            locked: false,
            offline: false,
            verbosity: Verbosity::Normal,
            enabled_features: Vec::new(),
            no_default_features: false,
            no_cache: false,
        })
    }

    /// Build output directory.
    pub fn build_dir(&self) -> PathBuf {
        let profile_name = match self.profile {
            Profile::Debug => "debug",
            Profile::Release => "release",
        };
        self.root.join("build").join(profile_name)
    }

    /// Local cache directory.
    pub fn cache_dir(&self) -> PathBuf {
        if let Some(ref cache) = self.manifest.cache {
            if let Some(ref local) = cache.local_path {
                return local.clone();
            }
        }
        dirs::cache_dir()
            .unwrap_or_else(|| PathBuf::from(".cache"))
            .join("cmod")
    }

    /// Dependency sources directory (where fetched deps are checked out).
    pub fn deps_dir(&self) -> PathBuf {
        self.root.join("build").join("deps")
    }

    /// Source directory (legacy, prefer `src_dirs()`).
    pub fn src_dir(&self) -> PathBuf {
        self.root.join("src")
    }

    /// Source directories from `[build].sources`, defaulting to `["src"]`.
    pub fn src_dirs(&self) -> Vec<PathBuf> {
        if let Some(ref build) = self.manifest.build {
            if !build.sources.is_empty() {
                return build.sources.iter().map(|s| self.root.join(s)).collect();
            }
        }
        vec![self.root.join("src")]
    }

    /// Exclude patterns from `[build].exclude`.
    pub fn exclude_patterns(&self) -> Vec<String> {
        self.manifest
            .build
            .as_ref()
            .map(|b| b.exclude.clone())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup_project() -> TempDir {
        let tmp = TempDir::new().unwrap();
        let toml = r#"
[package]
name = "test_project"
version = "0.1.0"
"#;
        std::fs::write(tmp.path().join("cmod.toml"), toml).unwrap();
        tmp
    }

    #[test]
    fn test_config_load() {
        let tmp = setup_project();
        let config = Config::load(tmp.path()).unwrap();
        assert_eq!(config.manifest.package.name, "test_project");
        assert_eq!(config.root, tmp.path());
        assert_eq!(config.lockfile_path, tmp.path().join("cmod.lock"));
        assert_eq!(config.profile, Profile::Debug);
        assert!(!config.locked);
        assert!(!config.offline);
        assert_eq!(config.verbosity, Verbosity::Normal);
    }

    #[test]
    fn test_config_load_not_found() {
        let tmp = TempDir::new().unwrap();
        let result = Config::load(tmp.path());
        assert!(result.is_err());
    }

    #[test]
    fn test_config_load_from_subdirectory() {
        let tmp = setup_project();
        let subdir = tmp.path().join("src").join("deep");
        std::fs::create_dir_all(&subdir).unwrap();

        let config = Config::load(&subdir).unwrap();
        assert_eq!(config.manifest.package.name, "test_project");
        assert_eq!(config.root, tmp.path());
    }

    #[test]
    fn test_build_dir() {
        let tmp = setup_project();
        let mut config = Config::load(tmp.path()).unwrap();

        assert_eq!(config.build_dir(), tmp.path().join("build/debug"));

        config.profile = Profile::Release;
        assert_eq!(config.build_dir(), tmp.path().join("build/release"));
    }

    #[test]
    fn test_deps_dir() {
        let tmp = setup_project();
        let config = Config::load(tmp.path()).unwrap();
        assert_eq!(config.deps_dir(), tmp.path().join("build/deps"));
    }

    #[test]
    fn test_src_dir() {
        let tmp = setup_project();
        let config = Config::load(tmp.path()).unwrap();
        assert_eq!(config.src_dir(), tmp.path().join("src"));
    }

    #[test]
    fn test_src_dirs_default() {
        let tmp = setup_project();
        let config = Config::load(tmp.path()).unwrap();
        let dirs = config.src_dirs();
        assert_eq!(dirs, vec![tmp.path().join("src")]);
    }

    #[test]
    fn test_src_dirs_custom() {
        let tmp = TempDir::new().unwrap();
        let toml = r#"
[package]
name = "test"
version = "0.1.0"

[build]
sources = ["Jolt/", "extra/"]
"#;
        std::fs::write(tmp.path().join("cmod.toml"), toml).unwrap();
        let config = Config::load(tmp.path()).unwrap();
        let dirs = config.src_dirs();
        assert_eq!(
            dirs,
            vec![tmp.path().join("Jolt/"), tmp.path().join("extra/")]
        );
    }

    #[test]
    fn test_exclude_patterns_default() {
        let tmp = setup_project();
        let config = Config::load(tmp.path()).unwrap();
        assert!(config.exclude_patterns().is_empty());
    }

    #[test]
    fn test_exclude_patterns_custom() {
        let tmp = TempDir::new().unwrap();
        let toml = r#"
[package]
name = "test"
version = "0.1.0"

[build]
exclude = ["*_test.cc", "test/**"]
"#;
        std::fs::write(tmp.path().join("cmod.toml"), toml).unwrap();
        let config = Config::load(tmp.path()).unwrap();
        let patterns = config.exclude_patterns();
        assert_eq!(patterns, vec!["*_test.cc", "test/**"]);
    }

    #[test]
    fn test_config_custom_cache_dir() {
        let tmp = TempDir::new().unwrap();
        let toml = r#"
[package]
name = "test"
version = "0.1.0"

[cache]
local_path = "/custom/cache"
"#;
        std::fs::write(tmp.path().join("cmod.toml"), toml).unwrap();

        let config = Config::load(tmp.path()).unwrap();
        assert_eq!(config.cache_dir(), PathBuf::from("/custom/cache"));
    }
}
