# ADR 0004: Track included headers and dependency BMIs as build inputs

- Status: Proposed
- Date: 2026-10-01
- Found by: PR #131 ("Found on the way: header changes do not rebuild, even with `--force`")

## Context

A compiled object depends on more than its source file. Up to
v0.1.0-alpha.6, cmod tracked only part of that, in two places:

- `BuildState` (`.cmod-build-state.json`) decides whether a node is up to
  date.
- `CacheKeyInputs` decides which cached object a node may reuse. Keys are
  content-addressed and shared across projects and, through the remote
  cache, across machines.

Three inputs were missing from both:

1. **Included headers.** Neither hashed anything the source `#include`d.
   After editing a header, `cmod build` said "up-to-date". `cmod build
   --force` skipped the build state, went to the cache, and got back the
   object built with the old header. Any project with the same source file
   and flags got that object too.
2. **Dependencies rebuilt earlier in the same build.** `needs_rebuild`
   compared a node's recorded dependency hashes with the dependency's
   hashes in the *previous* build's state. A TU importing a module that was
   rebuilt a moment earlier still looked up to date. It kept the old inlined
   body until the build after that.
3. **BMIs of other packages.** `build_module_graph` dropped every import that
   is not a module of the package being built, so a TU's imports of git, path
   and workspace dependencies were never inputs. After a dependency's module
   changed, its importers stayed up to date indefinitely, and their cache keys
   stayed the same.

Each one links a binary built from old inputs, and the build reports success.
`crates/cmod-cli/tests/e2e_validation.rs` (group 29) reproduces all three. On
alpha.6 they fail with the old output (`left: "1 10"`, `right: "2 20"`).

## Decision

### Headers come from the compiler

Every compile asks the compiler for the files it read:

| Backend | Flag | Written to |
|---|---|---|
| Clang | `-MD -MF` (on the `--precompile` step for interfaces) | `<obj>.d` |
| GCC | `-MD -MF` | `<obj>.d` |
| MSVC | `/sourceDependencies` | `<obj>.json` |

`depfile.rs` parses both formats. The Make parser accepts the extra rules GCC
writes in module mode (`.PHONY`, order-only `:|`, `CXX_IMPORTS +=`). It drops
BMI entries, which are tracked as graph edges instead. System headers stay in:
a libstdc++ update can change them without changing the compiler version.

cmod does not scan `#include`s itself. Only the compiler knows the include
path, the macros and which branch of an `#if` was taken.

### Build state records headers

`NodeState::headers` holds the node's headers as indices into a table that
lists each header once (`BuildState::headers`), with its hash and mtime.
`needs_rebuild` checks each header in the table once per build: if the mtime
is unchanged it skips the file, otherwise it compares content hashes, so a
`touch` alone rebuilds nothing. An up-to-date node's record of a touched
header takes the new mtime, so it is hashed once, not on every build.

The table holds one entry per header *version* (path, hash and mtime), not
per path. A header edited during a build is seen at two versions: nodes
compiled before the edit and nodes compiled after it. A shared per-path
entry would make one group look built against the other's version. A node with no recorded headers rebuilds.
That covers state files from older cmod, which have no `headers` key, and
nodes built without a header list (distributed workers, precompiled BMIs).

Dependency hashes are now hashed from the dependency outputs on disk,
memoized per build, instead of being read from the previous state. The
scheduler runs a node only after its dependencies, so the files are final by
then.

### Cache keys cover headers (ccache's "direct mode")

The headers are only known after compiling, but the key is needed before. As
in ccache, each key has two levels:

- The **base key** is the old key: source, dependency BMIs, compiler and
  flags.
- The **include manifest**, stored under `base.include_manifest_key()`,
  lists the header sets seen when compiling that base key, each as
  `(path, content hash)` pairs. It keeps the 16 most recent sets.
- **Artifacts** are stored under `base.with_headers(set)`. That key hashes
  the base key with every `(path, hash)` pair in the set.

A lookup reads the manifest and re-hashes each set's headers on disk. It tries
the key of the first set that still matches. A manifest is a hint and never a
source of truth: the artifact key is computed from local file contents, so a
corrupt or forged manifest can only cause a miss.

A header edited while the compile ran makes the header list untrustworthy:
the hash cmod reads may not be what the compiler read. As in ccache, if a
header's mtime is at or after the compile's start, or changed since cmod
first hashed it in this build, the header set counts as unknown. The object
is not cached and the node rebuilds next time. Each header's mtime is read
before its content is hashed, so a recorded (mtime, hash) pair never
describes two different versions of the file.

Header paths inside the package (the nearest directory with a `cmod.toml`)
are stored relative to it, so two checkouts in different places share
entries. Other headers are stored as absolute paths.

With a remote cache, the manifest is fetched when the local one has no
match. On store, it is merged with the remote copy before upload, so sets
from other machines are kept.

### Dependency BMIs are inputs

`ModuleGraph::external_imports` keeps the imports that `build_module_graph`
used to drop. `BuildNode::external_imports` carries them into the plan, and
the runner hashes the matching dependency BMI into the node's dependency
hashes, which feed both the incremental check and the base key.

## Consequences

- **The first build after upgrading recompiles everything.** Old state files
  have no header lists, and old cache entries sit under base keys, which no
  longer hold artifacts. Lockfile format unchanged.
- **Without a header list, nothing is cached.** If a compile writes no
  dependency file, its artifacts are not stored, because no key would be
  correct. The node also rebuilds on every build. Today this applies to
  distributed compiles and to MSVC builds whose `/sourceDependencies` output
  cannot be read. Correctness wins over speed here.
- **Cost.** One manifest read per cache lookup, plus hashing each distinct
  header once per build. Release builds of a Catch2 consumer (107 TUs, all
  restored from cache on every build, because dependency `obj/` directories
  are cleared) take the same time as before within noise: ~1.1 s with this
  change, 1.0 to 1.3 s on alpha.6.
- **`cmod plan` JSON** gains `external_imports` on nodes that import other
  packages' modules. The field is omitted when empty.
- **The runner now uploads `metadata.json`.** It used to upload artifacts
  only, and restores require metadata, so a runner-pushed entry could never
  be restored by another runner. It is uploaded last, so a reader never sees
  metadata before the artifacts it describes.

## Not done

- **Precise external imports.** `external_imports` covers the BMIs a TU
  names directly. A change deeper in a dependency reaches a TU through the
  dependency's own BMI, which is rebuilt.
- **Header-unit imports (`import <vector>;`).** cmod does not build header
  units today.
