use cmod_cache::ArtifactCache;
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
