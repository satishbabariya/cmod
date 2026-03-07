//! Advanced conditional dependency resolution and feature propagation.
//!
//! Implements RFC-0017 enhancements:
//! - Complex conditional expressions for dependency selection
//! - Feature propagation across transitive dependencies
//! - Dynamic feature selection based on platform and capabilities
//! - Feature conflict detection

use std::collections::{BTreeMap, BTreeSet};

use serde::{Deserialize, Serialize};

use cmod_core::error::CmodError;
use cmod_core::manifest::Manifest;

/// A transitive feature request: propagate features down the dependency tree.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeaturePropagation {
    /// Source package that activated this feature.
    pub source: String,
    /// Target package to activate features on.
    pub target: String,
    /// Features to activate.
    pub features: BTreeSet<String>,
}

/// Resolve features across the entire dependency tree, including transitive propagation.
pub fn resolve_transitive_features(
    root_manifest: &Manifest,
    dep_manifests: &BTreeMap<String, Manifest>,
    requested_features: &[String],
    no_default_features: bool,
    target_triple: &str,
) -> Result<TransitiveFeatureMap, CmodError> {
    let mut feature_map = TransitiveFeatureMap::default();

    // Start with root features
    let root_resolved =
        crate::features::resolve_features(root_manifest, requested_features, no_default_features)?;

    // Record root's dep features
    for (dep_name, features) in &root_resolved.dep_features {
        feature_map
            .dep_features
            .entry(dep_name.clone())
            .or_default()
            .extend(features.clone());
    }

    feature_map
        .activated_optional
        .extend(root_resolved.activated_optional_deps);

    // Process each dependency and propagate features
    let mut visited = BTreeSet::new();
    let mut queue: Vec<String> = root_manifest.dependencies.keys().cloned().collect();

    while let Some(dep_name) = queue.pop() {
        if !visited.insert(dep_name.clone()) {
            continue;
        }

        let dep_manifest = match dep_manifests.get(&dep_name) {
            Some(m) => m,
            None => continue,
        };

        // Get features activated for this dep
        let activated: Vec<String> = feature_map
            .dep_features
            .get(&dep_name)
            .map(|f| f.iter().cloned().collect())
            .unwrap_or_default();

        // Resolve this dep's features
        let dep_resolved = crate::features::resolve_features(dep_manifest, &activated, false)?;

        // Propagate to transitive deps
        for (sub_dep, sub_features) in &dep_resolved.dep_features {
            let entry = feature_map.dep_features.entry(sub_dep.clone()).or_default();
            let old_len = entry.len();
            entry.extend(sub_features.clone());

            // Record propagation
            if entry.len() > old_len {
                feature_map.propagations.push(FeaturePropagation {
                    source: dep_name.clone(),
                    target: sub_dep.clone(),
                    features: sub_features.clone(),
                });
            }
        }

        // Activate transitive optional deps
        for opt_dep in &dep_resolved.activated_optional_deps {
            feature_map.activated_optional.insert(opt_dep.clone());
        }

        // Evaluate platform-specific deps
        let effective_deps = dep_manifest.effective_dependencies(target_triple);
        for sub_dep in effective_deps.keys() {
            if !visited.contains(sub_dep) {
                queue.push(sub_dep.clone());
            }
        }
    }

    // Detect conflicts
    feature_map.conflicts = detect_feature_conflicts(&feature_map);

    Ok(feature_map)
}

/// Aggregated feature map across the full dependency tree.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TransitiveFeatureMap {
    /// Features activated per dependency.
    pub dep_features: BTreeMap<String, BTreeSet<String>>,
    /// Optional dependencies that are activated.
    pub activated_optional: BTreeSet<String>,
    /// Feature propagation records.
    pub propagations: Vec<FeaturePropagation>,
    /// Detected feature conflicts.
    pub conflicts: Vec<FeatureConflict>,
}

/// A feature conflict between two dependencies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FeatureConflict {
    /// Package with the conflict.
    pub package: String,
    /// Feature causing the conflict.
    pub feature: String,
    /// Description of the conflict.
    pub reason: String,
    /// Which packages activated conflicting features.
    pub activated_by: Vec<String>,
}

/// Detect feature conflicts in the resolved feature map.
fn detect_feature_conflicts(feature_map: &TransitiveFeatureMap) -> Vec<FeatureConflict> {
    let mut conflicts = Vec::new();

    // Check for mutually exclusive features (naming convention: "no-X" conflicts with "X")
    for (dep, features) in &feature_map.dep_features {
        for feature in features {
            let negation = format!("no-{}", feature);
            if features.contains(&negation) {
                // Find which propagations caused this
                let activated_by: Vec<String> = feature_map
                    .propagations
                    .iter()
                    .filter(|p| {
                        p.target == *dep
                            && (p.features.contains(feature) || p.features.contains(&negation))
                    })
                    .map(|p| p.source.clone())
                    .collect();

                conflicts.push(FeatureConflict {
                    package: dep.clone(),
                    feature: feature.clone(),
                    reason: format!(
                        "feature '{}' conflicts with '{}' on package '{}'",
                        feature, negation, dep
                    ),
                    activated_by,
                });
            }
        }
    }

    conflicts
}

