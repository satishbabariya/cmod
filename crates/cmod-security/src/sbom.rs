//! Software Bill of Materials (SBOM) generation.
//!
//! Generates a CycloneDX-compatible JSON SBOM from a manifest and lockfile,
//! documenting all direct and transitive dependencies for supply chain transparency.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use cmod_core::error::CmodError;
use cmod_core::lockfile::Lockfile;
use cmod_core::manifest::Manifest;

/// CycloneDX-compatible BOM format.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Sbom {
    /// BOM format identifier.
    #[serde(rename = "bomFormat")]
    pub bom_format: String,
    /// Specification version.
    #[serde(rename = "specVersion")]
    pub spec_version: String,
    /// Unique serial number for this BOM.
    #[serde(rename = "serialNumber")]
    pub serial_number: String,
    /// BOM version.
    pub version: u32,
    /// Metadata about the BOM itself.
    pub metadata: SbomMetadata,
    /// Components (dependencies).
    pub components: Vec<SbomComponent>,
    /// Dependency relationships.
    pub dependencies: Vec<SbomDependency>,
}

/// Metadata about who/when/what generated the BOM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SbomMetadata {
    /// When the BOM was generated.
    pub timestamp: String,
    /// Tool that generated the BOM.
    pub tools: Vec<SbomTool>,
    /// The top-level component.
    pub component: SbomComponent,
}

/// A tool used to generate the BOM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SbomTool {
    pub vendor: String,
    pub name: String,
    pub version: String,
}

/// A single software component in the BOM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SbomComponent {
    /// Component type (library, application, etc.).
    #[serde(rename = "type")]
    pub component_type: String,
    /// The identifier `dependencies` refer to the component by.
    #[serde(rename = "bom-ref", skip_serializing_if = "Option::is_none", default)]
    pub bom_ref: Option<String>,
    /// Package name.
    pub name: String,
    /// Package version.
    pub version: String,
    /// Package URL (purl).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub purl: Option<String>,
    /// Source repository.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// External references (repo URL, etc.).
    #[serde(
        rename = "externalReferences",
        skip_serializing_if = "Vec::is_empty",
        default
    )]
    pub external_references: Vec<SbomExternalRef>,
    /// Content hashes.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub hashes: Vec<SbomHash>,
}

/// External reference to a resource (repo, docs, etc.).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SbomExternalRef {
    #[serde(rename = "type")]
    pub ref_type: String,
    pub url: String,
}

/// A content hash.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SbomHash {
    #[serde(rename = "alg")]
    pub algorithm: String,
    pub content: String,
}

/// Dependency relationship.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SbomDependency {
    /// The component ref (name).
    #[serde(rename = "ref")]
    pub reference: String,
    /// Dependencies of this component.
    #[serde(rename = "dependsOn")]
    pub depends_on: Vec<String>,
}

/// Generate an SBOM from a manifest and lockfile.
pub fn generate_sbom(manifest: &Manifest, lockfile: &Lockfile) -> Result<Sbom, CmodError> {
    let timestamp = chrono_now();

    // Build the top-level component
    let root_component = SbomComponent {
        component_type: "application".to_string(),
        bom_ref: Some(manifest.package.name.clone()),
        name: manifest.package.name.clone(),
        version: manifest.package.version.clone(),
        purl: None,
        description: manifest.package.description.clone(),
        external_references: manifest
            .package
            .repository
            .as_ref()
            .map(|url| {
                vec![SbomExternalRef {
                    ref_type: "vcs".to_string(),
                    url: url.clone(),
                }]
            })
            .unwrap_or_default(),
        hashes: vec![],
    };

    // Build components from lockfile
    let mut components = Vec::new();
    let mut dep_map: BTreeMap<String, Vec<String>> = BTreeMap::new();

    for pkg in &lockfile.packages {
        let mut ext_refs = Vec::new();
        if let Some(ref repo) = pkg.repo {
            ext_refs.push(SbomExternalRef {
                ref_type: "vcs".to_string(),
                url: repo.clone(),
            });
        }

        let hashes = pkg
            .hash
            .as_deref()
            .and_then(sha256_hex_content)
            .map(|content| SbomHash {
                algorithm: "SHA-256".to_string(),
                content,
            })
            .into_iter()
            .collect();

        let purl = pkg
            .repo
            .as_ref()
            .map(|repo| format!("pkg:cmod/{}@{}?vcs_url={}", pkg.name, pkg.version, repo));

        components.push(SbomComponent {
            component_type: "library".to_string(),
            bom_ref: Some(pkg.name.clone()),
            name: pkg.name.clone(),
            version: pkg.version.clone(),
            purl,
            description: None,
            external_references: ext_refs,
            hashes,
        });

        dep_map.insert(pkg.name.clone(), pkg.deps.clone());
    }

    // Build dependency graph. A reference must name a component of the
    // BOM: a dependency the lockfile does not list is left out.
    let listed = |name: &String| dep_map.contains_key(name);
    let mut dependencies = Vec::new();

    // Root depends on its direct deps
    let root_deps: Vec<String> = manifest
        .dependencies
        .keys()
        .filter(|name| listed(name))
        .cloned()
        .collect();
    dependencies.push(SbomDependency {
        reference: manifest.package.name.clone(),
        depends_on: root_deps,
    });

    // Each package's deps
    for (name, deps) in &dep_map {
        dependencies.push(SbomDependency {
            reference: name.clone(),
            depends_on: deps.iter().filter(|dep| listed(dep)).cloned().collect(),
        });
    }

    let serial = format!(
        "urn:uuid:{}",
        simple_uuid(&timestamp, &manifest.package.name)
    );

    Ok(Sbom {
        bom_format: "CycloneDX".to_string(),
        spec_version: "1.5".to_string(),
        serial_number: serial,
        version: 1,
        metadata: SbomMetadata {
            timestamp,
            tools: vec![SbomTool {
                vendor: "cmod".to_string(),
                name: "cmod".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
            }],
            component: root_component,
        },
        components,
        dependencies,
    })
}

