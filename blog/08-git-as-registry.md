# Git as a Package Registry: How and Why

*The design rationale behind using Git repositories as the sole package registry for cmod.*

---

Every major package manager in every popular language has a central registry. npm has npmjs.com. Python has PyPI. Rust has crates.io. Even C++ tools like Conan and vcpkg maintain curated central repositories of packages.

cmod takes a fundamentally different approach: **Git is the registry.** There is no central server, no account to create, no approval queue to wait in. Your Git repository is your package.

This isn't laziness or a missing feature. It's a deliberate architectural decision with significant advantages for the C++ ecosystem specifically.

## The Problems with Central Registries

Central registries have served the JavaScript and Python ecosystems well, but they come with structural problems that are particularly acute for C++:

### Single point of failure
When npm goes down, the JavaScript ecosystem halts. When PyPI has issues, CI pipelines break worldwide. A central registry is a critical dependency for every project that uses it. For C++ — which runs nuclear power plants, medical devices, and flight control systems — this centralized fragility is unacceptable.

### Name squatting and typosquatting
Central registries create a flat namespace where names are first-come, first-served. This leads to name squatting (reserving popular names) and typosquatting attacks (publishing malicious packages with names like `lod4sh` instead of `lodash`). The npm ecosystem has been hit by this repeatedly.

### Supply chain attacks via account compromise
When identity is tied to a registry account rather than a repository, compromising one account can push malicious code to millions of downstream users. The `event-stream` and `ua-parser-js` incidents in npm demonstrate this risk.

### Governance and politics
Who decides what gets published? Who moderates? Who pays for the infrastructure? Central registries inevitably become political, with questions about moderation, funding, and control that distract from the technical mission.

## Why Git Works as a Registry

Git repositories already have everything a package registry needs:

- **Identity:** A Git URL is a globally unique, human-readable identifier. `github.com/fmtlib/fmt` is unambiguous.
- **Versioning:** Git tags map directly to semantic versions. Tag `v10.2.1` means version `10.2.1`.
- **History:** Full version history with commit hashes provides cryptographic identity for every state of the code.
- **Distribution:** Git hosting is commoditized. GitHub, GitLab, Bitbucket, Codeberg, self-hosted Gitea — the infrastructure exists and is battle-tested.
- **Authentication:** SSH keys and tokens already control access. Private dependencies work with the same authentication you use for your own code.
- **Redundancy:** Every clone is a full copy. If GitHub goes down, your local clone still has everything.

## How cmod Uses Git

### Module identity

In cmod, a module's identity is derived from its Git URL using reverse-domain notation:

```
github.com/fmtlib/fmt       →  com.github.fmtlib.fmt
gitlab.com/org/lib           →  com.gitlab.org.lib
```

This eliminates name collisions entirely. There's no race to register names. Your repository URL *is* your package name.

### Version resolution

cmod resolves versions by listing Git tags that match semver patterns:

```toml
[dependencies]
"github.com/fmtlib/fmt" = ">=10.0"
```

cmod fetches the repository's tag list, finds all tags matching the constraint, and selects the best version. The resolved commit hash is written to `cmod.lock`.

### Cloning and caching

On first resolution, cmod performs a shallow clone of each dependency. Subsequent builds use the cached clone and only fetch new tags when running `cmod update`.

### Branch and path dependencies

For development workflows, cmod supports branch pinning and local path dependencies:

```toml
[dependencies]
"github.com/user/experimental" = { branch = "feature-x" }
"../shared-lib" = { path = "../shared-lib" }
```

## Publishing Is Pushing a Tag

In cmod, publishing is:

```bash
git tag v1.0.0
git push origin v1.0.0
```

That's it. The `cmod publish` command automates this:

```bash
cmod publish            # Creates and pushes a Git tag
cmod publish --dry-run  # Preview what would happen
```

## Addressing Common Concerns

### "How do you discover packages?"
The same way you discover Git repositories: search engines, GitHub search, awesome lists, word of mouth. The `cmod search` command queries GitHub's search API to find modules.

### "What about private dependencies?"
Private dependencies work seamlessly because cmod uses the same Git authentication you already have configured.

### "Isn't cloning slow?"
cmod uses shallow clones and caches aggressively. After first resolution, builds require zero network access.

### "What if a repository is deleted?"
cmod mitigates this with lockfiles (exact commit hashes), `cmod vendor` (local source copies), and the local cache.

## The Go Precedent

Go modules use a similar model successfully at massive scale since 2019. cmod takes this further by adding mandatory lockfiles, cryptographic verification, and the security guarantees that C++'s use in critical infrastructure demands.

## Looking Forward

The Git-as-registry model opens up federated discovery, mirror flexibility, and zero vendor lock-in. Git is the most successful version control system ever built. Using it as a package registry is a recognition that the infrastructure we need already exists.

[Get started with cmod →](https://github.com/satishbabariya/cmod)
