# Design Doc: `cmod search` Against a Real Index

**Status:** Phase 1 complete 2026-07 (issue #78) — the index repo is live at
[cmod-registry/index](https://github.com/cmod-registry/index), seeded with
the nine cmod-ecosystem ports; `cmod search` works end-to-end against it
and the publish path now commits+pushes (it previously only edited the
local cache). Phase 2 (PR-based submissions) is next (#79). Companion to
[plan-crates-io-publishing.md](plan-crates-io-publishing.md) and RFC-0015
(ecosystem governance).

## What already exists (more than the issue assumed)

The code side of a Git-hosted registry is implemented and tested in
`crates/cmod-resolver/src/registry.rs`:

- **Index format**: `RegistryIndex` — one JSON document containing
  `RegistryEntry` records (name, description, keywords, repository,
  versions with yank flags). Serialized/loaded via `load`/`save`.
- **Client**: `RegistryClient` clones/pulls the index repo into the local
  cache dir; `cmod search` queries local deps + lockfile first, then the
  remote index, with a cached-index fallback under `--offline`.
- **Publish path**: `cmod publish` with `[publish] registry = "<url>"`
  appends the released version to the module's entry.
- **Governance**: `GovernancePolicy` + `NamingRules` +
  `validate_for_publishing` (semver validity, license presence, banned
  names); search already excludes deprecated modules.

## What is missing

1. ~~**The index repo itself.**~~ **Done (phase 1):** the repo exists and
   is seeded; default-configuration `cmod search` returns registry results.
2. **A submission path for non-owners.** `publish_module` pushes directly —
   fine for a single maintainer, wrong for community submissions.
3. **Scale/abuse story** — irrelevant until 1 and 2 exist.

## Design

### Hosting and identity

- Claim the **`cmod-registry` GitHub org** (or fall back to
  `satishbabariya/cmod-index` and update `default_url()` — one-line change).
  The org form is preferred: it survives a repo transfer and matches the
  URL already baked into released binaries.
- The index repo contains `index.json` plus a `POLICY.md` derived from
  `GovernancePolicy` defaults.

### Index format v1

Keep the existing single-file `RegistryIndex` JSON — it is implemented,
tested, and fine for hundreds of modules. Record a `schema_version` field
now (additive) so v2 sharding can be detected by old clients.

### Submission flow (phase 2)

PR-based, mirroring how Homebrew/winget operate at small scale:

1. Publisher runs `cmod publish`; with no direct push rights the client
   emits the JSON fragment and a link to open a PR against the index repo.
2. A GitHub Action on the index repo runs the same
   `validate_for_publishing` rules (naming, semver, license) plus a
   reachability check of the module's Git URL + tag.
3. Maintainer merge = listing. Yanks are PRs flipping the `yanked` flag —
   never row deletion, so lockfiles keep resolving.

### Scale path (phase 3, when needed)

Single JSON → shard by first path segment of the module name
(`index/github.com/f*.json`), the crates.io-index pattern. The
`schema_version` bump plus a client update handles migration; not worth
building before the index has ~1k entries.

## Phased plan

| Phase | Work | Size |
|---|---|---|
| 1 — bootstrap | Create the index repo (empty index + POLICY.md), verify `cmod search`/`publish` round-trip against it, add `schema_version` | small; unblocks everything |
| 2 — submissions | PR-template + validation Action; `cmod publish` emits PR fragment when direct push fails | medium |
| 3 — scale | Sharded index, client support | deferred until needed |

## Owner decisions required before phase 1

- [ ] Claim `cmod-registry` org vs. change `default_url()` to a personal repo
- [ ] Moderation stance for name disputes (RFC-0015 has the framework;
      POLICY.md must state who decides)