/// Serialize the SBOM to a JSON string.
pub fn sbom_to_json(sbom: &Sbom) -> Result<String, CmodError> {
    serde_json::to_string_pretty(sbom)
        .map_err(|e| CmodError::Other(format!("failed to serialize SBOM: {}", e)))
}

/// Generate an ISO 8601 UTC timestamp string from system time.
fn chrono_now() -> String {
    use std::time::SystemTime;
    let duration = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap_or_default();
    let secs = duration.as_secs();
    // Convert to date/time components
    let days = secs / 86400;
    let time_secs = secs % 86400;
    let hours = time_secs / 3600;
    let minutes = (time_secs % 3600) / 60;
    let seconds = time_secs % 60;
    // Simple date calculation from days since epoch (1970-01-01)
    let (year, month, day) = days_to_ymd(days);
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        year, month, day, hours, minutes, seconds
    )
}

/// Convert days since Unix epoch to (year, month, day).
fn days_to_ymd(days: u64) -> (u64, u64, u64) {
    // Algorithm from http://howardhinnant.github.io/date_algorithms.html
    let z = days + 719468;
    let era = z / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

/// Generate a deterministic UUID-like string from inputs.
fn simple_uuid(timestamp: &str, name: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(timestamp.as_bytes());
    hasher.update(name.as_bytes());
    let mut bytes: [u8; 16] = hasher.finalize()[..16].try_into().unwrap();
    // RFC 4122, as CycloneDX requires of serial numbers: version 5 (a
    // name-based UUID from a hash) and the RFC's variant.
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    let hex = hex::encode(bytes);
    format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32],
    )
}

