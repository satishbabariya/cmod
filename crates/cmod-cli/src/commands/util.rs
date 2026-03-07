use cmod_core::config::Config;
use cmod_core::error::CmodError;

/// Create a `Config` scoped to a single workspace member.
pub fn create_member_config(
    parent_config: &Config,
    member: &cmod_workspace::workspace::WorkspaceMember,
) -> Result<Config, CmodError> {
    let manifest_path = member.path.join("cmod.toml");
    let manifest = if manifest_path.exists() {
        cmod_core::manifest::Manifest::load(&manifest_path)?
    } else {
        member.manifest.clone()
    };

    Ok(Config {
        root: member.path.clone(),
        manifest_path: manifest_path.clone(),
        manifest,
        lockfile_path: parent_config.lockfile_path.clone(),
        profile: parent_config.profile,
        target: parent_config.target.clone(),
        locked: parent_config.locked,
        offline: parent_config.offline,
        verbosity: parent_config.verbosity,
        enabled_features: parent_config.enabled_features.clone(),
        no_default_features: parent_config.no_default_features,
        no_cache: parent_config.no_cache,
    })
}
