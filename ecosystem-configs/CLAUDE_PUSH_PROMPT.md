# Push cmod.toml to all cmod-ecosystem repos

You have `gh` CLI authenticated with write access to all repos under `github.com/cmod-ecosystem`.

## Task

For each repo listed below, push the corresponding `cmod.toml` file to a **`cmod-support`** branch. If the branch doesn't exist yet, create it from the repo's default branch.

## Repos and their default branches

| Repo | Default Branch | Config File |
|------|---------------|-------------|
| `cmod-ecosystem/boost` | `master` | `boost.cmod.toml` |
| `cmod-ecosystem/fmt` | `master` | `fmt.cmod.toml` |
| `cmod-ecosystem/json` | `develop` | `json.cmod.toml` |
| `cmod-ecosystem/spdlog` | `v1.x` | `spdlog.cmod.toml` |
| `cmod-ecosystem/googletest` | `main` | `googletest.cmod.toml` |
| `cmod-ecosystem/abseil-cpp` | `master` | `abseil-cpp.cmod.toml` |
| `cmod-ecosystem/Catch2` | `devel` | `Catch2.cmod.toml` |
| `cmod-ecosystem/CLI11` | `main` | `CLI11.cmod.toml` |
| `cmod-ecosystem/rapidjson` | `master` | `rapidjson.cmod.toml` |
| `cmod-ecosystem/folly` | `main` | `folly.cmod.toml` |

## Steps for each repo

1. Clone the repo (shallow): `git clone --depth 1 -b <default-branch> https://github.com/cmod-ecosystem/<repo>.git`
2. Create or checkout the `cmod-support` branch: `git checkout -b cmod-support` (if it already exists, use `git fetch origin cmod-support && git checkout cmod-support && git merge origin/<default-branch>`)
3. Copy the corresponding `.cmod.toml` file as `cmod.toml` in the repo root
4. Stage and check for changes: `git add cmod.toml && git diff --staged --quiet --exit-code cmod.toml`
5. If there are changes, commit with message:
   ```
   Add cmod.toml for cmod package manager integration

   Generated via `cmod migrate cmake` from existing CMakeLists.txt.
   Enables this library to be used as a cmod dependency.
   ```
6. Push: `git push origin cmod-support`
7. If no changes, skip and move to the next repo

## Important notes

- All config files are in the `ecosystem-configs/` directory relative to this file
- Push to the **`cmod-support`** branch, NOT the default branch
- If the `cmod-support` branch already exists on the remote, fetch it and update it rather than force-creating
- Skip repos where the clone fails (the fork may not exist yet)
- Print a summary at the end showing which repos succeeded, which were skipped, and which failed
