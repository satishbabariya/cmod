# cmod v0.1.0-alpha.4

**Compiler backends & ecosystem bootstrap.** cmod now builds C++20 modules with **Clang, GCC 14+, and MSVC (VS 2022)** — each with a permanent end-to-end CI guard — the **module registry is live** with `cmod search` returning real results, and the **VS Code extension shipped its first release**. Closes the full v0.1.0-alpha.4 milestone (#74).

---

## Upgrade notes

Three behaviour changes, all strict improvements:

- **`[toolchain] compiler = "gcc"` and `"msvc"` now build.** GCC drives `-fmodules-ts` with `.gcm` CMIs; MSVC drives `/interface` with `.ifc` BMIs (run from a VS developer environment — the error hints at vcvars otherwise). In alpha.3 these settings failed fast; before that they silently used clang.
- **`cmod add` accepts full URLs.** `cmod add https://github.com/owner/repo` — the paste-from-a-browser form — previously failed with a doubled scheme; it now normalizes to the canonical bare key.
- **`cmod publish` with `[publish] registry` actually publishes.** It previously edited a local cache clone and never pushed — every publication was silently discarded. Publishing now requires write access to the registry repository and fails loudly without it.

No manifest, lockfile, or cache-format changes; no cache invalidation this cycle.

## Highlights

### Three compilers, one abstraction

- `GccBackend`: single-pass `-fmodules-ts` compiles with module-mapper CMI placement, P1689 scanning via `g++ -fdeps-format=p1689r5` (same parser as the clang path).
- `MsvcBackend`: `/interface /TP` + `/ifcOutput` + `/reference name=path.ifc`, `cl /scanDependencies`, `lib.exe`/`link.exe` — with linker/archiver resolved *beside* `cl.exe` so Git Bash's coreutils `link` can never shadow it.
- Per-backend BMI extensions (`.pcm`/`.gcm`/`.ifc`) flow through the build plan; clang output paths are byte-identical to alpha.3.
- Both new backends are guarded by dedicated E2E CI jobs (ubuntu/g++-14 and windows/VS2022) that build and run a real module project on every PR.

### The registry is live

[`cmod-registry/index`](https://github.com/cmod-registry/index) now exists at the URL the client has always defaulted to, seeded with the nine validated [cmod-ecosystem](https://github.com/cmod-ecosystem) ports. `cmod search fmt`, `spdlog`, `json`, `catch2` return real results — online and via the offline cache. The publish path was fixed along the way (see upgrade notes). PR-based community submissions are next (#79).

### VS Code extension v0.1.0

First release: [vscode-v0.1.0](https://github.com/satishbabariya/cmod/releases/tag/vscode-v0.1.0) with platform VSIXes for 7 targets (each bundling the matching cmod binary) plus a universal VSIX. Marketplace listing follows once the publisher account exists (#52). The extension's npm dependency tree is `npm audit` clean.

### Hardening & fixes

- **Verified remote-cache restores**: downloads are hash-checked against entry metadata; truncated server-side files from interrupted uploads can never poison a build.
- **Windows CI fixed for real**: the "slow Windows legs" were cache evictions from 1,270 stale caches saturating the 10GB quota. With main-only cache saves, Windows test legs run ~2 minutes steady-state.
- Real-world validation sweep re-run against fmt/json/spdlog/Catch2: 4/4 green (after fixing the `cmod add` URL bug it caught, and repairing the spdlog ecosystem port).

## Stats

- 14 PRs merged over the milestone (#81–#89, #92–#94, plus ops work in the cmod-registry and cmod-ecosystem orgs)
- 873 tests passing (was 857 at alpha.3)
- 20-check CI matrix including E2E jobs for all three compiler backends
- Security: 0 open alerts across Dependabot (Cargo + npm), CodeQL, and secret scanning
