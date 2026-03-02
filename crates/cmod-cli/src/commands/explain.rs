use cmod_build::graph::ModuleNode;
use cmod_build::runner;
use cmod_cache::key::{hash_file, CacheKey, CacheKeyInputs};
use cmod_cache::ArtifactCache;
use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::types::Profile;

/// Run `cmod explain <module>` — explain why a module would be rebuilt.
pub fn run(module_name: String, verbose: bool) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    let src_dir = config.src_dir();
    let sources = runner::discover_sources(&src_dir)?;

    // Find the module
    let mut found_node: Option<ModuleNode> = None;
    for source in &sources {
        if let Ok(Some(name)) = runner::extract_module_name(source) {
            if name == module_name {
                let kind = runner::classify_source(source)?;
                found_node = Some(ModuleNode {
                    id: name.clone(),
                    name: name.clone(),
                    kind,
                    source: source.clone(),
                    package: config.manifest.package.name.clone(),
                    imports: vec![],
                    partition_of: None,
                });
                break;
            }
        }
        // Also check by filename stem
        if let Some(stem) = source.file_stem().and_then(|s| s.to_str()) {
            if stem == module_name {
                let kind = runner::classify_source(source)?;
                found_node = Some(ModuleNode {
                    id: module_name.clone(),
                    name: module_name.clone(),
                    kind,
                    source: source.clone(),
                    package: config.manifest.package.name.clone(),
                    imports: vec![],
                    partition_of: None,
                });
                break;
            }
        }
    }

    let node = found_node.ok_or_else(|| {
        CmodError::Other(format!("module '{}' not found in source tree", module_name))
    })?;

    println!("Module: {}", node.name);
    println!("Source: {}", node.source.display());
    println!("Kind:   {:?}", node.kind);
    println!();

    // Check rebuild reasons
    let mut reasons = Vec::new();

    // 1. Check if source file has changed (via cache key comparison)
    let cache = ArtifactCache::new(config.cache_dir());
    let source_hash = hash_file(&node.source).unwrap_or_default();

    if verbose {
        println!(
            "  Source hash: {}",
            &source_hash[..16.min(source_hash.len())]
        );
    }

    let cxx_standard = config
        .manifest
        .toolchain
        .as_ref()
        .and_then(|tc| tc.cxx_standard.clone())
        .unwrap_or_else(|| "20".to_string());

    let target = config
        .target
        .clone()
        .or_else(|| {
            config
                .manifest
                .toolchain
                .as_ref()
                .and_then(|tc| tc.target.clone())
        })
        .unwrap_or_else(|| "unknown".to_string());

    let inputs = CacheKeyInputs {
        source_hash: source_hash.clone(),
        dependency_hashes: vec![],
        compiler: "clang".to_string(),
        compiler_version: String::new(),
        cxx_standard,
        stdlib: String::new(),
        target: target.clone(),
        flags: vec![],
    };

    let key = CacheKey::compute(&inputs);

    if !cache.has(&node.name, &key) {
        reasons.push("cache miss — no cached artifact matches current inputs".to_string());
    } else {
        println!("  Cache: HIT — cached artifact exists for current inputs");
    }

    // 2. Check if build output exists
    let build_dir = config.build_dir();
    let profile_name = match config.profile {
        Profile::Debug => "debug",
        Profile::Release => "release",
    };
    let obj_path = build_dir.join("obj").join(format!("{}.o", node.name));
    let pcm_path = build_dir.join("pcm").join(format!("{}.pcm", node.name));

    if !obj_path.exists() {
        reasons.push(format!("object file missing: {}", obj_path.display()));
    }

    if matches!(
        node.kind,
        cmod_core::types::ModuleUnitKind::InterfaceUnit
            | cmod_core::types::ModuleUnitKind::PartitionUnit
    ) && !pcm_path.exists()
    {
        reasons.push(format!("PCM file missing: {}", pcm_path.display()));
    }

    // 3. Check if source is newer than output
    if obj_path.exists() {
        if let (Ok(src_meta), Ok(obj_meta)) = (
            std::fs::metadata(&node.source),
            std::fs::metadata(&obj_path),
        ) {
            if let (Ok(src_time), Ok(obj_time)) = (src_meta.modified(), obj_meta.modified()) {
                if src_time > obj_time {
                    reasons.push("source is newer than object file".to_string());
                }
            }
        }
    }

    // Print reasons
    if reasons.is_empty() {
        println!("  Status: UP TO DATE — no rebuild needed");
        println!("  Profile: {}", profile_name);
    } else {
        println!("  Status: NEEDS REBUILD");
        println!("  Profile: {}", profile_name);
        println!("  Reasons:");
        for (i, reason) in reasons.iter().enumerate() {
            println!("    {}. {}", i + 1, reason);
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_explain_module_not_found() {
        // In a temp dir with no sources, explain should fail gracefully
        let tmp = tempfile::TempDir::new().unwrap();
        let toml = "[package]\nname = \"test\"\nversion = \"0.1.0\"\n";
        std::fs::write(tmp.path().join("cmod.toml"), toml).unwrap();
        std::fs::create_dir_all(tmp.path().join("src")).unwrap();

        // Set current dir for the test
        let original = std::env::current_dir().unwrap();
        std::env::set_current_dir(tmp.path()).unwrap();

        let result = run("nonexistent_module".to_string(), false);
        assert!(result.is_err());

        std::env::set_current_dir(original).unwrap();
    }
}
