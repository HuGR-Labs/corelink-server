# WP-R — route runner base-image pulls through CoreLink (buildkit mirror) — HANDOFF

> **Status:** HANDOFF / not-yet-executed. This document is the direction for the
> **`corelink-runners`** repo. Nothing here is committed into `corelink-runners`
> from `corelink-server` — the runner-fabric owner/TL executes it there.
>
> **Why it matters:** WP-R is the ONLY thing that makes a customer's base-image
> pull actually traverse the CoreLink OCI cache. The F3.2 cross-tenant `_public`
> dedup (merged, inert behind `OCI_PUBLIC_DEDUP_ENABLED=OFF`) is a cache with no
> traffic until the runners point their base pulls at it. WP-R is a **hard
> dependency of the WP-E prod flip** and of the F3.3 "measure the wow" proof.

## 1. Current state (corelink-runners, verified 2026-08-19)

`deploy/runner/docker-shim.sh` lets a `runs-on: corelink` job run its **unmodified**
`docker build`. On the first docker invocation `_ensure_daemons()` spawns buildkitd:

```sh
buildkitd --addr unix:///run/buildkit/buildkitd.sock --oci-worker-snapshotter=overlayfs
```

There is **no `buildkitd.toml` and no registry mirror** — so every `FROM alpine`
/ `FROM node:20` pulls straight from `docker.io`. The cache is never in the path.

## 2. The change (in corelink-runners)

### 2a. Ship a `buildkitd.toml` registry mirror

Add `deploy/runner/buildkitd.toml` mapping Docker Hub to CoreLink's OCI registry
(`corelink-api.humangr.com`, the `/v2/` + `/token` two-leg HMAC-bearer surface —
`routes/oci.rs`, proven in F3.0/F3.1):

```toml
# Route docker.io base-image pulls through the CoreLink OCI cache.
[registry."docker.io"]
  mirrors = ["corelink-api.humangr.com"]

[registry."corelink-api.humangr.com"]
  # Public prod endpoint is HTTPS with a real cert — NOT insecure/http.
  http = false
  insecure = false
```

### 2b. Point buildkitd at it

In `_ensure_daemons()` add `--config /etc/buildkit/buildkitd.toml` (bake the file
into the runner image at that path via the runner `Dockerfile`):

```sh
buildkitd --addr unix:///run/buildkit/buildkitd.sock \
  --oci-worker-snapshotter=overlayfs \
  --config /etc/buildkit/buildkitd.toml
```

### 2c. Auth — the runner's `cas:rw` bearer

CoreLink's `/v2/` is authenticated (two-leg: `401` + `WWW-Authenticate: Bearer`,
token minted at `/token`, scoped to the tenant recovered from the bearer). buildkit
resolves registry creds from the standard docker cred store, so the runner must
`docker login corelink-api.humangr.com` (or drop a `~/.docker/config.json`) with
the **runner dogfood tenant** `cas:rw` PAT before the first build. The runner
already carries this tenant identity for the sccache/WebDAV + git-CAS dogfood
(tenant `d863fafb`); reuse it — do NOT mint a new namespace. The PAT is `cas:rw`
because a build both **reads** cache layers and (on the private-layer path)
**writes** the tenant's own layers back.

## 3. HARD DEPENDENCY — CoreLink must actually SERVE the base image

⚠️ **This is the load-bearing caveat.** Pointing buildkit at CoreLink only helps
for digests CoreLink can serve. Today:

- **WP-A admin mirror (merged, inert):** promotes *specific owner-allowlisted*
  layer-blob digests into `_public` — but only when an operator calls
  `POST /_internal/admin/public-mirror/promote`, and only for digests on the
  baked allowlist (which ships **deny-all** until WP-E).
- **WP-G read pull-through (DEFERRED):** the "fetch-from-upstream-on-miss" path
  was blocked on the frozen `BlobStore::get_blob` port signature and is NOT built.
  So CoreLink does **not** auto-fetch an un-mirrored base image on demand.

**Consequence:** until WP-G lands, a buildkit pull of an image whose layers were
never mirrored is a **MISS**. buildkit's mirror behavior must therefore
**fail-OPEN to `docker.io`** on a miss/error (this is buildkit's default when a
mirror 404s, but VERIFY it — a customer build must NEVER break because the cache
lacked a layer). Net: with WP-R alone, only **pre-mirrored** base images get the
speedup; everything else transparently falls through to Docker Hub.

**Therefore the pre-flip ordering is (GAP-E):**

1. **Pre-populate `_public`** — admin-mirror the base-image *layer-blob* digests
   we want accelerated (alpine, node, python, …) for every entry we intend to
   allowlist. (This is the `mirror-populated` dep of WP-E.)
2. **WP-R** — ship the buildkitd.toml mirror + auth to the runners.
3. **WP-E flip** — uncomment the allowlist pins + `OCI_PUBLIC_DEDUP_ENABLED=1` →
   fresh image → repin → `cf-deploy-prod` (the roll IS the flip).
4. **F3.3 measure** — a `runs-on: corelink` job that `docker build`s `FROM alpine`
   twice (cold, then warm from a *different* tenant) and shows the warm base pull
   served from CoreLink (`X-Cache: HIT`) + the pull-time delta.

## 4. Verification (prove-by-use, not unit test)

On a `runs-on: corelink` job after 2a–2c + a pre-mirrored `alpine`:

- `docker build` a trivial `FROM alpine` Dockerfile; capture buildkitd logs.
- Assert the layer-blob `GET /v2/library/alpine/blobs/sha256:…` went to
  `corelink-api.humangr.com` (not `registry-1.docker.io`) and returned `200` from
  cache. A second build from a **distinct tenant PAT** must serve the **same
  bytes** (cross-tenant `_public` dedup) — the F3.2 jaw-drop, proven by use.
- Negative: an un-mirrored image (e.g. a random tag) must still build (fall
  through to docker.io) — no customer breakage.

## 5. Risks / guardrails

- **Never break a build:** the mirror MUST fail-open to docker.io on
  miss/auth-failure/registry-down. Confirm buildkit's fallback empirically.
- **Auth token TTL:** the `/token` bearer is short-lived; buildkit re-mints per
  its cred flow — confirm long builds don't 401 mid-pull.
- **Tag→digest trust:** `_public` is keyed by immutable `sha256:` layer digests;
  the allowlist BANS tags. A mirror pull resolves a tag to a manifest first — that
  manifest resolution is NOT the `_public` path (only the layer blobs are). Keep
  manifest/config serving on the per-tenant/normal path; only allowlisted layer
  blobs dedup cross-tenant (see the WP-C red-team, `docs/design/2026-08-18-f32-inc5-redteam.md`).
- **Cost:** every base pull that traverses CoreLink is metered R2-GET work on the
  runner dogfood tenant — expected and cheap, but watch the `$`-ceiling on that
  tenant during the F3.3 measurement burst.

## 6. Owner decisions still open

- Green-light the WP-E prod flip (this doc's step 3) — owner's call, post red-team
  (red-team already `SAFE-TO-FLIP`, 0 blockers).
- Which base images to pre-mirror + allowlist first (alpine is the proven pin
  `sha256:25f1d6b1951ac8eb3740558fe94cb83d377bdadf95fd9f98b50d2e1b96130471`).
- Whether to prioritise building **WP-G read pull-through** (unblocks *every*
  base image automatically, not just pre-mirrored ones) before or after the first
  flip — it needs the `BlobStore::get_blob` port widened (a scoped follow-up).
