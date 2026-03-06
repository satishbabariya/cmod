use cmod_cache::{ArtifactCache, RemoteCache};
use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::shell::{format_bytes, Shell};

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
pub fn status(shell: &Shell) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    let cache = ArtifactCache::new(config.cache_dir());

    let size = cache.total_size()?;
    let modules = cache.list_modules()?;

    shell.status(
        "Cache",
        format!("directory: {}", config.cache_dir().display()),
    );
    shell.status("Size", format_bytes(size));
    shell.status("Modules", format!("{} cached", modules.len()));

    for module in &modules {
        shell.verbose("Module", module);
    }

    Ok(())
}

/// Run `cmod cache clean` — clear the local cache.
pub fn clean(shell: &Shell) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    let cache = ArtifactCache::new(config.cache_dir());
    let size_before = cache.total_size()?;

    cache.clean()?;

    shell.status(
        "Cleaned",
        format!("{} of cached artifacts", format_bytes(size_before)),
    );

    Ok(())
}

/// Run `cmod cache push` — push local cache entries to a remote cache.
pub fn push(shell: &Shell) -> Result<(), CmodError> {
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

    shell.status(
        "Pushing",
        format!("{} modules to remote cache...", modules.len()),
    );
    shell.verbose("Remote", remote_url);

    let mut pushed = 0;
    for module in &modules {
        shell.verbose("Pushing", module);
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

    shell.status("Pushed", format!("{} artifacts", pushed));
    Ok(())
}

/// Run `cmod cache pull` — pull cache entries from a remote cache.
pub fn pull(shell: &Shell) -> Result<(), CmodError> {
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

    shell.verbose("Remote", remote_url);

    // Load the lockfile to get module names and hashes
    let lockfile = cmod_core::lockfile::Lockfile::load(&config.lockfile_path)
        .map_err(|_| CmodError::Other("no lockfile found; run `cmod resolve` first".to_string()))?;

    if lockfile.packages.is_empty() {
        shell.status("Pulled", "no dependencies in lockfile");
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

        let key = match cmod_cache::CacheKey::from_hex(hash) {
            Some(k) => k,
            None => continue,
        };

        if cache.has(&pkg.name, &key) {
            shell.verbose("Cached", format!("{} — already cached", pkg.name));
            skipped += 1;
            continue;
        }

        match remote.has(&pkg.name, &key) {
            Ok(true) => {
                shell.verbose("Pulling", format!("{} from remote...", pkg.name));

                let dest_dir = cache.entry_dir(&pkg.name, &key);
                std::fs::create_dir_all(&dest_dir)?;

                for artifact in &["module.pcm", "object.o", "metadata.json"] {
                    let dest = dest_dir.join(artifact);
                    match remote.get(&pkg.name, &key, artifact, &dest) {
                        Ok(true) => {
                            shell.verbose("Downloaded", format!("{}/{}", pkg.name, artifact));
                        }
                        Ok(false) => {}
                        Err(e) => {
                            shell.verbose("Failed", format!("{}/{}: {}", pkg.name, artifact, e));
                        }
                    }
                }
                pulled += 1;
            }
            Ok(false) => {
                shell.verbose("Missing", format!("{} — not in remote cache", pkg.name));
            }
            Err(e) => {
                shell.verbose(
                    "Error",
                    format!("{} — remote check failed: {}", pkg.name, e),
                );
            }
        }
    }

    shell.status(
        "Pulled",
        format!("{} entries ({} already cached)", pulled, skipped),
    );
    Ok(())
}

/// Run `cmod cache gc` — garbage collect old/oversized cache entries.
pub fn gc(shell: &Shell) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    let cache = ArtifactCache::new(config.cache_dir());
    let size_before = cache.total_size()?;

    let max_age = config
        .manifest
        .cache
        .as_ref()
        .and_then(|c| c.ttl.as_deref())
        .and_then(cmod_cache::parse_ttl);

    let max_bytes = config
        .manifest
        .cache
        .as_ref()
        .and_then(|c| c.max_size.as_deref())
        .and_then(parse_size);

    if max_age.is_none() && max_bytes.is_none() {
        shell.warn("no TTL or max_size configured in [cache]; nothing to evict");
        shell.note("set [cache] ttl = \"7d\" or max_size = \"500M\" in cmod.toml");
        return Ok(());
    }

    if let Some(ref age) = max_age {
        shell.verbose("TTL", format!("{:?}", age));
    }
    if let Some(bytes) = max_bytes {
        shell.verbose("Max size", format_bytes(bytes));
    }

    let result = cache.auto_evict(max_age, max_bytes)?;

    let size_after = cache.total_size()?;
    shell.status(
        "GC",
        format!(
            "{} entries removed, {} freed ({} -> {})",
            result.entries_removed,
            format_bytes(result.bytes_freed),
            format_bytes(size_before),
            format_bytes(size_after),
        ),
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
pub fn export_bmi(module: &str, key: &str, output: &str, shell: &Shell) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    let output_path = std::path::PathBuf::from(output);
    let mut package = cmod_cache::export_bmi(&config.cache_dir(), module, key, &output_path)?;

    // Sign the BMI package if signing key is configured
    let signing_config = config.manifest.security.as_ref().and_then(|sec| {
        cmod_security::signing::resolve_signing_config(
            sec.signing_key.as_deref(),
            sec.signing_backend.as_deref(),
        )
    });

    if let Some(ref cfg) = signing_config {
        shell.verbose("Signing", "BMI package will be signed");
        let package_json = serde_json::to_string(&package)
            .map_err(|e| CmodError::Other(format!("failed to serialize for signing: {}", e)))?;

        match cmod_security::signing::sign_data(cfg, package_json.as_bytes()) {
            Ok(result) => {
                package.metadata.signature = Some(result.signature.clone());

                // Write .sig file
                let sig_path = output_path.join("bmi_package.sig");
                std::fs::write(&sig_path, &result.signature)?;

                // Re-write package JSON with signature
                let updated_json = serde_json::to_string_pretty(&package).map_err(|e| {
                    CmodError::Other(format!("failed to serialize BMI package: {}", e))
                })?;
                std::fs::write(output_path.join("bmi_package.json"), &updated_json)?;

                shell.verbose(
                    "Signed",
                    format!("by {} ({})", result.signer, result.backend.as_str()),
                );
            }
            Err(e) => {
                shell.warn(format!("failed to sign BMI package: {}", e));
            }
        }
    }

    shell.status(
        "Exported",
        format!("BMI package for '{}' to {}", module, output),
    );
    shell.verbose("Package", format!("{} file(s)", package.files.len()));
    shell.verbose("Compat", package.metadata.compat_key());
    for (name, hash) in &package.files {
        shell.verbose(
            "File",
            format!("{} ({})", name, &hash[..12.min(hash.len())]),
        );
    }

    Ok(())
}

/// Run `cmod cache import` — import a BMI package into the local cache.
pub fn import_bmi(path: &str, shell: &Shell) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    let package_path = std::path::PathBuf::from(path);

    // Verify signature if present
    let sig_path = package_path.join("bmi_package.sig");
    let pkg_json_path = package_path.join("bmi_package.json");

    if sig_path.exists() && pkg_json_path.exists() {
        let signing_config = config.manifest.security.as_ref().and_then(|sec| {
            cmod_security::signing::resolve_signing_config(
                sec.signing_key.as_deref(),
                sec.signing_backend.as_deref(),
            )
        });

        match cmod_security::signing::verify_file(
            &pkg_json_path,
            &sig_path,
            signing_config.as_ref(),
        ) {
            Ok(cmod_security::signing::VerifyStatus::Valid { signer, backend }) => {
                shell.verbose(
                    "Signature",
                    format!("verified ({}, signed by {})", backend.as_str(), signer),
                );
            }
            Ok(cmod_security::signing::VerifyStatus::Untrusted { signer, reason, .. }) => {
                shell.warn(format!(
                    "BMI package signature untrusted (signer: {}): {}",
                    signer, reason,
                ));
            }
            Ok(cmod_security::signing::VerifyStatus::Unsigned) => {
                shell.verbose("Signature", "no signature present");
            }
            Ok(cmod_security::signing::VerifyStatus::Invalid { reason }) => {
                return Err(CmodError::SecurityViolation {
                    reason: format!("BMI package has invalid signature: {}", reason),
                });
            }
            Err(e) => {
                shell.warn(format!("could not verify BMI signature: {}", e));
            }
        }
    } else {
        shell.verbose("Signature", "no signature file present");
    }

    let metadata = cmod_cache::import_bmi(&config.cache_dir(), &package_path)?;

    shell.status(
        "Imported",
        format!(
            "BMI for '{}' v{} ({})",
            metadata.module_name,
            metadata.version,
            metadata.compat_key()
        ),
    );
    shell.verbose(
        "Compiler",
        format!("{} {}", metadata.compiler, metadata.compiler_version),
    );
    shell.verbose("Target", &metadata.target);
    shell.verbose("Standard", format!("C++{}", metadata.cxx_standard));

    Ok(())
}
