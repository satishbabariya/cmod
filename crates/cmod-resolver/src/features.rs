use std::collections::{BTreeMap, BTreeSet};

use cmod_core::error::CmodError;
use cmod_core::manifest::{Dependency, Manifest};

/// Resolved feature set for a package.
#[derive(Debug, Clone, Default)]
pub struct ResolvedFeatures {
    /// Features enabled for each dependency (keyed by dep name).
    pub dep_features: BTreeMap<String, BTreeSet<String>>,
    /// Optional dependencies that are activated.
    pub activated_optional_deps: BTreeSet<String>,
}

/// Resolve features for a manifest and its dependencies.
///
/// Features can be:
/// - Direct features defined in `[features]`
/// - Feature values that reference optional dependencies (e.g., `dep:my_opt_dep`)
/// - Features that enable other features
///
/// Returns the set of enabled features for each dependency.
pub fn resolve_features(
    manifest: &Manifest,
    requested_features: &[String],
    no_default_features: bool,
) -> Result<ResolvedFeatures, CmodError> {
    let mut result = ResolvedFeatures::default();
    let mut enabled = BTreeSet::new();

    // Start with default features unless disabled
    if !no_default_features {
        if let Some(defaults) = manifest.features.get("default") {
            for f in defaults {
                enabled.insert(f.clone());
            }
        }
    }

    // Add explicitly requested features
    for f in requested_features {
        enabled.insert(f.clone());
    }

    // Recursively resolve feature dependencies
    let mut to_process: Vec<String> = enabled.iter().cloned().collect();
    let mut processed = BTreeSet::new();

    while let Some(feature) = to_process.pop() {
        if !processed.insert(feature.clone()) {
            continue;
        }

        // Check if this feature is defined in the manifest
        if let Some(sub_features) = manifest.features.get(&feature) {
            for sub in sub_features {
                if sub.starts_with("dep:") {
                    // Optional dependency activation
                    let dep_name = sub.trim_start_matches("dep:");
                    result.activated_optional_deps.insert(dep_name.to_string());
                } else if sub.contains('/') {
                    // dep_name/feature_name — enable a feature on a dependency
                    let parts: Vec<&str> = sub.splitn(2, '/').collect();
                    if parts.len() == 2 {
                        result
                            .dep_features
                            .entry(parts[0].to_string())
                            .or_default()
                            .insert(parts[1].to_string());
                    }
                } else {
                    // Another feature to enable
                    to_process.push(sub.clone());
                }
            }
        }

        // Check if feature name matches an optional dependency
        if let Some(dep) = manifest.dependencies.get(&feature) {
            if matches!(dep, Dependency::Detailed(d) if d.optional) {
                result.activated_optional_deps.insert(feature.clone());
            }
        }
    }

    // Collect features specified directly on dependency declarations
    for (name, dep) in &manifest.dependencies {
        if let Dependency::Detailed(d) = dep {
            if !d.features.is_empty() {
                let entry = result.dep_features.entry(name.clone()).or_default();
                for f in &d.features {
                    entry.insert(f.clone());
                }
            }
        }
    }

    Ok(result)
}

