# ADR — colo Cache-API L1 + cached map gate for the `_public` edge read

- **Status:** ACCEPTED — delivered and proven in prod. Closes the "ADR to follow" promised
  by the WP-A/WP-C change (PR #1130).
- **Date:** 2026-08-19
- **Campaign:** F3.3 (measure/deliver the jaw-drop). Follows
  [`2026-08-16-adr-worker-native-public-cache-read.md`](./2026-08-16-adr-worker-native-public-cache-read.md)
  (the edge-serve decision) and
  [`2026-08-16-adr-edge-public-cache-invariants.md`](./2026-08-16-adr-edge-public-cache-invariants.md)
  (revocation + `$`-ceiling invariants).

## Context — the edge-serve path alone did NOT meet the `<100 ms` bar

The prior ADR moved `_public` brew/pip cache HITs off the container onto the Worker edge
(`EDGE_PUBLIC_READ="serve"`, native `CONFIG_DB` map read + `CAS_BUCKET` R2 read). That removed
the ~585 ms `origin` container round-trip, but edge-serve **alone** still paid, per HIT:

| phase | cost | why |
|---|---|---|
| `wdb` (map read) | ~159 ms | `adapter_cache_map` over D1-over-HTTP |
| R2 GET (`ostore`) | ~50 ms | native `CAS_BUCKET.get` of the blob |
| metering D1 write | ~54 ms | per-request request-count UPSERT |

Measured on a real US runner box (colo ATL): **warm HIT ~120 ms server / ~190 ms wall** — the
`<100 ms` product bar was **NOT met by edge-serve alone**. The residual floor is the two
D1-over-HTTP hops (the map read and the metering write) plus the per-HIT R2 GET.

## Decision — a proper multi-tier edge cache in front of both the map and the blob

Mirror the 3-tier PAT-verify cache (`auth/pat-moat`) for the cache read itself. Two independent
tiers, both flag-gated behind the existing `EDGE_PUBLIC_READ`, both pure optimizations that
degrade to the D1/R2-direct path when absent (so node tests and any cold isolate stay correct):

1. **Map gate cache (WP-C, `lookupPublicContentHashCached`).** The `_public` map lookup gains
   an **L1** (per-isolate, 5 s) + **L2** (`METADATA_KV`, key `pubmap:<url_hash>`, 60 s = the
   bounded revocation window) cache. It caches BOTH the positive verdict (`content_hash`) and
   the negative one (miss/revoked), **never** caches a D1 fault, and **is the revocation gate** —
   the 60 s L2 TTL is exactly the `_public` edge revocation window (see the invariants ADR / B1b).

2. **Colo blob cache (WP-A, Cloudflare Cache API).** A per-colo Cache-API entry sits in front of
   R2 (now the origin-of-record), **keyed by the content identity (`content_hash`), NEVER the
   PAT**, so every tenant in a colo shares one entry. A colo hit serves fill-validated bytes
   **without re-hashing** — the key IS the hash and only our re-hash-validating fill ever writes
   the entry. The immutable blob carries a long TTL because **revocation is enforced upstream at
   the short-TTL map gate**, not on the blob.

**Re-hash-relocation-to-fill** (owner-approved): on a colo miss the Worker reads R2, re-verifies
`blake3(bytes) == content_hash`, serves, and fills the colo cache with the validated bytes. A
mismatch is treated as a MISS (self-heal) and falls through to the container. The colo cache is
therefore never load-bearing for correctness — only for latency.

## Consequence — the `<100 ms` DoD is MET, proven by use

Re-measured 2026-08-19 from a real `runs-on: corelink` box (tenant `3c7d77b1`, Option-C mint),
14 sequential GETs of the known `_public` `tree` bottle
(`/brew/<tenant>/v2/homebrew/core/tree/blobs/sha256:cb6d74ec…`, 77998 B), all `200` +
`X-Cache: HIT` + Server-Timing **`origin` phase ABSENT** (the wire-level proof the container was
bypassed):

| condition | wall p50 | server total p50 | notes |
|---|---|---|---|
| **warm HIT** (13 samples) | **43 ms** (p90 55, max 72) | **8 ms** (max 40) | colo-cache hit; no R2, no D1 map read |
| cold first HIT (per colo) | 644 ms | 570 ms | one-time colo Cache-API fill from R2 |

**Warm HIT p50 43 ms wall / 8 ms server — the `<100 ms` product bar is met with large margin.**
Only the first caller per colo per artifact pays the ~570 ms cold-fill; every subsequent caller in
that colo is served in ~8 ms server-side. `auth;dur=0;desc="l1"` on the same responses confirms the
PAT-verify cache is also warm (~0 ms), so the whole HIT is edge-native.

## Invariants preserved (unchanged from the invariants ADR)

- The colo blob key is the **`content_hash`, never the PAT** — cross-tenant sharing is the moat,
  and a PAT-keyed entry would defeat it. Access is still gated: the map read (behind its own cache)
  runs first, and PAT verify runs before that.
- Revocation stays enforced at the **short-TTL map gate** (L2 KV 60 s); the long-TTL immutable blob
  cache is safe because a revoked `content_hash` stops resolving at the map, so the blob is never
  reached.
- The colo cache only ever holds **re-hash-validated** bytes (only the validating fill writes it);
  absent the Cache API the path degrades to R2-direct, so correctness never depends on it.
- MISS / revocation / re-hash mismatch / any fault ⇒ fall through to the container path, unchanged.

## Re-measure recipe

One-shot workflow on `HuGR-Labs/corelink-cold-organic-e2e` (Option-C → cold tenant): redeem the
env-0 cred-ticket (`POST $CLW_FABRIC_ENDPOINT/v1/leases/$CLW_LEASE_ID/cas-cred`, body
`{"ticket":"$CLW_CRED_TICKET"}` → `.cas_pat` + `.clw_tenant`), then `curl` the bottle N× reading
`Server-Timing` (server ms) and `-w %{time_total}` (wall). Delete the workflow after use.
