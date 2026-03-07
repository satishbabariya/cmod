# Publishing & Releasing

cmod publishes releases by creating Git tags. This guide covers the publishing workflow, signing, and SBOM generation.

## How Publishing Works

In cmod, publishing means creating a Git tag that marks a specific version of your module. Since Git is the registry, a published tag is how consumers discover and resolve your module's versions.

## Publishing a Release

### Dry run

Preview what would happen without making changes:

```bash
cmod publish --dry-run
```

This validates:
- The manifest is well-formed
- The version follows semver
- `package.description` and `package.license` are set
- Governance policies pass (unless `--skip-governance`)

If governance validation fails, the output will show specific issues. Use `--skip-governance` to bypass these checks.

### Create the release tag

```bash
cmod publish
```

This creates a Git tag like `v0.1.0` based on `package.version` in `cmod.toml`.

### Push to remote

```bash
cmod publish --push
```

Creates the tag and pushes it to the `origin` remote.

### Sign the release

```bash
cmod publish --sign --push
```

Signs the Git tag using the signing backend configured in `[security]`:

```toml
[security]
signing_key = "ABCD1234"
signing_backend = "pgp"    # "pgp", "ssh", or "sigstore"
```

### Skip signing

If signing is configured but you want to skip it for a specific release:

```bash
cmod publish --no-sign
```

`--sign` and `--no-sign` are mutually exclusive.

### Skip governance checks

```bash
cmod publish --skip-governance
```

Bypasses governance policy validation (useful for hotfixes).

## Publish Configuration

Configure what gets included in the release:

```toml
[publish]
registry = "https://registry.example.com"   # Optional registry
include = ["src/**", "cmod.toml", "LICENSE"] # Files to include
exclude = ["tests/**", ".git", "*.tmp"]      # Files to exclude
tags = ["latest"]                            # Additional tags
```

## Pre-Publish Hooks

Run validation scripts before publishing:

```toml
[hooks]
pre-publish = "./scripts/validate-release.sh"
```

The hook runs in the project root. A non-zero exit code aborts the publish.

## SBOM Generation

Generate a Software Bill of Materials for your release:

```bash
cmod sbom                      # Print to stdout
cmod sbom -o sbom.json         # Write to file
```

Include SBOM generation in your release workflow:

```bash
cmod sbom -o sbom.json
cmod publish --sign --push
```

## Release Checklist

1. Update `package.version` in `cmod.toml`
2. Run tests: `cmod test`
3. Verify dependencies: `cmod verify --signatures`
4. Audit dependencies: `cmod audit`
5. Generate SBOM: `cmod sbom -o sbom.json`
6. Dry run: `cmod publish --dry-run`
7. Publish: `cmod publish --sign --push`