/// The lockfile's content hash (`sha256:<hex>`) as CycloneDX hash content,
/// which is the hex alone; `None` for anything else, such as the `local`
/// of a path dependency.
fn sha256_hex_content(hash: &str) -> Option<String> {
    let hex = hash.strip_prefix("sha256:").unwrap_or(hash);
    (hex.len() == 64 && hex.chars().all(|c| c.is_ascii_hexdigit()))
        .then(|| hex.to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;
    use cmod_core::lockfile::LockedPackage;

    fn make_pkg(name: &str, version: &str, repo: Option<&str>) -> LockedPackage {
        LockedPackage {
            name: name.to_string(),
            version: version.to_string(),
            source: None,
            repo: repo.map(|s| s.to_string()),
            commit: Some("abc123".to_string()),
            hash: Some("deadbeef".to_string()),
            toolchain: None,
            targets: BTreeMap::new(),
            deps: vec![],
            features: vec![],
        }
    }

    #[test]
    fn test_generate_sbom_empty() {
        let manifest = cmod_core::manifest::default_manifest("myapp");
        let lockfile = Lockfile {
            version: 1,
            integrity: None,
            packages: vec![],
        };

        let sbom = generate_sbom(&manifest, &lockfile).unwrap();
        assert_eq!(sbom.bom_format, "CycloneDX");
        assert_eq!(sbom.metadata.component.name, "myapp");
        assert!(sbom.components.is_empty());
    }

    #[test]
    fn test_generate_sbom_with_deps() {
        let manifest = cmod_core::manifest::default_manifest("myapp");
        let lockfile = Lockfile {
            version: 1,
            integrity: None,
            packages: vec![
                make_pkg("fmt", "10.2.0", Some("https://github.com/fmtlib/fmt")),
                make_pkg("spdlog", "1.14.0", Some("https://github.com/gabime/spdlog")),
            ],
        };

        let sbom = generate_sbom(&manifest, &lockfile).unwrap();
        assert_eq!(sbom.components.len(), 2);
        assert_eq!(sbom.components[0].name, "fmt");
        assert_eq!(sbom.components[1].name, "spdlog");

        // Check purl
        assert!(sbom.components[0]
            .purl
            .as_ref()
            .unwrap()
            .contains("pkg:cmod/fmt@10.2.0"));

        // `deadbeef` is no SHA-256: no hash is listed rather than an invalid one.
        assert!(sbom.components[0].hashes.is_empty());
        assert_eq!(sbom.components[0].bom_ref.as_deref(), Some("fmt"));
    }

    /// The BOM validates as CycloneDX 1.5: hash content is bare hex, every
    /// reference names a component's `bom-ref`, and the serial number is an
    /// RFC 4122 UUID.
    #[test]
    fn test_sbom_is_valid_cyclonedx() {
        let mut manifest = cmod_core::manifest::default_manifest("myapp");
        for name in ["github.com/fmtlib/fmt", "local_dep", "unlocked"] {
            manifest.dependencies.insert(
                name.to_string(),
                cmod_core::manifest::Dependency::Simple("^1".to_string()),
            );
        }
        let sha = "C04CE3C4779A073860C173C0F1478AE0736158D9223617AA3F6EEC5C97BDCF22";
        let mut fmt = make_pkg(
            "github.com/fmtlib/fmt",
            "10.2.0",
            Some("https://github.com/fmtlib/fmt"),
        );
        fmt.hash = Some(format!("sha256:{}", sha));
        fmt.deps = vec!["local_dep".to_string(), "gone".to_string()];
        let mut local = make_pkg("local_dep", "0.1.0", None);
        local.hash = Some("local".to_string());
        let lockfile = Lockfile {
            version: 1,
            integrity: None,
            packages: vec![fmt, local],
        };

        let sbom = generate_sbom(&manifest, &lockfile).unwrap();
        assert_eq!(
            sbom.components[0].hashes[0].content,
            sha.to_ascii_lowercase()
        );
        assert!(sbom.components[1].hashes.is_empty());

        let refs: Vec<&str> = std::iter::once(&sbom.metadata.component)
            .chain(&sbom.components)
            .filter_map(|c| c.bom_ref.as_deref())
            .collect();
        assert_eq!(refs, vec!["myapp", "github.com/fmtlib/fmt", "local_dep"]);
        for dep in &sbom.dependencies {
            assert!(refs.contains(&dep.reference.as_str()), "{}", dep.reference);
            for on in &dep.depends_on {
                assert!(refs.contains(&on.as_str()), "dangling {}", on);
            }
        }
        assert_eq!(
            sbom.dependencies[0].depends_on,
            vec!["github.com/fmtlib/fmt", "local_dep"]
        );

        let uuid = sbom.serial_number.strip_prefix("urn:uuid:").unwrap();
        let groups: Vec<&str> = uuid.split('-').collect();
        assert_eq!(
            groups.iter().map(|g| g.len()).collect::<Vec<_>>(),
            vec![8, 4, 4, 4, 12]
        );
        assert!(groups[2].starts_with('5'), "{}", uuid);
        assert!("89ab".contains(&groups[3][..1]), "{}", uuid);
    }

    #[test]
    fn test_sbom_to_json() {
        let manifest = cmod_core::manifest::default_manifest("myapp");
        let lockfile = Lockfile {
            version: 1,
            integrity: None,
            packages: vec![],
        };
        let sbom = generate_sbom(&manifest, &lockfile).unwrap();
        let json = sbom_to_json(&sbom).unwrap();

        assert!(json.contains("CycloneDX"));
        assert!(json.contains("myapp"));
        assert!(json.contains("specVersion"));
    }

    #[test]
    fn test_simple_uuid_deterministic() {
        let u1 = simple_uuid("2026-01-01", "test");
        let u2 = simple_uuid("2026-01-01", "test");
        assert_eq!(u1, u2);

        let u3 = simple_uuid("2026-01-02", "test");
        assert_ne!(u1, u3);
    }

    #[test]
    fn test_sbom_dependency_graph() {
        let manifest = cmod_core::manifest::default_manifest("myapp");
        let mut pkg = make_pkg("fmt", "10.2.0", None);
        pkg.deps = vec!["base".to_string()];

        let lockfile = Lockfile {
            version: 1,
            integrity: None,
            packages: vec![pkg, make_pkg("base", "1.0.0", None)],
        };

        let sbom = generate_sbom(&manifest, &lockfile).unwrap();
        // Root should have its own dep entry
        assert!(sbom.dependencies.iter().any(|d| d.reference == "myapp"));
        // fmt should depend on base
        let fmt_dep = sbom
            .dependencies
            .iter()
            .find(|d| d.reference == "fmt")
            .unwrap();
        assert!(fmt_dep.depends_on.contains(&"base".to_string()));
    }
}
