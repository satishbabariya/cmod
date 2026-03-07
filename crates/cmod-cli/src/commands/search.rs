use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::shell::Shell;

/// Run `cmod search` — search for modules by name pattern.
pub fn run(query: &str, local_only: bool, offline: bool, shell: &Shell) -> Result<(), CmodError> {
    let cwd = std::env::current_dir()?;
    let config = Config::load(&cwd)?;

    let pattern = query.to_lowercase();
    let mut found = Vec::new();

    // Search in manifest dependencies
    for (name, dep) in &config.manifest.dependencies {
        if matches_pattern(name, &pattern) {
            let version = dep.version_req().unwrap_or("*");
            found.push(SearchResult {
                name: name.clone(),
                version: version.to_string(),
                source: "dependency".to_string(),
                description: None,
                repository: None,
            });
        }
    }

    // Search in dev-dependencies
    for (name, dep) in &config.manifest.dev_dependencies {
        if matches_pattern(name, &pattern) {
            let version = dep.version_req().unwrap_or("*");
            found.push(SearchResult {
                name: name.clone(),
                version: version.to_string(),
                source: "dev-dependency".to_string(),
                description: None,
                repository: None,
            });
        }
    }

    // Search in lockfile for transitive deps
    if config.lockfile_path.exists() {
        if let Ok(lockfile) = cmod_core::lockfile::Lockfile::load(&config.lockfile_path) {
            for pkg in &lockfile.packages {
                if matches_pattern(&pkg.name, &pattern) && !found.iter().any(|r| r.name == pkg.name)
                {
                    found.push(SearchResult {
                        name: pkg.name.clone(),
                        version: pkg.version.clone(),
                        source: "lockfile".to_string(),
                        description: None,
                        repository: pkg.repo.clone(),
                    });
                }
            }
        }
    }

    // Search in workspace members
    if config.manifest.is_workspace() {
        if let Ok(ws) = cmod_workspace::WorkspaceManager::load(&config.root) {
            for member in &ws.members {
                if matches_pattern(&member.name, &pattern) {
                    found.push(SearchResult {
                        name: member.name.clone(),
                        version: member.manifest.package.version.clone(),
                        source: "workspace".to_string(),
                        description: member.manifest.package.description.clone(),
                        repository: None,
                    });
                }
            }
        }
    }

    // Search remote registry (unless --local-only or --offline)
    if !local_only && !offline {
        search_registry(query, &mut found, shell);
    } else if !local_only && offline {
        // Try cached registry in offline mode
        search_cached_registry(query, &mut found, shell);
    }

    // Rank results by relevance: exact name match > name contains > description/keyword match
    found.sort_by(|a, b| {
        let score_a = search_relevance_score(&a.name, a.description.as_deref(), &pattern);
        let score_b = search_relevance_score(&b.name, b.description.as_deref(), &pattern);
        score_b.cmp(&score_a)
    });

    if found.is_empty() {
        shell.status("Search", format!("no modules matching '{}'", query));
        shell.verbose(
            "Hint",
            "cmod search queries local dependencies, lockfile, and registry",
        );
    } else {
        shell.status(
            "Found",
            format!("{} result(s) for '{}'", found.len(), query),
        );
        for result in &found {
            let desc = result
                .description
                .as_deref()
                .map(|d| format!(" — {}", d))
                .unwrap_or_default();
            let repo = result
                .repository
                .as_deref()
                .map(|r| format!(" [{}]", r))
                .unwrap_or_default();
            shell.status(
                "Match",
                format!(
                    "{} v{} ({}){}{}",
                    result.name, result.version, result.source, desc, repo,
                ),
            );
        }
    }

    Ok(())
}

struct SearchResult {
    name: String,
    version: String,
    source: String,
    description: Option<String>,
    repository: Option<String>,
}

/// Search the remote registry index.
fn search_registry(query: &str, found: &mut Vec<SearchResult>, shell: &Shell) {
    let cache_dir = dirs::cache_dir()
        .unwrap_or_else(|| std::path::PathBuf::from(".cache"))
        .join("cmod");
    let client =
        cmod_resolver::RegistryClient::new(cmod_resolver::RegistryClient::default_url(), cache_dir);

    match client.update() {
        Ok(index) => {
            add_registry_results(&index, query, found);
        }
        Err(e) => {
            shell.verbose("Registry", format!("could not fetch registry: {}", e));
            // Fall back to cached index
            if let Ok(Some(index)) = client.cached_index() {
                shell.verbose("Registry", "using cached registry index");
                add_registry_results(&index, query, found);
            }
        }
    }
}