/// Detect cycles in the feature dependency graph.
///
/// Returns an error if any feature chain forms a cycle (e.g., A enables B enables A).
/// The resolution algorithm itself handles cycles gracefully via the visited set,
/// but this function provides an explicit diagnostic for `cmod check`.
pub fn detect_feature_cycles(manifest: &Manifest) -> Result<(), CmodError> {
    // Use a proper DFS with a recursion stack to detect true back-edges.
    // A single "visited" set would falsely flag DAGs with shared sub-features
    // (diamond pattern) as cyclic.
    let mut processed = BTreeSet::new();

    for feature_name in manifest.features.keys() {
        if processed.contains(feature_name) {
            continue;
        }

        let mut in_stack = BTreeSet::new();
        // Stack holds (node, children_pushed) to simulate recursive DFS.
        let mut stack: Vec<(String, bool)> = vec![(feature_name.clone(), false)];

        while let Some((current, children_pushed)) = stack.last_mut() {
            if !*children_pushed {
                if in_stack.contains(current.as_str()) {
                    return Err(CmodError::Other(format!(
                        "cycle detected in feature graph: '{}' is reachable from itself (via '{}')",
                        current, feature_name
                    )));
                }
                if processed.contains(current.as_str()) {
                    stack.pop();
                    continue;
                }
                in_stack.insert(current.clone());
                *children_pushed = true;

                // Push children
                let current_owned = current.clone();
                if let Some(sub_features) = manifest.features.get(&current_owned) {
                    for sub in sub_features {
                        if !sub.starts_with("dep:")
                            && !sub.contains('/')
                            && manifest.features.contains_key(sub)
                        {
                            stack.push((sub.clone(), false));
                        }
                    }
                }
            } else {
                // All children processed — mark as done
                let node = current.clone();
                in_stack.remove(&node);
                processed.insert(node);
                stack.pop();
            }
        }
    }
    Ok(())
}