/// Evaluate whether a dependency should be included based on cfg conditions.
pub fn evaluate_conditional_dep(
    cfg_expr: &str,
    target_triple: &str,
    activated_features: &BTreeSet<String>,
) -> bool {
    // Standard cfg evaluation
    if cmod_core::manifest::eval_cfg(cfg_expr, target_triple) {
        return true;
    }

    // Feature-based cfg: cfg(feature = "X")
    if let Some(inner) = cfg_expr
        .strip_prefix("cfg(feature = \"")
        .and_then(|s| s.strip_suffix("\")"))
    {
        return activated_features.contains(inner);
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmod_core::manifest::{Dependency, Package};

    fn test_manifest(
        name: &str,
        deps: BTreeMap<String, Dependency>,
        features: BTreeMap<String, Vec<String>>,
    ) -> Manifest {
        Manifest {
            package: Package {
                name: name.to_string(),
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
    fn test_resolve_transitive_features_empty() {
        let manifest = test_manifest("root", BTreeMap::new(), BTreeMap::new());
        let result = resolve_transitive_features(
            &manifest,
            &BTreeMap::new(),
            &[],
            false,
            "x86_64-unknown-linux-gnu",
        )
        .unwrap();
        assert!(result.dep_features.is_empty());
        assert!(result.conflicts.is_empty());
    }

    #[test]
    fn test_resolve_transitive_features_propagation() {
        let mut root_features = BTreeMap::new();
        root_features.insert("fast".to_string(), vec!["math/avx2".to_string()]);

        let mut root_deps = BTreeMap::new();
        root_deps.insert("math".to_string(), Dependency::Simple("^1.0".into()));

        let root = test_manifest("root", root_deps, root_features);

        let mut math_features = BTreeMap::new();
        math_features.insert("avx2".to_string(), vec!["simd/avx2".to_string()]);
        let mut math_deps = BTreeMap::new();
        math_deps.insert("simd".to_string(), Dependency::Simple("^1.0".into()));
        let math = test_manifest("math", math_deps, math_features);

        let mut dep_manifests = BTreeMap::new();
        dep_manifests.insert("math".to_string(), math);

        let result = resolve_transitive_features(
            &root,
            &dep_manifests,
            &["fast".to_string()],
            true,
            "x86_64-unknown-linux-gnu",
        )
        .unwrap();

        assert!(result.dep_features.get("math").unwrap().contains("avx2"));
        assert!(result.dep_features.get("simd").unwrap().contains("avx2"));
        assert!(!result.propagations.is_empty());
    }

    #[test]
    fn test_detect_feature_conflicts() {
        let mut feature_map = TransitiveFeatureMap::default();
        let mut features = BTreeSet::new();
        features.insert("threads".to_string());
        features.insert("no-threads".to_string());
        feature_map
            .dep_features
            .insert("runtime".to_string(), features);

        let conflicts = detect_feature_conflicts(&feature_map);
        assert!(!conflicts.is_empty());
        assert!(conflicts[0].reason.contains("conflicts"));
    }

    #[test]
    fn test_evaluate_conditional_dep_cfg() {
        let features = BTreeSet::new();
        assert!(evaluate_conditional_dep(
            "cfg(unix)",
            "x86_64-unknown-linux-gnu",
            &features
        ));
        assert!(!evaluate_conditional_dep(
            "cfg(windows)",
            "x86_64-unknown-linux-gnu",
            &features
        ));
    }

    #[test]
    fn test_evaluate_conditional_dep_feature() {
        let mut features = BTreeSet::new();
        features.insert("simd".to_string());

        assert!(evaluate_conditional_dep(
            "cfg(feature = \"simd\")",
            "x86_64-unknown-linux-gnu",
            &features
        ));
        assert!(!evaluate_conditional_dep(
            "cfg(feature = \"gpu\")",
            "x86_64-unknown-linux-gnu",
            &features
        ));
    }

    #[test]
    fn test_transitive_feature_map_serde() {
        let map = TransitiveFeatureMap::default();
        let json = serde_json::to_string(&map).unwrap();
        let parsed: TransitiveFeatureMap = serde_json::from_str(&json).unwrap();
        assert!(parsed.dep_features.is_empty());
    }
}
