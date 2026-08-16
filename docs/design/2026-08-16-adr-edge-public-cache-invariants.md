# ADR: Edge `_public` cache — bounded-revocation, $-ceiling exemption, and the multi-region (SAM) platform limit

- Status: Accepted
- Date: 2026-08-16
- Supersedes/extends: `docs/design/2026-08-16-adr-worker-native-public-cache-read.md` (the F3.3 edge read/serve).
- Context: the multi-region latency campaign moved the `_public` brew/pip cache-HIT read onto the Worker
  edge (WP-A colo Cache API + WP-C cached map gate, live on `corelink-prod`/iad). Three cross-cutting
  invariants of that path were previously implicit or undocumented; a brutal estate audit (2026-08-16)
  surfaced each. This ADR records the accepted decisions so they are stated design, not silent side effects.

## Decision 1 — Revocation of a `_public` blob is bounded to the map-gate cache TTL (~60 s), by design

The edge read gates every hit on the `adapter_cache_map` + `public_blocklist` lookup
(`worker/src/lib/edge_public_read.ts`, `lookupPublicContentHashCached`). That verdict is cached L1 (5 s,
per-isolate) + L2 (Workers KV `pubmap:<url_hash>`, 60 s — the KV TTL floor), mirroring the pat / tsusp /
tier / residency three-tier caches (ADR-0070) and the same uniform 60 s auth-freshness window the platform
already accepts for PAT revocation (ADR-0030, `SLO-FRESH-PAT-REVOKE ≤ 60 s`).

Consequence: a blob revoked (blocklisted / hard-deleted) at time *T* can continue to be served from a colo
that holds a warm positive verdict for **up to ≈ 65 s** (KV TTL + L1 slack). This is a **widening** of the
pre-edge behaviour, where every read ran the blocklist join live (revocation was effectively immediate).

Why this is accepted, not a defect:
- The bound is finite, uniform with the rest of the system's cache-freshness contract, and small.
- The **blob** cache (colo Cache API, 7-day TTL) does **not** extend the window: it is content-addressed and
  immutable, and every hit still consults the (short-TTL) map gate first — a revoked hash becomes a map miss
  and its now-unreferenced bytes simply age out. Revocation latency is the map-gate TTL, never the blob TTL.
- Re-hash-on-fill still self-heals corrupted/poisoned bytes to a MISS.

Bounded residual + tracked follow-up (NOT part of this ADR's accepted state — a future hardening):
- **Purge-on-revoke.** The container revoke path (`crates/corelink-container/src/routes/public_revoke.rs`)
  does not (and structurally cannot, being cross-plane) delete the Worker's `pubmap:<url_hash>` KV entries.
  Wiring an explicit purge (revoke → look up the affected `url_hash`es → delete the KV keys via a Worker
  internal endpoint) would collapse the window to the ~5 s L1 slack. This is worthwhile before the F3.4
  poisoning-defense milestone but is deliberately out of scope here; the 60 s bound is the accepted floor
  until then.

## Decision 2 — A `_public` cache HIT served from the edge is exempt from the container per-op $-ceiling

The Rust container brew/pip middleware charges an unconditional per-request $-ceiling
(`crates/corelink-container/src/routes/brew.rs`, `pip.rs`, ADR-0068). The edge-serve path bypasses the
container, so an edge-served `_public` HIT does not pass through that gate. The Worker's pre-serve
`runQuotaBatch` (`worker/src/lib/quota.ts`) enforces request-count and storage quota, but has no
dollar/spend-ceiling concept.

Decision: **this exemption is intended.** A `_public` read is shared, deduped, content-addressed, and
~free for us to serve (a colo-cache or R2 read, no upstream fetch, no compute). The product's promise is
that the cache is cheap and fast; charging the monthly spend cap against the cheapest, most-shared traffic
class is off-brand and customer-hostile. Request-count and storage quota still apply, so this is not an
un-metered free-for-all — only the per-op *dollar-ceiling* charge is waived, and only for `_public` GET
cache HITs.

Scope + limits:
- Applies only to `EDGE_PUBLIC_READ=serve` `_public` brew/pip **GET** HITs. MISS/write/private paths still
  go through the container and its $-ceiling unchanged.
- The un-charged amount is the flat per-op ceiling cost on the cheapest path — not "unlimited spend" (the
  audit's initial HIGH framing was over-dimensioned; the real exposure is a per-op flat charge on public reads).
- Recorded as an invariant at the serve site (`worker/src/index.ts`, the `EDGE_PUBLIC_READ==="serve"` block)
  and asserted by the edge serve tests, so it can never silently regress into an accidental bypass again.

## Decision 3 — Brazil / South America has no Cloudflare region; BR is served from US (and EU)

Verified via the Cloudflare API and code during the multi-region campaign:
- R2 location hints are only `WNAM / ENAM / WEUR / EEUR / APAC / OC` — there is **no South-America (SAM)
  location**. D1 has no SAM replica region either.
- Every `corelink-cas-{iad,sam,lhr,nrt,syd}` bucket is physically `location=ENAM` (US); only
  `corelink-cas-eu` (`jurisdiction=eu`, bound by the lhr worker) is physically EU. The "regional" bucket
  names for the US set are cosmetic.

Consequence: **Brazil-local cache storage is physically impossible on Cloudflare today.** The best a BR
client gets is service from the nearest physical region (US, or EU) — the same topology every major
registry (npm, PyPI, ghcr) uses. This is a platform ceiling, not CoreLink engineering debt. APAC *is*
achievable (R2 has an APAC location + D1 replicates APAC), so Japan/Australia can be made truly local by
recreating the nrt/syd buckets APAC-located (tracked as the campaign's WP4). SAM cannot, and no amount of
CoreLink work changes that until Cloudflare adds a SAM region.

## Alternatives considered
- **Charge the $-ceiling on the edge** (port a cheap ceiling check into the Worker): rejected per Decision 2
  — it contradicts the "cache is cheap" product stance and adds a hot-path cost to the cheapest traffic.
- **Zero-TTL / live map read on every edge hit**: rejected — it reintroduces the per-request D1 round trip
  the campaign exists to remove, for a revocation-latency gain already bounded to the system-wide 60 s window.
- **BR-local storage via a third-party (non-CF) region**: out of scope — the whole platform is Cloudflare;
  a split substrate for one country is disproportionate. Revisit only if Cloudflare ships a SAM region.
