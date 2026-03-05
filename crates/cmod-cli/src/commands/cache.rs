use cmod_cache::{ArtifactCache, RemoteCache};
use cmod_core::config::Config;
use cmod_core::error::CmodError;

/// Run `cmod cache status` — show cache info (human-readable or JSON).
pub fn status_json() -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;
    let cache = ArtifactCache::new(config.cache_dir());
    let status = cache.status_json()?;
    let json = serde_json::to_string_pretty(&status)
        .map_err(|e| CmodError::Other(format!("failed to serialize cache status: {}", e)))?;
    println!("{}", json);
    Ok(())
}

/// Run `cmod cache inspect <key>` — show metadata for a specific cache entry.
pub fn inspect(module: &str, key: &str) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;
    let cache = ArtifactCache::new(config.cache_dir());

    let cache_key = cmod_cache::CacheKey::from_hex(key)
        .ok_or_else(|| CmodError::Other(format!("invalid cache key: {}", key)))?;

    let info = cache.inspect(module, &cache_key)?;
    let json = serde_json::to_string_pretty(&info)
        .map_err(|e| CmodError::Other(format!("failed to serialize entry info: {}", e)))?;
    println!("{}", json);
    Ok(())
}

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

    eprintln!(
        "  Cleared {} of cached artifacts",
        format_bytes(size_before)
    );

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
        .ok_or_else(|| {
            CmodError::Other(
                "no shared cache URL configured; add [cache] shared_url to cmod.toml".to_string(),
            )
        })?;

    let remote =
        cmod_cache::HttpRemoteCache::new(remote_url, cmod_cache::RemoteCacheMode::ReadWrite);

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
                let relative = entry
                    .path()
                    .strip_prefix(&module_dir)
                    .unwrap_or(entry.path());
                let rel_str = relative.to_string_lossy().to_string();
                let parts: Vec<&str> = rel_str.split('/').collect();

                if parts.len() >= 2 {
                    let key = cmod_cache::CacheKey::from_hex(parts[0])
                        .unwrap_or(cmod_cache::CacheKey::from_hex("unknown").unwrap());
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
///
/// Uses the lockfile to determine module names and content hashes,
/// then constructs cache keys and fetches available artifacts.
pub fn pull(verbose: bool) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    let remote_url = config
        .manifest
        .cache
        .as_ref()
        .and_then(|c| c.shared_url.as_ref())
        .ok_or_else(|| {
            CmodError::Other(
                "no shared cache URL configured; add [cache] shared_url to cmod.toml".to_string(),
            )
        })?;

    let remote =
        cmod_cache::HttpRemoteCache::new(remote_url, cmod_cache::RemoteCacheMode::ReadOnly);

    if verbose {
        eprintln!("  Remote: {}", remote_url);
    }

    // Load the lockfile to get module names and hashes
    let lockfile = cmod_core::lockfile::Lockfile::load(&config.lockfile_path)
        .map_err(|_| CmodError::Other("no lockfile found; run `cmod resolve` first".to_string()))?;

    if lockfile.packages.is_empty() {
        eprintln!("  No dependencies in lockfile, nothing to pull.");
        return Ok(());
    }

    let cache = ArtifactCache::new(config.cache_dir());
    let mut pulled = 0u32;
    let mut skipped = 0u32;

    for pkg in &lockfile.packages {
        let hash = pkg.hash.as_deref().unwrap_or("");
        if hash.is_empty() {
            continue;
        }

        // Use content hash as cache key lookup
        let key = match cmod_cache::CacheKey::from_hex(hash) {
            Some(k) => k,
            None => continue,
        };

        // Check if already cached locally
        if cache.has(&pkg.name, &key) {
            if verbose {
                eprintln!("    {} — already cached", pkg.name);
            }
            skipped += 1;
            continue;
        }

        // Check remote availability and pull
        match remote.has(&pkg.name, &key) {
            Ok(true) => {
                if verbose {
                    eprintln!("    {} — pulling from remote...", pkg.name);
                }

                let dest_dir = cache.entry_dir(&pkg.name, &key);
                std::fs::create_dir_all(&dest_dir)?;

                for artifact in &["module.pcm", "object.o", "metadata.json"] {
                    let dest = dest_dir.join(artifact);
                    match remote.get(&pkg.name, &key, artifact, &dest) {
                        Ok(true) => {
                            if verbose {
                                eprintln!("      {} — downloaded", artifact);
                            }
                        }
                        Ok(false) => {} // Not available, skip
                        Err(e) => {
                            if verbose {
                                eprintln!("      {} — failed: {}", artifact, e);
                            }
                        }
                    }
                }
                pulled += 1;
            }
            Ok(false) => {
                if verbose {
                    eprintln!("    {} — not in remote cache", pkg.name);
                }
            }
            Err(e) => {
                if verbose {
                    eprintln!("    {} — remote check failed: {}", pkg.name, e);
                }
            }
        }
    }

    eprintln!("  Pulled {} entries ({} already cached)", pulled, skipped);
    Ok(())
}

