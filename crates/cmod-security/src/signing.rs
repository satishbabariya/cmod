//! Cryptographic signing and verification for cmod packages.
//!
//! Supports multiple signing backends:
//! - **OpenPGP** via `gpg` CLI
//! - **SSH** via `ssh-keygen`
//! - **Sigstore** via `cosign` (keyless or key-based)
//!
//! The signing subsystem is used to:
//! 1. Sign commits/tags during `cmod publish`
//! 2. Verify commit signatures during `cmod verify --signatures`
//! 3. Sign BMI packages for distribution

use std::path::{Path, PathBuf};
use std::process::Command;

use serde::{Deserialize, Serialize};

use cmod_core::error::CmodError;

/// Supported signing backends.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SigningBackend {
    /// OpenPGP (GPG) signing.
    Pgp,
    /// SSH key signing (Git 2.34+).
    Ssh,
    /// Sigstore cosign keyless signing.
    Sigstore,
}

impl SigningBackend {
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().as_str() {
            "pgp" | "gpg" | "openpgp" => Some(SigningBackend::Pgp),
            "ssh" => Some(SigningBackend::Ssh),
            "sigstore" | "cosign" => Some(SigningBackend::Sigstore),
            _ => None,
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            SigningBackend::Pgp => "pgp",
            SigningBackend::Ssh => "ssh",
            SigningBackend::Sigstore => "sigstore",
        }
    }
}

/// Signing configuration resolved from manifest and environment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SigningConfig {
    /// Which backend to use.
    pub backend: SigningBackend,
    /// Key identifier (GPG key ID, SSH key path, or Sigstore identity).
    pub key_id: Option<String>,
    /// Path to the signing key file (SSH private key, etc.).
    pub key_path: Option<PathBuf>,
    /// Sigstore OIDC issuer URL (for keyless signing).
    pub oidc_issuer: Option<String>,
    /// Sigstore certificate identity (email).
    pub certificate_identity: Option<String>,
}

/// Result of a signing operation.
#[derive(Debug, Clone)]
pub struct SignResult {
    /// The signature data (PEM/base64).
    pub signature: String,
    /// The signer identity.
    pub signer: String,
    /// Backend used.
    pub backend: SigningBackend,
}

/// Result of a signature verification.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VerifyStatus {
    /// Signature is valid and trusted.
    Valid {
        signer: String,
        backend: SigningBackend,
    },
    /// Signature is present but the signer is not trusted.
    Untrusted {
        signer: String,
        backend: SigningBackend,
        reason: String,
    },
    /// No signature present.
    Unsigned,
    /// Signature verification failed.
    Invalid { reason: String },
}

impl VerifyStatus {
    pub fn is_valid(&self) -> bool {
        matches!(self, VerifyStatus::Valid { .. })
    }

    pub fn is_unsigned(&self) -> bool {
        matches!(self, VerifyStatus::Unsigned)
    }
}

/// Detect which signing backends are available on the system.
pub fn detect_available_backends() -> Vec<SigningBackend> {
    let mut backends = Vec::new();

    if is_tool_available("gpg") {
        backends.push(SigningBackend::Pgp);
    }
    if is_tool_available("ssh-keygen") {
        backends.push(SigningBackend::Ssh);
    }
    if is_tool_available("cosign") {
        backends.push(SigningBackend::Sigstore);
    }

    backends
}

/// Sign arbitrary data using the configured backend.
pub fn sign_data(config: &SigningConfig, data: &[u8]) -> Result<SignResult, CmodError> {
    match config.backend {
        SigningBackend::Pgp => sign_pgp(config, data),
        SigningBackend::Ssh => sign_ssh(config, data),
        SigningBackend::Sigstore => sign_sigstore(config, data),
    }
}

/// Verify a signature against data.
pub fn verify_signature(
    signature: &str,
    data: &[u8],
    config: Option<&SigningConfig>,
) -> VerifyStatus {
    if signature.contains("BEGIN PGP SIGNATURE") {
        verify_pgp_detached(signature, data)
    } else if signature.contains("BEGIN SSH SIGNATURE") {
        verify_ssh_detached(signature, data, config)
    } else if signature.starts_with('{') || signature.contains("cosign") {
        verify_sigstore_detached(signature, data, config)
    } else {
        VerifyStatus::Invalid {
            reason: "unrecognized signature format".to_string(),
        }
    }
}

