use cmod_core::config::Config;
use cmod_core::error::CmodError;
use cmod_core::shell::Shell;

/// Run `cmod search` — search for modules by name pattern.
pub fn run(query: &str, shell: &Shell) -> Result<(), CmodError> {
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
                    });
                }
            }
        }
    }

    if found.is_empty() {
        shell.status("Search", format!("no modules matching '{}'", query));
        shell.verbose(
            "Hint",
            "cmod search queries local dependencies and lockfile",
        );
        shell.verbose(
            "Hint",
            "for Git-based discovery, use `git ls-remote` or browse the repo",
        );
    } else {
        shell.status(
            "Found",
            format!("{} result(s) for '{}'", found.len(), query),
        );
        for result in &found {
            shell.status(
                "Match",
                format!("{} v{} ({})", result.name, result.version, result.source),
            );
        }
    }

    Ok(())
}

struct SearchResult {
    name: String,
    version: String,
    source: String,
}

/// Check if a module name matches the search pattern (case-insensitive substring).
fn matches_pattern(name: &str, pattern: &str) -> bool {
    name.to_lowercase().contains(pattern)
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
}