/// Run `cmod cache gc` — garbage collect old/oversized cache entries.
pub fn gc(verbose: bool) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    let cache = ArtifactCache::new(config.cache_dir());
    let size_before = cache.total_size()?;

    // Parse TTL from manifest [cache].ttl
    let max_age = config
        .manifest
        .cache
        .as_ref()
        .and_then(|c| c.ttl.as_deref())
        .and_then(cmod_cache::parse_ttl);

    // Parse max_size from manifest [cache].max_size
    let max_bytes = config
        .manifest
        .cache
        .as_ref()
        .and_then(|c| c.max_size.as_deref())
        .and_then(parse_size);

    if max_age.is_none() && max_bytes.is_none() {
        eprintln!("  No TTL or max_size configured in [cache]; nothing to evict.");
        eprintln!("  Set [cache] ttl = \"7d\" or max_size = \"500M\" in cmod.toml.");
        return Ok(());
    }

    if verbose {
        if let Some(ref age) = max_age {
            eprintln!("  TTL: {:?}", age);
        }
        if let Some(bytes) = max_bytes {
            eprintln!("  Max size: {}", format_bytes(bytes));
        }
    }

    let result = cache.auto_evict(max_age, max_bytes)?;

    let size_after = cache.total_size()?;
    eprintln!(
        "  GC complete: {} entries removed, {} freed ({} → {})",
        result.entries_removed,
        format_bytes(result.bytes_freed),
        format_bytes(size_before),
        format_bytes(size_after),
    );

    Ok(())
}

/// Parse a human-readable size string like "1G", "500M", "100K" into bytes.
fn parse_size(s: &str) -> Option<u64> {
    let s = s.trim();
    if s.is_empty() {
        return None;
    }

    let (num_str, multiplier) = if s.ends_with('G') || s.ends_with('g') {
        (&s[..s.len() - 1], 1024 * 1024 * 1024u64)
    } else if s.ends_with('M') || s.ends_with('m') {
        (&s[..s.len() - 1], 1024 * 1024u64)
    } else if s.ends_with('K') || s.ends_with('k') {
        (&s[..s.len() - 1], 1024u64)
    } else {
        (s, 1u64)
    };

    let num: u64 = num_str.parse().ok()?;
    Some(num * multiplier)
}

/// Run `cmod cache export` — export a cached module as a BMI package.
pub fn export_bmi(module: &str, key: &str, output: &str, verbose: bool) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    let output_path = std::path::PathBuf::from(output);
    let package = cmod_cache::export_bmi(&config.cache_dir(), module, key, &output_path)?;

    eprintln!("  Exported BMI package for '{}' to {}", module, output);
    eprintln!("  {} file(s) in package", package.files.len());

    if verbose {
        eprintln!("  Compatibility key: {}", package.metadata.compat_key());
        for (name, hash) in &package.files {
            eprintln!("    {} ({})", name, &hash[..12.min(hash.len())]);
        }
    }

    Ok(())
}

/// Run `cmod cache import` — import a BMI package into the local cache.
pub fn import_bmi(path: &str, verbose: bool) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    let package_path = std::path::PathBuf::from(path);
    let metadata = cmod_cache::import_bmi(&config.cache_dir(), &package_path)?;

    eprintln!(
        "  Imported BMI for '{}' v{} ({})",
        metadata.module_name,
        metadata.version,
        metadata.compat_key()
    );

    if verbose {
        eprintln!(
            "  Compiler: {} {}",
            metadata.compiler, metadata.compiler_version
        );
        eprintln!("  Target: {}", metadata.target);
        eprintln!("  C++ standard: {}", metadata.cxx_standard);
    }

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
