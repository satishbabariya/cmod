# RFC-0022: Security, Signing & Supply-Chain Integrity

## Status
Draft

## Summary
This RFC defines the **security model** for **cmod**, covering module authenticity, artifact signing, dependency trust, and cache integrity. The goal is to provide strong supply-chain guarantees **without central registries** and without breaking the Git-native workflow.

---

## Motivation

Modern build systems are vulnerable to:
- Dependency hijacking
- Malicious commits or force-pushes
- Cache poisoning
- CI credential abuse

C++ ecosystems lack standardized, end-to-end supply-chain protection. cmod addresses this by combining **Git trust**, **cryptographic signing**, and **content-addressed verification**.

---

## Goals

- Verify module provenance
- Prevent tampered artifacts
- Secure distributed caches
- Preserve decentralization
- Integrate cleanly with Git and CI

## Non-Goals

- Enforcing a single trust authority
- Preventing all malicious code
- Replacing OS-level sandboxing

---

## Trust Model

cmod follows a **trust-on-first-use (TOFU)** model:

- Modules are trusted based on Git origin
- Commits are pinned via `cmod.lock`
- Optional cryptographic verification strengthens trust

Trust is explicit, inspectable, and revocable.

---

## Module Signing

### Source Signing

Modules may optionally sign releases or commits using:
- OpenPGP
- Sigstore / Fulcio
- SSH commit signing

Example:
```
cmod verify --signatures
```

Verification checks:
- Commit signature validity
- Key identity and trust level
- Match against lockfile commit hash

---

## Artifact Signing

Binary artifacts (BMIs, objects) may be signed at build time.

- Signatures stored alongside cache entries
- Verified on download
- Invalid signatures invalidate cache entries

This protects against cache poisoning and MITM attacks.

---

## Cache Integrity

- All cache entries are content-addressed
- Hash verified before use
- Optional signature verification

Remote cache requirements:
- Authenticated access
- TLS transport
- Explicit user opt-in

---

## Lockfile Enforcement

Security-sensitive modes:

```
cmod build --locked --verify
```

Guarantees:
- No dependency drift
- No unsigned or unexpected modules
- No silent resolution changes

CI systems are encouraged to enforce this mode.

---

## Dependency Auditing

Optional tooling (RFC-0018):
- License scanning
- Known vulnerability checks
- Dependency graph inspection

Example:
```
cmod audit
```

---

## Revocation & Recovery

- Trusted keys can be revoked locally
- Lockfile updates required to accept new commits
- Compromised caches can be flushed safely

No global revocation authority is required.

---

## Comparison

| System | Security Model |
|------|---------------|
| Cargo | Checksums + registry trust |
| npm | Registry + signatures |
| Bazel | Workspace trust + remote cache |
| cmod | Git + lockfile + optional signatures |

---

## Backward Compatibility

- All security features are opt-in
- Unsigned modules continue to work
- Warnings available for insecure configurations

---

## Open Questions

- Default signature requirements in CI?
- Standard key distribution mechanisms?
- SBOM generation support?

---

## Conclusion

This RFC completes cmod’s **end-to-end supply-chain story**:
- Git-native
- Decentralized
- Verifiable
- Practical for real-world C++ projects

---

## End of Core RFC Series
