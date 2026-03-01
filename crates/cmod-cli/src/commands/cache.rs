use cmod_cache::{ArtifactCache, RemoteCache};
use cmod_core::config::Config;
use cmod_core::error::CmodError;

/// Run `cmod cache status` — show cache info.
pub fn status() -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    let cache = ArtifactCache::new(config.cache_dir());

    let size = cache.total_size()?;
    let modules = cache.list_modules()?;

    eprintln!("  Cache directory: {}", config.cache_dir().display());
    eprintln!("  Total size: {}", format_bytes(size));
    eprintln!("  Cached modules: {}", modules.len());

    for module in &modules {
        eprintln!("    - {}", module);
    }

    Ok(())
}

/// Run `cmod cache clean` — clear the local cache.
pub fn clean() -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    let cache = ArtifactCache::new(config.cache_dir());
    let size_before = cache.total_size()?;

    cache.clean()?;

    eprintln!("  Cleared {} of cached artifacts", format_bytes(size_before));

    Ok(())
}

/// Run `cmod cache push` — push local cache entries to a remote cache.
pub fn push(verbose: bool) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    let remote_url = config
        .manifest
        .cache
        .as_ref()
        .and_then(|c| c.shared_url.as_ref())
        .ok_or_else(|| CmodError::Other(
            "no shared cache URL configured; add [cache] shared_url to cmod.toml".to_string(),
        ))?;

    let remote = cmod_cache::HttpRemoteCache::new(
        remote_url,
        cmod_cache::RemoteCacheMode::ReadWrite,
    );

    let cache = ArtifactCache::new(config.cache_dir());
    let modules = cache.list_modules()?;

    eprintln!("  Pushing {} modules to remote cache...", modules.len());
    if verbose {
        eprintln!("  Remote: {}", remote_url);
    }

    let mut pushed = 0;
    for module in &modules {
        if verbose {
            eprintln!("    Pushing: {}", module);
        }
        // Walk the module's cache entries
        let module_dir = config.cache_dir().join(module);
        if module_dir.is_dir() {
            for entry in walkdir::WalkDir::new(&module_dir)
                .into_iter()
                .filter_map(|e| e.ok())
                .filter(|e| e.file_type().is_file())
            {
                let relative = entry.path().strip_prefix(&module_dir).unwrap_or(entry.path());
                let rel_str = relative.to_string_lossy().to_string();
                let parts: Vec<&str> = rel_str.split('/').collect();

                if parts.len() >= 2 {
                    let key = cmod_cache::CacheKey::from_hex(parts[0]).unwrap_or(
                        cmod_cache::CacheKey::from_hex("unknown").unwrap(),
                    );
                    let artifact_name = parts[1..].join("/");
                    let _ = remote.put(module, &key, &artifact_name, entry.path());
                    pushed += 1;
                }
            }
        }
    }

    eprintln!("  Pushed {} artifacts", pushed);
    Ok(())
}

/// Run `cmod cache pull` — pull cache entries from a remote cache.
pub fn pull(verbose: bool) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    let remote_url = config
        .manifest
        .cache
        .as_ref()
        .and_then(|c| c.shared_url.as_ref())
        .ok_or_else(|| CmodError::Other(
            "no shared cache URL configured; add [cache] shared_url to cmod.toml".to_string(),
        ))?;

    let _remote = cmod_cache::HttpRemoteCache::new(
        remote_url,
        cmod_cache::RemoteCacheMode::ReadOnly,
    );

    if verbose {
        eprintln!("  Remote: {}", remote_url);
    }

    // Pull requires knowing what keys to fetch — this needs a lockfile
    // with cache key metadata, or a manifest of remote entries.
    eprintln!("  Cache pull requires a build first to determine cache keys");
    eprintln!("  Run `cmod build` with remote cache enabled for automatic pull");

    Ok(())
}

fn format_bytes(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;

    if bytes >= GB {
        format!("{:.2} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.2} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.2} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}
