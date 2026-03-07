# Security & Trust

cmod includes supply-chain security features to verify dependency integrity and authenticity. This guide covers the trust model, verification, signing, and auditing.

## Trust Model: TOFU

cmod uses a **Trust On First Use** (TOFU) model:

1. The first time you fetch a dependency, cmod records its Git origin and content hash in a local trust database
2. On subsequent fetches, cmod verifies that the source matches the trusted record
3. If the source has changed unexpectedly, cmod raises a security warning

### Trust database

The trust database is stored at `~/.config/cmod/trust.toml`. It tracks:
- Git URLs of trusted dependencies
- Content hashes of previously fetched sources
- Signature verification status

### Bypassing trust checks

For development or when intentionally adding untrusted dependencies:

```bash
cmod resolve --untrusted
cmod build --untrusted
```

The `--untrusted` flag skips TOFU verification. Use with caution.

## Integrity Verification

### Verify dependencies

Check the integrity of all resolved dependencies:

```bash
cmod verify
```

This verifies:
- Content hashes match what's recorded in the lockfile
- Source files haven't been tampered with
- The lockfile integrity hash is valid

### Verify with signatures

```bash
cmod verify --signatures
```

Also checks Git commit signatures (GPG or SSH) on dependency repositories.

### Build-time verification

Verify lockfile integrity before building:

```bash
cmod build --verify
```

### Strict mode for CI

Combine `--locked` and `--verify` for maximum assurance in CI:

```bash
cmod build --locked --verify
```

This fails if:
- The lockfile is missing or outdated
- The integrity hash doesn't match
- Any dependency source has changed

## Signing Configuration

### Setting up signing

Configure signing in `cmod.toml`:

```toml
[security]
signing_key = "ABCD1234"          # Your signing key ID
signing_backend = "pgp"           # "pgp", "ssh", or "sigstore"
signature_policy = "warn"         # "none", "warn", or "require"
```

### Signing backends

| Backend | Key Type | Description |
|---------|----------|-------------|
| `pgp` | GPG key ID | Traditional PGP/GPG signing |
| `ssh` | SSH key path | Git SSH commit signing |
| `sigstore` | OIDC identity | Keyless signing via Sigstore/Fulcio |

### Sigstore configuration

For keyless signing with Sigstore:

```toml
[security]
signing_backend = "sigstore"
oidc_issuer = "https://accounts.google.com"
certificate_identity = "user@example.com"
```

### Signature policy

Controls how cmod handles unsigned dependencies:

| Policy | Behavior |
|--------|----------|
| `none` | No signature verification |
| `warn` | Warn about unsigned dependencies (default) |
| `require` | Fail if any dependency is unsigned |

## Trusted Sources

Specify patterns for trusted Git sources:

```toml
[security]
trusted_sources = [
    "github.com/*",
    "gitlab.com/myorg/*",
]
verify_checksums = true
```

- `verify_checksums = true` — verify content hashes on every dependency fetch
- `trusted_sources` — wildcard patterns for sources that bypass extra scrutiny

## Publishing with Signatures

Sign release tags when publishing:

```bash
cmod publish --sign --push
```

Skip signing (overrides `[security]` config):

```bash
cmod publish --no-sign
```

## Dependency Auditing

Audit dependencies for security and quality issues:

```bash
cmod audit
```

This checks for:
- Known vulnerabilities in dependencies
- Dependency age and maintenance status
- Unsigned or unverified sources

## Software Bill of Materials (SBOM)

Generate an SBOM listing all dependencies and their versions:

```bash
cmod sbom                      # Print to stdout
cmod sbom -o sbom.json         # Write to file
```

The SBOM includes:
- All direct and transitive dependencies
- Exact versions and commit hashes
- Source URLs
- License information (when available)

## Security Best Practices

1. **Always commit `cmod.lock`** — ensures reproducible builds with pinned versions
2. **Use `--locked` in CI** — prevents silent dependency updates
3. **Use `--verify` in CI** — catches lockfile tampering
4. **Enable `verify_checksums`** — verifies content on every fetch
5. **Set `signature_policy = "warn"` or `"require"`** — track unsigned dependencies
6. **Run `cmod audit` regularly** — check for known vulnerabilities
7. **Generate SBOMs for releases** — `cmod sbom -o sbom.json`
8. **Review `cmod.lock` diffs in PRs** — catch unexpected dependency changes