/// Search the cached registry index (offline mode).
fn search_cached_registry(query: &str, found: &mut Vec<SearchResult>, shell: &Shell) {
    let cache_dir = dirs::cache_dir()
        .unwrap_or_else(|| std::path::PathBuf::from(".cache"))
        .join("cmod");
    let client =
        cmod_resolver::RegistryClient::new(cmod_resolver::RegistryClient::default_url(), cache_dir);

    match client.cached_index() {
        Ok(Some(index)) => {
            shell.verbose("Registry", "searching cached registry index");
            add_registry_results(&index, query, found);
        }
        Ok(None) => {
            shell.verbose("Registry", "no cached registry index available");
        }
        Err(e) => {
            shell.verbose("Registry", format!("failed to read cached index: {}", e));
        }
    }
}

/// Add matching results from a registry index.
fn add_registry_results(
    index: &cmod_resolver::RegistryIndex,
    query: &str,
    found: &mut Vec<SearchResult>,
) {
    let results = index.search(query);
    for entry in results {
        // Skip if already found from local sources
        if found.iter().any(|r| r.name == entry.name) {
            continue;
        }
        let version = index
            .latest_version(&entry.name)
            .map(|v| v.version.clone())
            .unwrap_or_else(|| "unknown".to_string());
        found.push(SearchResult {
            name: entry.name.clone(),
            version,
            source: "registry".to_string(),
            description: entry.description.clone(),
            repository: Some(entry.repository.clone()),
        });
    }
}

/// Check if a module name matches the search pattern (case-insensitive substring).
fn matches_pattern(name: &str, pattern: &str) -> bool {
    name.to_lowercase().contains(pattern)
}

/// Score a search result for relevance ranking (higher is more relevant).
fn search_relevance_score(name: &str, description: Option<&str>, pattern: &str) -> u32 {
    let name_lower = name.to_lowercase();
    if name_lower == pattern {
        // Exact name match
        100
    } else if name_lower.ends_with(&format!(".{}", pattern)) {
        // Name ends with the query (e.g., "github.fmtlib.fmt" for query "fmt")
        80
    } else if name_lower.contains(pattern) {
        // Name contains the query
        60
    } else if description
        .map(|d| d.to_lowercase().contains(pattern))
        .unwrap_or(false)
    {
        // Description matches
        40
    } else {
        // Keyword/category match (came from registry search)
        20
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_matches_pattern_exact() {
        assert!(matches_pattern("github.com/fmtlib/fmt", "fmt"));
    }

    #[test]
    fn test_matches_pattern_case_insensitive() {
        assert!(matches_pattern("MyModule", "mymod"));
    }

    #[test]
    fn test_matches_pattern_partial() {
        assert!(matches_pattern("github.com/nlohmann/json", "json"));
    }

    #[test]
    fn test_matches_pattern_no_match() {
        assert!(!matches_pattern("github.com/fmtlib/fmt", "boost"));
    }

    #[test]
    fn test_matches_pattern_empty() {
        assert!(matches_pattern("anything", ""));
    }

    #[test]
    fn test_search_relevance_exact_match_highest() {
        let exact = search_relevance_score("fmt", None, "fmt");
        let contains = search_relevance_score("github.fmtlib.fmt", None, "fmt");
        let desc_only = search_relevance_score("other", Some("a fmt library"), "fmt");
        assert!(exact > contains);
        assert!(contains > desc_only);
    }

    #[test]
    fn test_search_relevance_suffix_over_substring() {
        let suffix = search_relevance_score("github.fmtlib.fmt", None, "fmt");
        let substring = search_relevance_score("fmtlib.formatting", None, "fmt");
        assert!(suffix > substring);
    }

    #[test]
    fn test_add_registry_results_dedup() {
        let mut index = cmod_resolver::RegistryIndex::new("test", "");
        index.upsert_module(cmod_resolver::registry::RegistryEntry {
            name: "fmt".into(),
            description: Some("Format lib".into()),
            repository: "https://github.com/fmtlib/fmt".into(),
            versions: vec![cmod_resolver::registry::RegistryVersion {
                version: "10.2.0".into(),
                tag: "v10.2.0".into(),
                commit: "abc".into(),
                min_cpp_standard: None,
                published_at: "".into(),
                yanked: false,
            }],
            keywords: vec![],
            category: None,
            license: None,
            authors: vec![],
            updated_at: "".into(),
            verified: false,
            deprecated: None,
        });

        let mut found = vec![SearchResult {
            name: "fmt".into(),
            version: "10.2.0".into(),
            source: "dependency".into(),
            description: None,
            repository: None,
        }];

        add_registry_results(&index, "fmt", &mut found);
        // Should not duplicate
        assert_eq!(found.len(), 1);
    }
}
