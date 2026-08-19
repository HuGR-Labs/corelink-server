# ADR — OCI manifest-resolution keystone (the moat's missing piece)

- **Date:** 2026-08-19
- **Status:** Proposed (campaign design; M0)
- **Owner-ratified scope:** build the manifest-resolution keystone so the
  cross-tenant `_public` OCI base-layer cache actually accelerates a real
  `docker build`. (F3.2 docker-CAS follow-on.)

## The honest problem statement (corrects a prior overstatement)

F3.2 shipped cross-tenant `_public` **blob** dedup and proved it **byte-exact at
the blob API** (a direct authenticated `GET /v2/library/alpine/blobs/<digest>`
returned the shared bytes to a tenant that never pushed alpine). That proof is
real but **narrow**: it proved blob-level dedup correctness, **not** that a real
`docker build` benefits.

It does not, today, because **manifest resolution is per-tenant only**:

- `crates/corelink-adapter-host/src/oci/pull/manifest.rs:47` — `get(kv, tenant,
  scope, repo, reference)` does `kv.get(tenant, &key)` and `.ok_or(NotFound)`.
  There is **no `_public` routing and no upstream on-miss**. The module header
  (`:4-5`) states it: *"tenant scoping implicit via the `tenant` arg — every
  `ManifestKvStore` impl per-tenant."*

A buildkit registry-mirror resolves an image **manifest-first**. For
`FROM debian:12` it does:

1. `GET /v2/library/debian/manifests/12` (tag) → an **OCI image index**
   (multi-arch manifest list).
2. pick the `linux/amd64` entry's digest → `GET …/manifests/sha256:<img>`
   (the per-arch image manifest).
3. read its `config` digest + `layers[]` digests → `GET …/blobs/<digest>` each.

Step 1 hits our per-tenant manifest store. A tenant that never pushed debian
gets **404 → buildkit fails open to docker.io → pulls the entire image from
docker.io → our promoted `_public` blobs are never requested.** The blob moat is
dead weight for un-pushed images. The already-promoted alpine layer only served
in the WP-R proof because that lease's tenant **already had alpine's manifest in
its per-tenant KV** (a prior push on that tenant) — it was not a cold
cross-tenant pull.

**Nothing is broken for users:** `docker build FROM <anything>` works today via
the WP-R buildkit mirror's fail-open to docker.io. What is missing is the cache
*engaging* — the acceleration + egress-saving + network-effect the moat promises.
This ADR builds that.

## De-risk (verified 2026-08-19, live against docker.io)

