# Decision Doc: Publishing cmod Library Crates to crates.io

**Status:** Investigated 2026-07 (issue #46). Recommendation: **do not publish
for now** — see below. Revisit when an external library consumer actually
appears.

## Question

Should `cmod-core`, `cmod-cache`, `cmod-resolver` (and by extension the other
workspace crates) be published to crates.io, and if so under what names,
versioning policy, and release automation?

## Findings

### 1. The `cmod` names are taken (decisive)

`cmod` and `cmod-core` are **already registered on crates.io by an unrelated,
actively maintained project** — rise0chen's "Build a cross-language module
with Rust FFI" (v0.4.6, updated 2026-06, ~20k downloads,
<https://github.com/rise0chen/cmod>). The other six names
(`cmod-cache`, `cmod-resolver`, `cmod-build`, `cmod-workspace`,
`cmod-security`, `cmod-cli`, `cmod-lsp`) are free — but publishing a family
whose foundation crate cannot be named `cmod-core` means the whole registry
identity would need a different prefix. Name transfer is not realistic: the
existing crates are legitimately in use, not squatted.

### 2. Everything else is ready or trivially fixable

- Per-crate `description`, `license`, and `repository` metadata: already set
  on all 8 crates (workspace-inherited).
- Path dependencies would need `version = "..."` added alongside `path` (8
  in `cmod-cli` alone); cargo requires it for publishing.
- Publish order must follow the dependency DAG (`cmod-core` first, `cmod-cli`
  last); `cargo publish` has no built-in workspace ordering pre-stabilization,
  so a release script or `cargo-workspaces`/`release-plz` would be needed.
- `rust-version = "1.80"` is already declared and would carry to the registry.

### 3. There is no consumer pull

No external project consumes these crates as libraries today. The crates are
internal implementation layers of the `cmod` binary; their APIs churn freely
between alphas (e.g. `add_member` and `matches_test_patterns` changed
signature within this milestone without any deprecation cycle). Rust
consumers who want the libraries can already use git dependencies:

```toml
[dependencies]
cmod-core = { git = "https://github.com/satishbabariya/cmod", tag = "v0.1.0-alpha.2" }
```

## Options

| Option | Names | Cost | Benefit |
|--------|-------|------|---------|
| **A. Don't publish (recommended)** | n/a | none | No name compromise, no semver contract while APIs churn; git deps cover the rare consumer |
| B. Publish under a new prefix (e.g. `cmodpm-*`) | ugly, diverges from repo/binary name | rename or `package=`-alias all crates, maintain publish tooling, honor semver from day one | discoverability, `cargo add` ergonomics |
| C. Publish only leaf crates that are free (`cmod-cache`, …) | inconsistent | impossible in practice — all depend on the unavailable `cmod-core` | none |

## Recommendation

**Option A.** The deciding facts: the anchor names are unavailable to us, the
APIs are pre-1.0 and intentionally unstable, and no consumer demand exists.
Publishing would buy discoverability for libraries nobody imports, at the
cost of either an awkward prefix or a rename, plus a permanent semver
contract. Git dependencies serve any early adopter today.

**Trigger to revisit:** a concrete external consumer (or the 1.0 planning
cycle) — at that point choose a prefix, add path-dep versions, and wire
`release-plz` or a publish job into `release.yml` after the tag build.

## Decision

- [x] Option A — do not publish now (per this investigation; reopen #46 to
  overturn)