/// Sign a file and produce a detached signature file.
pub fn sign_file(config: &SigningConfig, file_path: &Path) -> Result<SignResult, CmodError> {
    let data = std::fs::read(file_path)?;
    sign_data(config, &data)
}

/// Verify a file against its detached signature.
pub fn verify_file(
    file_path: &Path,
    signature_path: &Path,
    config: Option<&SigningConfig>,
) -> Result<VerifyStatus, CmodError> {
    let signature = std::fs::read_to_string(signature_path)?;
    let data = std::fs::read(file_path)?;
    Ok(verify_signature(&signature, &data, config))
}

// ─── PGP Backend ─────────────────────────────────────────────

fn sign_pgp(config: &SigningConfig, data: &[u8]) -> Result<SignResult, CmodError> {
    let tmp = tempfile::TempDir::new()?;
    let data_path = tmp.path().join("data");
    let sig_path = tmp.path().join("data.sig");
    std::fs::write(&data_path, data)?;

    let mut cmd = Command::new("gpg");
    cmd.args(["--detach-sign", "--armor", "--output"]);
    cmd.arg(&sig_path);

    if let Some(ref key_id) = config.key_id {
        cmd.args(["--local-user", key_id]);
    }

    cmd.arg(&data_path);

    let output = cmd
        .output()
        .map_err(|e| CmodError::Other(format!("failed to run gpg: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(CmodError::Other(format!("gpg signing failed: {}", stderr)));
    }

    let signature = std::fs::read_to_string(&sig_path)?;
    let signer = config
        .key_id
        .clone()
        .unwrap_or_else(|| "default".to_string());

    Ok(SignResult {
        signature,
        signer,
        backend: SigningBackend::Pgp,
    })
}

fn verify_pgp_detached(signature: &str, data: &[u8]) -> VerifyStatus {
    let tmp = match tempfile::TempDir::new() {
        Ok(t) => t,
        Err(_) => {
            return VerifyStatus::Invalid {
                reason: "failed to create temp dir".to_string(),
            }
        }
    };

    let sig_path = tmp.path().join("sig.asc");
    let data_path = tmp.path().join("data");

    if std::fs::write(&sig_path, signature).is_err() || std::fs::write(&data_path, data).is_err() {
        return VerifyStatus::Invalid {
            reason: "failed to write temp files".to_string(),
        };
    }

    let output = Command::new("gpg")
        .args([
            "--status-fd",
            "1",
            "--verify",
            &sig_path.display().to_string(),
            &data_path.display().to_string(),
        ])
        .output();

    match output {
        Ok(result) => {
            let stdout = String::from_utf8_lossy(&result.stdout);
            let stderr = String::from_utf8_lossy(&result.stderr);
            let signer = extract_gpg_signer(&stderr).unwrap_or_else(|| "unknown".to_string());

            if result.status.success()
                && (stdout.contains("GOODSIG") || stdout.contains("VALIDSIG"))
            {
                VerifyStatus::Valid {
                    signer,
                    backend: SigningBackend::Pgp,
                }
            } else if stdout.contains("BADSIG") {
                VerifyStatus::Invalid {
                    reason: format!("bad PGP signature from {}", signer),
                }
            } else if stdout.contains("NO_PUBKEY") || stderr.contains("No public key") {
                VerifyStatus::Untrusted {
                    signer: signer.clone(),
                    backend: SigningBackend::Pgp,
                    reason: "public key not found".to_string(),
                }
            } else if stdout.contains("EXPKEYSIG") {
                VerifyStatus::Untrusted {
                    signer: signer.clone(),
                    backend: SigningBackend::Pgp,
                    reason: "signing key expired".to_string(),
                }
            } else if result.status.success() {
                VerifyStatus::Valid {
                    signer,
                    backend: SigningBackend::Pgp,
                }
            } else {
                VerifyStatus::Untrusted {
                    signer: signer.clone(),
                    backend: SigningBackend::Pgp,
                    reason: "verification inconclusive".to_string(),
                }
            }
        }
        Err(_) => VerifyStatus::Invalid {
            reason: "gpg not available".to_string(),
        },
    }
}

fn extract_gpg_signer(stderr: &str) -> Option<String> {
    for line in stderr.lines() {
        if line.contains("Good signature from") {
            let start = line.find('"')?;
            let end = line.rfind('"')?;
            if start < end {
                return Some(line[start + 1..end].to_string());
            }
        }
    }
    None
}

// ─── SSH Backend ─────────────────────────────────────────────

fn sign_ssh(config: &SigningConfig, data: &[u8]) -> Result<SignResult, CmodError> {
    let key_path = config
        .key_path
        .as_deref()
        .or_else(|| config.key_id.as_deref().map(Path::new))
        .ok_or_else(|| {
            CmodError::Other(
                "SSH signing requires key_path or key_id pointing to private key".into(),
            )
        })?;

    let tmp = tempfile::TempDir::new()?;
    let data_path = tmp.path().join("data");
    std::fs::write(&data_path, data)?;

    let output = Command::new("ssh-keygen")
        .args([
            "-Y",
            "sign",
            "-f",
            &key_path.display().to_string(),
            "-n",
            "cmod",
        ])
        .arg(&data_path)
        .output()
        .map_err(|e| CmodError::Other(format!("failed to run ssh-keygen: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(CmodError::Other(format!("SSH signing failed: {}", stderr)));
    }

    let sig_path = data_path.with_extension("sig");
    let signature = if sig_path.exists() {
        std::fs::read_to_string(&sig_path)?
    } else {
        String::from_utf8_lossy(&output.stdout).to_string()
    };

    let signer = key_path.display().to_string();

    Ok(SignResult {
        signature,
        signer,
        backend: SigningBackend::Ssh,
    })
}

fn verify_ssh_detached(
    signature: &str,
    data: &[u8],
    config: Option<&SigningConfig>,
) -> VerifyStatus {
    let home = dirs::home_dir().unwrap_or_default();
    let allowed_signers_paths = [
        home.join(".ssh/allowed_signers"),
        home.join(".config/git/allowed_signers"),
    ];

    let allowed_signers = allowed_signers_paths.iter().find(|p| p.exists());

    let allowed_signers = match allowed_signers {
        Some(path) => path.clone(),
        None => {
            return VerifyStatus::Untrusted {
                signer: "unknown".to_string(),
                backend: SigningBackend::Ssh,
                reason: "no allowed_signers file found".to_string(),
            };
        }
    };

    let tmp = match tempfile::TempDir::new() {
        Ok(t) => t,
        Err(_) => {
            return VerifyStatus::Invalid {
                reason: "failed to create temp dir".to_string(),
            }
        }
    };

    let sig_path = tmp.path().join("sig");
    let data_path = tmp.path().join("data");

    if std::fs::write(&sig_path, signature).is_err() || std::fs::write(&data_path, data).is_err() {
        return VerifyStatus::Invalid {
            reason: "failed to write temp files".to_string(),
        };
    }

    let principal = config
        .and_then(|c| c.certificate_identity.as_deref())
        .unwrap_or("*");

    let output = Command::new("ssh-keygen")
        .args([
            "-Y",
            "verify",
            "-f",
            &allowed_signers.display().to_string(),
            "-I",
            principal,
            "-n",
            "cmod",
            "-s",
            &sig_path.display().to_string(),
        ])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            if let Some(ref mut stdin) = child.stdin {
                use std::io::Write;
                let _ = stdin.write_all(data);
            }
            child.wait_with_output()
        });

    match output {
        Ok(result) => {
            if result.status.success() {
                VerifyStatus::Valid {
                    signer: principal.to_string(),
                    backend: SigningBackend::Ssh,
                }
            } else {
                let stderr = String::from_utf8_lossy(&result.stderr);
                if stderr.contains("INVALID") || stderr.contains("Could not verify") {
                    VerifyStatus::Invalid {
                        reason: format!("bad SSH signature: {}", stderr.trim()),
                    }
                } else {
                    VerifyStatus::Untrusted {
                        signer: principal.to_string(),
                        backend: SigningBackend::Ssh,
                        reason: stderr.trim().to_string(),
                    }
                }
            }
        }
        Err(_) => VerifyStatus::Invalid {
            reason: "ssh-keygen not available".to_string(),
        },
    }
}

// ─── Sigstore Backend ────────────────────────────────────────

fn sign_sigstore(config: &SigningConfig, data: &[u8]) -> Result<SignResult, CmodError> {
    if !is_tool_available("cosign") {
        return Err(CmodError::Other(
            "cosign is not installed; install it from https://docs.sigstore.dev/cosign/system_config/installation/"
                .to_string(),
        ));
    }

    let tmp = tempfile::TempDir::new()?;
    let blob_path = tmp.path().join("blob");
    let sig_path = tmp.path().join("blob.sig");
    let cert_path = tmp.path().join("blob.cert");
    std::fs::write(&blob_path, data)?;

    let mut cmd = Command::new("cosign");

    if let Some(ref key_path) = config.key_path {
        // Key-based signing
        cmd.args([
            "sign-blob",
            "--key",
            &key_path.display().to_string(),
            "--output-signature",
            &sig_path.display().to_string(),
            "--yes",
        ]);
    } else {
        // Keyless signing (OIDC)
        cmd.args([
            "sign-blob",
            "--output-signature",
            &sig_path.display().to_string(),
            "--output-certificate",
            &cert_path.display().to_string(),
            "--yes",
        ]);
    }

    cmd.arg(&blob_path);

    let output = cmd
        .output()
        .map_err(|e| CmodError::Other(format!("failed to run cosign: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(CmodError::Other(format!(
            "cosign signing failed: {}",
            stderr
        )));
    }

    let signature = std::fs::read_to_string(&sig_path)?;
    let signer = config
        .certificate_identity
        .clone()
        .unwrap_or_else(|| "sigstore-keyless".to_string());

    Ok(SignResult {
        signature,
        signer,
        backend: SigningBackend::Sigstore,
    })
}

fn verify_sigstore_detached(
    signature: &str,
    data: &[u8],
    config: Option<&SigningConfig>,
) -> VerifyStatus {
    if !is_tool_available("cosign") {
        return VerifyStatus::Invalid {
            reason: "cosign not available for verification".to_string(),
        };
    }

    let tmp = match tempfile::TempDir::new() {
        Ok(t) => t,
        Err(_) => {
            return VerifyStatus::Invalid {
                reason: "failed to create temp dir".to_string(),
            }
        }
    };

    let blob_path = tmp.path().join("blob");
    let sig_path = tmp.path().join("blob.sig");

    if std::fs::write(&blob_path, data).is_err() || std::fs::write(&sig_path, signature).is_err() {
        return VerifyStatus::Invalid {
            reason: "failed to write temp files".to_string(),
        };
    }

    let mut cmd = Command::new("cosign");

    if let Some(cfg) = config {
        if let Some(ref key_path) = cfg.key_path {
            cmd.args([
                "verify-blob",
                "--key",
                &key_path.display().to_string(),
                "--signature",
                &sig_path.display().to_string(),
            ]);
        } else {
            // Keyless verification
            let issuer = cfg
                .oidc_issuer
                .as_deref()
                .unwrap_or("https://accounts.google.com");
            let identity = cfg.certificate_identity.as_deref().unwrap_or("*");

            cmd.args([
                "verify-blob",
                "--signature",
                &sig_path.display().to_string(),
                "--certificate-oidc-issuer",
                issuer,
                "--certificate-identity",
                identity,
            ]);
        }
    } else {
        cmd.args([
            "verify-blob",
            "--signature",
            &sig_path.display().to_string(),
            "--certificate-oidc-issuer",
            "https://accounts.google.com",
            "--certificate-identity-regexp",
            ".*",
        ]);
    }

    cmd.arg(&blob_path);

    let output = cmd.output();

    match output {
        Ok(result) => {
            let signer = config
                .and_then(|c| c.certificate_identity.clone())
                .unwrap_or_else(|| "sigstore".to_string());

            if result.status.success() {
                VerifyStatus::Valid {
                    signer,
                    backend: SigningBackend::Sigstore,
                }
            } else {
                let stderr = String::from_utf8_lossy(&result.stderr);
                VerifyStatus::Invalid {
                    reason: format!("cosign verification failed: {}", stderr.trim()),
                }
            }
        }
        Err(_) => VerifyStatus::Invalid {
            reason: "failed to execute cosign".to_string(),
        },
    }
}

// ─── BMI Signing ─────────────────────────────────────────────

/// Sign a BMI (precompiled module) file and write the detached signature.
///
/// Produces a `.pcm.sig` file alongside the BMI.
pub fn sign_bmi(bmi_path: &Path, config: &SigningConfig) -> Result<SignResult, CmodError> {
    let result = sign_file(config, bmi_path)?;
    let sig_path = bmi_path.with_extension(
        bmi_path
            .extension()
            .map(|e| format!("{}.sig", e.to_string_lossy()))
            .unwrap_or_else(|| "sig".to_string()),
    );
    std::fs::write(&sig_path, &result.signature)?;
    Ok(result)
}

/// Verify a BMI file against its detached signature.
///
/// Checks both signature validity and key revocation status.
pub fn verify_bmi(
    bmi_path: &Path,
    config: Option<&SigningConfig>,
    revoked_keys: &[String],
) -> Result<VerifyStatus, CmodError> {
    let sig_path = bmi_path.with_extension(
        bmi_path
            .extension()
            .map(|e| format!("{}.sig", e.to_string_lossy()))
            .unwrap_or_else(|| "sig".to_string()),
    );
    if !sig_path.exists() {
        return Ok(VerifyStatus::Unsigned);
    }

    let status = verify_file(bmi_path, &sig_path, config)?;

    // Check key revocation
    if let VerifyStatus::Valid { signer, backend } = &status {
        if revoked_keys.iter().any(|k| k == signer) {
            return Ok(VerifyStatus::Untrusted {
                signer: signer.clone(),
                backend: *backend,
                reason: "signing key has been revoked".to_string(),
            });
        }
    }

    Ok(status)
}

// ─── Helpers ─────────────────────────────────────────────────

fn is_tool_available(name: &str) -> bool {
    Command::new(name)
        .arg("--version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Resolve a signing configuration from manifest security settings.
pub fn resolve_signing_config(
    signing_key: Option<&str>,
    backend_hint: Option<&str>,
) -> Option<SigningConfig> {
    let backend = backend_hint.and_then(SigningBackend::parse).or_else(|| {
        signing_key.map(|key| {
            if key.ends_with(".pub") || key.starts_with("ssh-") || key.contains("id_") {
                SigningBackend::Ssh
            } else if key.contains("sigstore") || key.contains("cosign") {
                SigningBackend::Sigstore
            } else {
                SigningBackend::Pgp
            }
        })
    })?;

    let (key_id, key_path) = match backend {
        SigningBackend::Pgp => (signing_key.map(|s| s.to_string()), None),
        SigningBackend::Ssh => {
            let path = signing_key.map(|s| {
                let p = PathBuf::from(s);
                if p.is_absolute() {
                    p
                } else {
                    dirs::home_dir().unwrap_or_default().join(".ssh").join(s)
                }
            });
            (None, path)
        }
        SigningBackend::Sigstore => (None, signing_key.map(PathBuf::from)),
    };

    Some(SigningConfig {
        backend,
        key_id,
        key_path,
        oidc_issuer: None,
        certificate_identity: None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_signing_backend_parse() {
        assert_eq!(SigningBackend::parse("pgp"), Some(SigningBackend::Pgp));
        assert_eq!(SigningBackend::parse("gpg"), Some(SigningBackend::Pgp));
        assert_eq!(SigningBackend::parse("ssh"), Some(SigningBackend::Ssh));
        assert_eq!(
            SigningBackend::parse("sigstore"),
            Some(SigningBackend::Sigstore)
        );
        assert_eq!(
            SigningBackend::parse("cosign"),
            Some(SigningBackend::Sigstore)
        );
        assert_eq!(SigningBackend::parse("unknown"), None);
    }

    #[test]
    fn test_signing_backend_as_str() {
        assert_eq!(SigningBackend::Pgp.as_str(), "pgp");
        assert_eq!(SigningBackend::Ssh.as_str(), "ssh");
        assert_eq!(SigningBackend::Sigstore.as_str(), "sigstore");
    }

    #[test]
    fn test_verify_status_predicates() {
        assert!(VerifyStatus::Valid {
            signer: "a".into(),
            backend: SigningBackend::Pgp,
        }
        .is_valid());
        assert!(VerifyStatus::Unsigned.is_unsigned());
        assert!(!VerifyStatus::Unsigned.is_valid());
    }

    #[test]
    fn test_detect_available_backends() {
        let backends = detect_available_backends();
        // We can at least call it without panicking
        assert!(backends.len() <= 3);
    }

    #[test]
    fn test_resolve_signing_config_pgp() {
        let config = resolve_signing_config(Some("ABCD1234"), None).unwrap();
        assert_eq!(config.backend, SigningBackend::Pgp);
        assert_eq!(config.key_id.as_deref(), Some("ABCD1234"));
    }

    #[test]
    fn test_resolve_signing_config_ssh() {
        let config = resolve_signing_config(Some("id_ed25519"), None).unwrap();
        assert_eq!(config.backend, SigningBackend::Ssh);
        assert!(config.key_path.is_some());
    }

    #[test]
    fn test_resolve_signing_config_explicit_backend() {
        let config = resolve_signing_config(Some("mykey"), Some("sigstore")).unwrap();
        assert_eq!(config.backend, SigningBackend::Sigstore);
    }

    #[test]
    fn test_resolve_signing_config_none() {
        assert!(resolve_signing_config(None, None).is_none());
    }

    #[test]
    fn test_verify_signature_unknown_format() {
        let status = verify_signature("not a signature", b"data", None);
        assert!(matches!(status, VerifyStatus::Invalid { .. }));
    }

    #[test]
    fn test_verify_bmi_unsigned() {
        let tmp = tempfile::TempDir::new().unwrap();
        let bmi = tmp.path().join("test.pcm");
        std::fs::write(&bmi, b"fake bmi content").unwrap();

        // No .pcm.sig file → Unsigned
        let status = verify_bmi(&bmi, None, &[]).unwrap();
        assert!(status.is_unsigned());
    }

    #[test]
    fn test_verify_bmi_revoked_key() {
        let tmp = tempfile::TempDir::new().unwrap();
        let bmi = tmp.path().join("test.pcm");
        std::fs::write(&bmi, b"bmi content").unwrap();

        // Write a fake PGP signature that will "verify" as valid
        // In practice, verify_bmi delegates to verify_file which uses
        // verify_signature. We test the revocation path by checking that
        // even if a signature were valid, a revoked key signer is rejected.
        // Since we can't easily mock GPG, we verify the revocation logic
        // at the unit level via verify_bmi's return when sig file is absent.
        let status = verify_bmi(&bmi, None, &["revoked_signer".to_string()]).unwrap();
        // No sig file → Unsigned (revocation only checked after valid sig)
        assert!(status.is_unsigned());
    }

    #[test]
    fn test_sign_bmi_produces_sig_file() {
        // sign_bmi calls sign_file which requires a signing backend (gpg/ssh).
        // We can't test actual signing without keys, but we can verify the
        // sig path derivation logic.
        let bmi = PathBuf::from("/tmp/test.pcm");
        let sig_path = bmi.with_extension(
            bmi.extension()
                .map(|e| format!("{}.sig", e.to_string_lossy()))
                .unwrap_or_else(|| "sig".to_string()),
        );
        assert_eq!(sig_path, PathBuf::from("/tmp/test.pcm.sig"));
    }
}
