# Design Note: Remote-Cache Resilience (Partial Transfers)

**Status:** Decided 2026-07 (issue #62). Phase 1 implemented; phases 2–3
recorded with explicit triggers.

## The actual failure modes

Issue #62 asked for "resumable/partial uploads." Examining the transfer
paths shows three distinct concerns with very different costs:

1. **Interrupted PUT leaves a truncated artifact on the server.** nginx's
   `dav_methods PUT` writes in place — a disconnect mid-upload leaves a
   partial file that later serves with `200`. This is a **correctness**
   hole: a consumer restores a truncated PCM/object.
2. **Interrupted GET.** The client buffers the whole body and writes
   atomically (`.part` + rename, #63) — an interrupted download leaves
   *nothing*; the existing retry (3 attempts, backoff) re-requests. There is
   no partial state to resume, only wasted bytes on retry.
3. **Upload resume for large artifacts.** Pure efficiency; matters only
   when individual artifacts are large enough that restarting a PUT hurts
   (≳100 MB — LTO objects in very large modules).

Artifacts today are KB–MB scale (an entire example project's cache is
~600 KB), so 2 and 3 are efficiency questions with no current pain, while
1 corrupts builds *today* given one flaky uplink.

## Decision

### Phase 1 — verified restores (implemented)

Every cache entry's `metadata.json` already records per-artifact SHA-256
and size (`CachedArtifactEntry`). The remote-restore paths now use it:

- the build runner fetches `metadata.json` before artifacts on a remote
  hit and verifies each downloaded artifact's hash; any mismatch treats
  the whole entry as a miss (partial server files are never used, never
  stored locally);
- `cmod cache pull` performs the same verification.

This **neutralizes the harm of interrupted PUTs** with zero protocol or
server changes: a truncated server artifact fails verification, the client
compiles locally, and the next successful push overwrites the stump.

### Phase 2 — streaming + Range resume for GET (deferred)

Plain servers (nginx, Caddy) support `Range` natively, so download resume
needs no protocol extension — but it requires switching from
buffer-then-write to streaming-to-`.part`, keeping partials on network
error, and `Range`-continuing on retry. **Trigger:** artifact sizes where
re-downloading on retry is measurably painful (≳50–100 MB), i.e. large-scale
LTO/monorepo adoption. Until then the added state machine is not worth it.

### Phase 3 — resumable PUT (rejected for now)

Every resumable-upload mechanism (TUS, S3-style multipart, chunk+finalize
conventions) requires **server-side logic**, which breaks the project's
"any static file server" promise from `docs/guide/remote-cache.md`. With
phase 1 closing the correctness hole, an interrupted PUT costs only a
retried upload. **Trigger to revisit:** a real deployment with individual
artifacts ≳100 MB over unreliable links; the likely design then is an
optional TUS extension negotiated via `OPTIONS`, keeping plain servers
first-class.

## Non-goals

- WebDAV `MOVE`-based two-phase PUT (atomic server-side): tempting for
  nginx, but unsupported by other backends and unverifiable by the client;
  phase 1 verification covers the same failure at the consumer, which is
  where it matters.
