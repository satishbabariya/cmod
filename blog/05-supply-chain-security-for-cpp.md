# Supply Chain Security for C++: How cmod Protects Your Dependencies

*In a world of dependency confusion attacks and compromised packages, C++ deserves built-in supply chain security. cmod delivers it.*

---

## The Supply Chain Problem

Software supply chain attacks have exploded in recent years. The pattern is simple and devastating:

1. Attacker compromises a popular open-source package
2. Malicious code is distributed to thousands of downstream projects
3. By the time anyone notices, the damage is done

The C++ ecosystem is particularly vulnerable because:

- **No standard verification.** Most C++ projects download dependencies without verifying hashes or signatures.
- **Manual dependency management.** Git submodules, vendor directories, and ad-hoc scripts don't track what changed between versions.
- **No audit trail.** When a security issue is discovered, there's no standard way to determine which projects are affected.

cmod was designed with supply chain security as a first-class concern — not an afterthought bolted on later.

## How cmod Secures Your Build

### 1. Mandatory Lockfiles

Every cmod project has a `cmod.lock` file that records the exact Git commit hash for every dependency:

```
[[package]]
name = "github.com/fmtlib/fmt"
version = "10.2.1"
commit = "a0b8a4e67f91..."
```

This file is deterministic: given the same `cmod.toml`, the resolver produces the same lockfile. When you build with `--locked`, cmod verifies that every dependency matches its pinned commit exactly.

**What this prevents:**
- Silent dependency updates that introduce malicious code
- Version tag mutations (where a maintainer pushes a different commit to an existing tag)
- Resolution inconsistencies between developers and CI

### 2. Trust-On-First-Use (TOFU)

cmod implements a TOFU trust model for dependencies:

- The first time you resolve a dependency, its identity (repository URL, signing key) is recorded in the trust store
- On subsequent resolutions, cmod verifies that the dependency's identity hasn't changed
- If the identity changes unexpectedly, cmod alerts you and refuses to proceed

This catches:
- Repository takeover attacks (where an attacker gains control of a dependency's Git repository)
- URL hijacking (where a domain expires and is re-registered by an attacker)
- Key rotation without notification

### 3. Hash Verification

```bash
cmod verify
```

This command verifies the integrity of every resolved dependency by checking:

- Git commit hashes match the lockfile
- Source file contents haven't been modified after checkout
- No unexpected files have been added to dependency directories

### 4. Signature Verification

For dependencies that provide cryptographic signatures:

```bash
cmod verify --signatures
```

cmod checks that:
- Git tags or commits are signed with a known key
- Signatures are valid and haven't expired
- The signing key matches the trusted identity for that dependency

### 5. Security Policy Enforcement

cmod supports configurable security policies that define what's allowed in your project:

- Which sources are trusted
- What verification level is required
- Whether unsigned dependencies are permitted
- Network access restrictions for builds

The `--untrusted` flag explicitly opts into using unverified dependencies — making the security trade-off visible and deliberate rather than silent.

### 6. Dependency Auditing

```bash
cmod audit
```

The audit command analyzes your dependency tree for known security issues, helping you stay ahead of vulnerabilities before they become incidents.

### 7. Software Bill of Materials (SBOM)

```bash
cmod sbom --output sbom.json
```

cmod generates a complete SBOM listing every dependency, its version, source, and verification status. This is increasingly required by:

- Government regulations (US Executive Order 14028)
- Enterprise procurement policies
- Industry compliance frameworks (SOC 2, ISO 27001)

## The Security Pipeline

Here's how cmod's security features fit into a typical CI/CD pipeline:

```bash
# 1. Resolve dependencies (or verify lockfile is current)
cmod resolve

# 2. Build with locked dependencies — fail if lockfile doesn't match
cmod build --locked --release

# 3. Verify integrity of all dependencies
cmod verify --signatures

# 4. Audit for known vulnerabilities
cmod audit

# 5. Generate SBOM for compliance
cmod sbom --output sbom.json

# 6. Run tests
cmod test --release
```

Every step is auditable, deterministic, and automated. No manual verification needed.

## Comparison with Other Ecosystems

| Feature | cmod | npm | Cargo | pip | Conan |
|---------|------|-----|-------|-----|-------|
| Mandatory lockfiles | Yes | Yes (v7+) | Yes | No | Optional |
| Hash verification | Yes | Yes | Yes | Yes (hashes) | Partial |
| Signature verification | Yes | No | No (planned) | No | Partial |
| TOFU trust model | Yes | No | No | No | No |
| SBOM generation | Built-in | Third-party | Third-party | Third-party | No |
| Audit command | Built-in | Built-in | `cargo-audit` | `pip-audit` | No |

cmod's security model draws inspiration from the best practices across ecosystems while adding features — like TOFU trust and integrated SBOM generation — that are unique.

## Security Without Friction

The key design principle behind cmod's security features is that **security should be the default, not an opt-in.**

- Lockfiles are mandatory, not optional
- Verification is built into the standard workflow
- Opting out of security (`--untrusted`) requires an explicit flag
- SBOM generation is a single command, not a third-party toolchain

When security is easy, people use it. When it's hard, they skip it. cmod makes it easy.

## What's Coming

cmod's security roadmap includes:

- **Phase 4: Full signature verification** — Cryptographic proof of dependency integrity with `--locked --verify` mode
- **Vulnerability database integration** — Automated matching against known CVEs
- **Policy-as-code** — Define security requirements in `cmod.toml` and enforce them in CI

## Get Started

Security isn't something you add later. Start with cmod and get supply chain protection from your first `cmod init`:

```bash
cmod init my_secure_project
cd my_secure_project
cmod add github.com/fmtlib/fmt@10.0
cmod resolve        # Lockfile created automatically
cmod verify         # Verify from day one
cmod build --locked # Reproducible, verified build
```

---

*cmod is open source under Apache-2.0. Help us build the most secure C++ build tool — [contribute on GitHub](https://github.com/nickshouse/cmod).*