- Anonymous token dance works from the container's network position: `GET
  auth.docker.io/token?service=registry.docker.io&scope=repository:library/debian:pull`
  → 200 + JWT. (Same dance `DockerHubBlobFetcher::anon_token_for`,
  `public_mirror.rs:472`, already implements for blobs.)
- `GET /v2/library/debian/manifests/12` with `Accept:
  application/vnd.oci.image.index.v1+json` → `mediaType=…image.index.v1+json`,
  `Docker-Content-Digest: sha256:813017f3…`, entries incl `linux/amd64
  sha256:88a7d30d…`, `linux/arm64 sha256:62743235…`, **plus `unknown/unknown`
  attestation entries** (buildkit SBOM/provenance) that the resolver MUST skip
  (`platform.os == "unknown"`).

## What already exists (reuse, do NOT rebuild)

- `crates/corelink-container/src/routes/public_mirror.rs`
  - `fetch_verify_promote(fetcher, allowlist, moat, repository, digest,
    namespace, cap)` (`:337`) — allowlist-gated fetch→verify→`moat.put`,
    parameterized by namespace. The admin promote passes `(_public, Some(0))`.
  - `DockerHubBlobFetcher` (`:438`) — the SSRF-safe reqwest client
    (`ssrf_safe_redirect_policy`, fixed `registry-1.docker.io` origin) + the
    anon-token dance (`:472`) + `fetch_blob` (`:536`). **The token dance is
    manifest-reusable** — factor it out.
  - `validate_repository` (`:~395`) — origin-escape / path-traversal guard.
  - `POST /_internal/admin/public-mirror/promote` — the admin-gated blob promote.
- `crates/corelink-adapter-host/src/upstream_ssrf.rs` — `host_is_internal_ip`
  (`:25`), `ssrf_safe_redirect_policy` (`:68`). The ONE audited SSRF guard;
  reuse, never re-implement.
- `crates/corelink-container/src/routes/oci.rs`
  - `OciMoatStore` (`:218`) with `dedup: bool` + `allowlist` fields, and
    `routes_to_public(blob_key)` (`:286`) = `self.dedup &&
    allowlist.is_allowlisted(blob_key)` — the SINGLE predicate used on both read
    and write blob paths. `get_blob` (`:623`) already reads `_public`-first for
    allowlisted digests; its comment names WP-G as the on-miss follow-on.
  - `finalize_upload` write-routing (`:~575`) — `(namespace, cap) =
    if routes_to_public { (_public, Some(0)) } else { (tenant, cap) }`.
- `crates/corelink-container/src/public_flags.rs` — `oci_public_dedup_enabled()`
  boot-read (`"1"`-exact, default OFF). New flags follow this shape.
- `crates/corelink-container/src/public_base_allowlist.rs` — digest-pinned,
  fail-closed, `include_str!` baked manifest, `is_allowlisted(digest)` (`:85`).
- Flag threading: `worker/src/durable_object.ts:985` forwards
  `OCI_PUBLIC_DEDUP_ENABLED` into `container.start({env})`; `worker/src/index.ts`
  `Env` (`:245`); `wrangler.toml` per-region prod var blocks. Boot-read + mount
  gate in `crates/corelink-container/src/routes.rs:~897`.
- Singleflight template: `request_count.rs` `CachedTierResolver` (per-key
  async-mutex coalesce). Rate-limit: `routes/ratelimit_layer.rs` (`oci_bucket_key`).

## What is greenfield (this campaign)

1. **`UpstreamManifestFetcher`** — fetch a manifest by `repo` + `reference`
   (tag or digest), with the correct multi-media-type `Accept`, returning
   `(bytes, content_type, docker_content_digest)`. Reuses the extracted token
   dance + SSRF-safe client. Digest-reference fetches verify bytes→digest; tag
   fetches compute+return the digest (tags are mutable, see trust model).
2. **Manifest on-miss** in `oci/pull/manifest.rs::get` — on a per-tenant KV miss,
   if flag on, fetch upstream → verify/compute digest → store (per-tenant KV for
   the long tail; `_public` KV for an allowlisted image) → serve. Fail-open to
   404 on any error (preserves today's behavior).
3. **`_public` manifest routing** — an allowlisted image's manifest (and the
   per-arch manifests it indexes) resolve from `_public` first, shared
   cross-tenant. Requires an **image-level allowlist** (by manifest digest), not
   just the existing layer-blob allowlist.
4. **Transitive `_public` promote** — promoting an image to `_public` means the
   full closure: index + per-arch manifests + config + layer blobs, every one
   digest-verified. This is what makes a cold cross-tenant `docker build` serve
   entirely from CoreLink.
5. **Abuse controls** — singleflight (coalesce concurrent misses of the same
   ref), per-tenant rate-limit + size-cap on upstream fetches, per-tenant quota
   charge on per-tenant stores, and the `_public` anti-bloat ceiling (B4 — today
   `_public` writes are uncapped `Some(0)`; the promote path must bound growth).

## Security model (the poisoning surface is the manifest, not the bytes)

Content-addressing protects blob **bytes**. A **manifest is a pointer** — it
names blob digests. Two rules keep `_public` safe:

- **Per-tenant on-miss (A) needs NO allowlist.** Bytes land in the tenant's own
  namespace; only that tenant reads them; digest-verified on fetch. The gate here
  is **abuse** (rate-limit + size-cap + the tenant's own quota/`$`-ceiling),
  not trust. A tenant fetching a poisoned digest only poisons itself.
- **`_public` (B) is allowlist-gated by IMAGE manifest digest, owner-pinned.**
  An image enters `_public` only if its index/manifest digest is on the
  owner-curated allowlist; the promote then digest-verifies the FULL transitive
  closure before any `_public` write. No tag ever promotes to `_public` (tags are
  mutable → first-writer poisoning); only immutable digests. This preserves the
  writer-set invariant: `_public` writers stay `{tenant finalize_upload, admin
  mirror, THIS gated image-promote}`, all verify-before-write.

**Tag trust:** a tag reference (`debian:12`) is mutable. For **per-tenant** (A)
we fetch the tag, compute its digest, store both keys — trust = "we fetched from
the FIXED upstream over the audited SSRF client", scoped to the one tenant. For
**`_public`** (B) a tag is NEVER auto-promoted; the owner allowlists a specific
**digest**, and cross-tenant tag→digest resolution is out of scope (a build that
pins `FROM debian@sha256:…` hits `_public` directly; `FROM debian:12`
cross-tenant would require an owner-pinned tag→digest map, deferred).

## Milestones (each gate binary, proved by evidence)

- **M0 — this doc + de-risk + memory correction.** DONE when merged and the
  overstated "moat LIVE end-to-end" memory is corrected to "blob-API dedup
  proven; real-build moat pending this keystone."
- **M1 — per-tenant manifest + blob on-miss (A), flag-gated, INERT.**
  `UpstreamManifestFetcher` (extract token dance); manifest on-miss →
  per-tenant KV; blob on-miss → per-tenant moat (reuse the fetch→verify→put core
  WITHOUT the allowlist gate, per-tenant namespace + resolved cap); singleflight
  + rate-limit + size-cap + fail-open; new flag `OCI_UPSTREAM_ON_MISS` (default
  OFF). Unit + integration tests (on-miss caches; 2nd hit no fetch; upstream 404
  → 404; SSRF reuse; tenant-B isolation). Flag OFF ⇒ byte-identical to today.
  **Gate:** hermetic tests green; flag-off no-op proven; no new prod behavior.
- **M2 — `_public` cross-tenant image resolution (B), flag-gated, INERT.**
  Concrete design (locked 2026-08-19):
  - **Allowlist is digest-agnostic** — `public_base_allowlist.is_allowlisted(d)`
    is a flat `sha256:` set membership; the SAME allowlist gates an **image /
    index manifest digest** (M2) exactly as a layer-blob digest (M1's write path).
    No new allowlist type. Owner pins the **immutable index (or image-manifest)
    digest** of a base; tags are never pinned.
  - **Write gate (into `_public`)** — the ONLY way content enters `_public` from
    the read path: in the resolver, when the requested `reference` is a DIGEST
    and `is_allowlisted(reference)`, fetch → verify → promote the **full
    transitive closure** to `_public`: an index promotes its per-arch child
    manifests (skip `unknown/unknown`) + each child's config + layer blobs; an
    image manifest promotes its config + layers. Every descriptor is
    digest-verified against fetched bytes before any `_public` write. Trust flows
    from the ONE allowlisted root: the root pins exact child digests, children pin
    exact blob digests — the whole closure is content-addressed from the pinned
    root, so the children/blobs need NOT be individually allowlisted.
  - **Read gate (from `_public`) = EXISTENCE, not allowlist** — for a
    **by-digest** manifest/blob GET, check `_public` FIRST (through the same
    `MoatCache.get(PUBLIC_NAMESPACE, …)` that already applies the
    `public_blocklist` revocation filter); on a `_public` hit serve it, else fall
    back to per-tenant. Safe because (a) by-digest is content-addressed —
    `_public`'s copy is byte-identical to any private copy of the same digest, no
    leak; (b) the WRITE gate (allowlisted root + closure verify) is the sole
    admission control for what is IN `_public`; (c) revocation still filters on
    read. **By-TAG never reads or writes `_public`** (tags are mutable →
    first-writer poisoning). This REPLACES M1/inc6's allowlist-on-read for
    by-digest GETs so transitively-promoted children/layers (not individually
    allowlisted) serve cross-tenant. Flag-gated; OFF ⇒ per-tenant only.
  - **Anti-bloat ceiling (B4)** — `_public` writes are `Some(0)`-uncapped in
    `byte_accounting.rs:800`, but the `_public` per-region row DOES accrue
    `bytes_used`. Before a `_public` promote, read that row and REFUSE (fail-open
    to per-tenant / 404) once `bytes_used` exceeds a documented `_public`
    growth ceiling — bounds cross-tenant cost-contagion / mirror-amplification.
  - **Writer-set invariant preserved** — `_public` writers stay `{tenant
    finalize_upload, admin mirror, THIS resolver closure-promote}`, all
    verify-before-write.
  Cross-tenant isolation + adversarial e2e
  (`tests/e2e-user-journeys/.../oci_public_isolation.rs` extended): an allowlisted
  index → a second tenant that never pushed it is served the closure from
  `_public` (the cross-tenant proof); an UN-allowlisted image digest can NEVER
  enter `_public` (adversarial); a per-tenant private digest is never served to
  another tenant; revocation on a `_public` digest makes the read MISS.
  **Gate:** isolation suite green; adversarial no-poison green; flag-off no-op.
- **M3 — deploy + prove-by-use (one image roll, flags baked ON).** From a runner
  box, tenant that never pushed the image: cold `docker build FROM <curated>` →
  served entirely from CoreLink (manifest + config + layers), buildkit debug log
  shows **0 docker.io fallback refs**, byte-exact, cross-tenant. Then a long-tail
  `FROM <uncurated>` per-tenant-cached proof (2nd build faster, from our cache).
  **Gate:** MIRROR-HIT end-to-end on an UN-pushed image (the proof the P7 blob
  test could not give). **Rollback:** flags OFF via repin.

## Non-goals / deferred

- Registry write-through / push dedup changes (unchanged; `finalize_upload`
  routing stays).
- Cross-tenant tag→digest resolution for `_public` (owner-pinned tag map;
  deferred — digest-pinned `FROM` works, tag `FROM` cross-tenant does not).
- Non-docker.io upstreams (GHCR, quay) — the fixed-upstream origin stays
  `registry-1.docker.io`; a second upstream is a later, separately-audited flag.

## Rollback

Every milestone is flag-gated and boot-read; activation is a repin, not a live
flip (a running DO container keeps its boot env). Unset the flags + repin ⇒
exact pre-campaign behavior (per-tenant only, fail-open to docker.io).