/// Check if a dependency should be included (non-optional, or activated optional).
pub fn should_include_dep(name: &str, dep: &Dependency, resolved: &ResolvedFeatures) -> bool {
    match dep {
        Dependency::Simple(_) => true,
        Dependency::Detailed(d) => {
            if d.optional {
                resolved.activated_optional_deps.contains(name)
            } else {
                true
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmod_core::manifest::{DetailedDependency, Package};

    fn test_manifest(
        features: BTreeMap<String, Vec<String>>,
        deps: BTreeMap<String, Dependency>,
    ) -> Manifest {
        Manifest {
            package: Package {
                name: "test".to_string(),
                version: "0.1.0".to_string(),
                edition: None,
                description: None,
                authors: vec![],
                license: None,
                repository: None,
                homepage: None,
            },
            module: None,
            dependencies: deps,
            dev_dependencies: BTreeMap::new(),
            build_dependencies: BTreeMap::new(),
            features,
            compat: None,
            toolchain: None,
            build: None,
            test: None,
            format: None,
            lint: None,
            workspace: None,
            cache: None,
            metadata: None,
            security: None,
            publish: None,
            hooks: None,
            ide: None,
            plugins: None,
            abi: None,
            target: BTreeMap::new(),
        }
    }

    #[test]
    fn test_resolve_no_features() {
        let manifest = test_manifest(BTreeMap::new(), BTreeMap::new());
        let result = resolve_features(&manifest, &[], false).unwrap();
        assert!(result.dep_features.is_empty());
        assert!(result.activated_optional_deps.is_empty());
    }

    #[test]
    fn test_resolve_default_features() {
        let mut features = BTreeMap::new();
        features.insert("default".to_string(), vec!["simd".to_string()]);
        features.insert("simd".to_string(), vec![]);

        let manifest = test_manifest(features, BTreeMap::new());
        let result = resolve_features(&manifest, &[], false).unwrap();
        // default feature should be processed
        assert!(result.dep_features.is_empty());
    }

    #[test]
    fn test_resolve_no_default_features() {
        let mut features = BTreeMap::new();
        features.insert("default".to_string(), vec!["simd".to_string()]);
        features.insert("simd".to_string(), vec!["dep:simd_lib".to_string()]);

        let manifest = test_manifest(features, BTreeMap::new());
        let result = resolve_features(&manifest, &[], true).unwrap();
        // Default features disabled, so simd_lib not activated
        assert!(!result.activated_optional_deps.contains("simd_lib"));
    }

    #[test]
    fn test_resolve_explicit_features() {
        let mut features = BTreeMap::new();
        features.insert("simd".to_string(), vec!["dep:simd_lib".to_string()]);

        let manifest = test_manifest(features, BTreeMap::new());
        let result = resolve_features(&manifest, &["simd".to_string()], true).unwrap();
        assert!(result.activated_optional_deps.contains("simd_lib"));
    }

    #[test]
    fn test_resolve_feature_enables_dep_feature() {
        let mut features = BTreeMap::new();
        features.insert("fast".to_string(), vec!["math/avx2".to_string()]);

        let manifest = test_manifest(features, BTreeMap::new());
        let result = resolve_features(&manifest, &["fast".to_string()], true).unwrap();
        assert!(result.dep_features["math"].contains("avx2"));
    }

    #[test]
    fn test_resolve_transitive_features() {
        let mut features = BTreeMap::new();
        features.insert("default".to_string(), vec!["full".to_string()]);
        features.insert(
            "full".to_string(),
            vec!["simd".to_string(), "logging".to_string()],
        );
        features.insert("simd".to_string(), vec!["dep:simd_lib".to_string()]);
        features.insert("logging".to_string(), vec!["dep:log_lib".to_string()]);

        let manifest = test_manifest(features, BTreeMap::new());
        let result = resolve_features(&manifest, &[], false).unwrap();
        assert!(result.activated_optional_deps.contains("simd_lib"));
        assert!(result.activated_optional_deps.contains("log_lib"));
    }

    #[test]
    fn test_should_include_dep_non_optional() {
        let dep = Dependency::Simple("^1.0".to_string());
        let resolved = ResolvedFeatures::default();
        assert!(should_include_dep("fmt", &dep, &resolved));
    }

    #[test]
    fn test_should_include_dep_optional_not_activated() {
        let dep = Dependency::Detailed(DetailedDependency {
            version: Some("^1.0".to_string()),
            git: None,
            branch: None,
            rev: None,
            tag: None,
            path: None,
            features: vec![],
            optional: true,
            default_features: true,
            workspace: false,
        });
        let resolved = ResolvedFeatures::default();
        assert!(!should_include_dep("opt_dep", &dep, &resolved));
    }

    #[test]
    fn test_should_include_dep_optional_activated() {
        let dep = Dependency::Detailed(DetailedDependency {
            version: Some("^1.0".to_string()),
            git: None,
            branch: None,
            rev: None,
            tag: None,
            path: None,
            features: vec![],
            optional: true,
            default_features: true,
            workspace: false,
        });
        let mut resolved = ResolvedFeatures::default();
        resolved
            .activated_optional_deps
            .insert("opt_dep".to_string());
        assert!(should_include_dep("opt_dep", &dep, &resolved));
    }

    #[test]
    fn test_detect_feature_cycle() {
        let mut features = BTreeMap::new();
        features.insert("a".to_string(), vec!["b".to_string()]);
        features.insert("b".to_string(), vec!["a".to_string()]);

        let manifest = test_manifest(features, BTreeMap::new());
        let result = detect_feature_cycles(&manifest);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("cycle"));
    }

    #[test]
    fn test_detect_no_feature_cycle() {
        let mut features = BTreeMap::new();
        features.insert("a".to_string(), vec!["b".to_string()]);
        features.insert("b".to_string(), vec!["dep:lib".to_string()]);

        let manifest = test_manifest(features, BTreeMap::new());
        assert!(detect_feature_cycles(&manifest).is_ok());
    }

    #[test]
    fn test_dep_declared_features() {
        let mut deps = BTreeMap::new();
        deps.insert(
            "math".to_string(),
            Dependency::Detailed(DetailedDependency {
                version: Some("^1.0".to_string()),
                git: None,
                branch: None,
                rev: None,
                tag: None,
                path: None,
                features: vec!["simd".to_string(), "f64".to_string()],
                optional: false,
                default_features: true,
                workspace: false,
            }),
        );

        let manifest = test_manifest(BTreeMap::new(), deps);
        let result = resolve_features(&manifest, &[], false).unwrap();
        assert!(result.dep_features["math"].contains("simd"));
        assert!(result.dep_features["math"].contains("f64"));
    }
}
