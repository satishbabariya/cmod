use std::path::{Path, PathBuf};

use crate::error::CmodError;
use crate::manifest::Manifest;
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

    /// Verbose logging.
    pub verbose: bool,
}

impl Config {
    /// Load configuration from the current working directory (or a specified path).
    pub fn load(start_dir: &Path) -> Result<Self, CmodError> {
        let manifest_path =
            Manifest::find(start_dir).ok_or_else(|| CmodError::ManifestNotFound {
                path: start_dir.join("cmod.toml").display().to_string(),
            })?;

        let root = manifest_path
            .parent()
            .unwrap_or(start_dir)
            .to_path_buf();

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
            verbose: false,
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

    /// Source directory.
    pub fn src_dir(&self) -> PathBuf {
        self.root.join("src")
    }
}
