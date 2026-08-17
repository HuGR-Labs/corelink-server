# ADR: Edge async metering — take the quota trio off the warm hot path

- Status: Proposed (build behind a shadow flag; flip only after live parity proof)
- Date: 2026-08-17
- Extends: `docs/design/2026-08-16-adr-worker-native-public-cache-read.md` (F3.3 edge read/serve),
  `docs/design/2026-08-16-adr-edge-public-cache-invariants.md` (the edge-serve invariants), and the
  ADR-0070 three-tier auth-freshness cache family (pat / tsusp / tier / residency).
- Concept: `docs/knowledge/launch/edge-quota-tier-serving.md` (this ADR changes its serving mechanics).

## Context — the measured residual

After WP-A (colo Cache API L1) + WP-C (cached `_public` map gate) landed (#1130), a warm same-region
`_public` cache HIT measured **~44 ms server** from a US runner box — and that residual is **~100 % the
synchronous quota D1 round trip(s)** to the ENAM primary. The edge already caches the *tier* read
(`resolveTenantTierCached`, ADR-0070). What is still synchronous on the hot path, in `index.ts` after PAT
auth (the `resolvedTenantId !== "_anonymous"/…` block), is the rest of the **quota trio**:

1. **request-count metering** — `incrementMonthlyRequestCount` (`quota.ts`), an atomic
   `INSERT … ON CONFLICT … RETURNING request_count` UPSERT. One D1-PRIMARY write, always cross-colo for
   a non-US caller (writes never hit a replica).
2. **storage-cap SUM** — `checkStorageQuota` (`quota.ts`), a `SELECT SUM(bytes_used) …` read, run on
   **every** request of a finite-storage tier (i.e. every tier except `enterprise`), **including GET reads**.

Measured truth (this campaign): the ~44 ms dogfood number saw only statement (1) because the dogfood
tenant has unlimited storage, so `checkStorageQuota` early-returns without a SUM. **A real free/pro
customer pays BOTH (1) and (2) as two serial round trips** — roughly 2× the dogfood residual. Both must
leave the warm path to hit the campaign target (20–50 ms warm same-region, 100 ms hard ceiling).

`runQuotaBatch` (`quota.ts`) already batches (1)+(2) into **one** round trip and is the current warm path
(`index.ts`, the post-auth quota block) — so today's warm HIT is one D1 round trip, not two serial ones.
That is exactly the ~44 ms residual. This ADR keeps `runQuotaBatch` as the **exact-synchronous path** for
mutating requests and the near-cap fallback, and adds a cached/async fast path AROUND it for warm reads.

## Decision — cache the two verdicts, do the write off-path, keep exactness where it is load-bearing

### B1 — KV-cache the storage-cap verdict (mirrors the tier cache exactly)

Add `resolveStorageVerdictCached` (new `worker/src/lib/quota_storage_cache.ts`), an L1(5 s per-isolate) →
L2(`qstor:<tenant>`, 60 s Workers KV) → L3(`checkStorageQuota` D1 SUM) cache, byte-for-byte the shape of
`tenant_tier_cache.ts`. It caches the **verdict** (`ok` + the `total_bytes` it was based on), not raw bytes.

- **Correctness of bounded staleness.** `SUM(bytes_used)` changes only on a WRITE (PUT/POST fill or a
  DELETE/erase). A read cannot move it. So a ≤65 s stale storage verdict on a **read** can only be wrong in
  the direction of: a tenant who *just* crossed their storage cap via a write keeps being allowed to READ
  for ≤65 s. That is the same uniform ADR-0070 freshness window already accepted for tier/suspend, and it is
  customer-favorable (reads, which cannot grow storage, stay available slightly longer). It **never**
  wrongly 402s an under-cap tenant, because a fresh confirmed under-cap verdict is what gets cached.
- **Write verbs do NOT use the cache.** A PUT/POST (`isStorageMutating`) must see live storage before it is
  allowed to ADD bytes — a byte-adding op past the cap is the exact thing storage enforcement exists to
  stop. Mutating requests keep calling `checkStorageQuota`/`runQuotaBatch` live (uncached). Only
  non-mutating GET/HEAD reads consult the cache. This preserves the existing "reads fail-open, writes
  fail-closed" posture (`quota.ts` verb-aware error result) — the cache is a read-only accelerant.
- **Fail-open preserved.** A `d1Error` verdict is returned unchanged and **never cached** (identical to the
  tier cache's `d1Error` rule) — a transient fault can never pin a tenant to a wrong verdict.
- **No product change.** An over-storage tenant is **still 402'd on reads** (today's behaviour — verified at
  `index.ts` where `!storageCheck.ok → quotaExceeded`), just from a ≤65 s-fresh cached verdict instead of a
  per-request SUM. We are NOT removing read-side storage enforcement (that would be a product decision).

### B2 — request-count: async increment on the warm path, exact-sync near the cap

The counter cannot be a pure cached read — it must both **enforce** (429 at the cap) and **advance**. Split
by headroom:

- Maintain a KV verdict `qreq:<tenant>:<yyyymm>` = `{count, atMs}`, the last D1-observed monthly count,
  TTL 60 s. L1 mirror per isolate.
- **Hot-path decision** (non-mutating, non-fan-out request):
  - `headroom = tierCap - cachedCount`.
  - **Far under cap** (`headroom > BURST_MARGIN`): **serve immediately**; increment via
    `ctx.waitUntil(monthlyRequestCountStatement…)` (off the hot path); on completion refresh the KV
    `{count}` from the `RETURNING` value (optimistic; best-effort). No synchronous D1 on the hot path.
  - **Near cap or KV miss** (`headroom ≤ BURST_MARGIN`): fall back to the **exact synchronous path** —
    `runQuotaBatch` (one round trip: the UPSERT RETURNING count **and**, for a finite-storage tier, the
    storage SUM), then enforce `requestCapResultForCount(count, tier)` exactly as today. Refresh KV.
- **`BURST_MARGIN`** is the max requests a single tenant could plausibly issue within the KV staleness
  window across all isolates. Set conservatively (see Invariants) so that even a full-window lag cannot
  carry a tenant past `cap`. A tenant within `BURST_MARGIN` of the cap always takes the exact path, so the
  boundary is enforced exactly; only tenants comfortably under the cap get the async fast path.

### `runQuotaBatch` stays as the exact-synchronous path

`runQuotaBatch` remains exactly what it is today for: (a) every **mutating** request (a write must see live
storage before adding bytes), and (b) the B2 **near-cap / KV-miss** fallback (exact count when the boundary
is in play). Only NON-mutating reads that arm the fast path skip it. The existing 429-before-storage
ordering is preserved because an over-cap tenant is, by definition, in the near-cap branch and takes
`runQuotaBatch` unchanged. After a slow-path `runQuotaBatch`, the caller seeds the request-count KV from the
authoritative `RETURNING` count so subsequent reads for that tenant can arm the fast path.

## Rollout — shadow → parity → flip (the rigor gate; billing-global change)

The request-count is a **billing/quota counter**; making its write async **reverses a deliberate documented
choice** (the `runQuotaBatch` doc-comment kept it synchronous precisely because `waitUntil` has no
durability guarantee). That reversal is only acceptable with the fail-open properties below AND a live
parity proof:

1. **`EDGE_ASYNC_METER=shadow`** — compute BOTH: the exact synchronous verdict (served, today's behaviour)
   AND the B1/B2 cached/async decision (NOT served). Emit a structured divergence signal (verdict match,
   count delta) via `ctx.waitUntil`. Zero user impact. Reuse the F1 `shadowCompareEdgePublicRead` idiom.
2. **Prove parity on real traffic** — over N real requests across tenants, the cached decision must match
   the sync verdict 100 % on the ENFORCEMENT bit (allow vs 429), and the async count must converge (delta
   within the tolerated fail-open band, never OVER the true count). Proven by USE from a runner box, not by
   unit tests.
3. **`EDGE_ASYNC_METER=on`** — serve the cached/async decision; `runQuotaBatch` for mutating + near-cap.
4. **Rollback** — unset the flag → 100 % synchronous path (today's exact behaviour). Instant, per-region.

## Invariants (violating any = a billing or availability regression)

- **Never OVER-count.** The counter may under-count on a dropped `waitUntil` write (already-tolerated
  fail-open); it must never over-count (that would wrongly 429 a paying tenant). Optimistic KV refresh uses
  the D1 `RETURNING` value, never a blind local `+1` that could double-count across isolates.
- **The cap boundary is enforced EXACTLY.** Any tenant within `BURST_MARGIN` of its tier cap takes the
  synchronous `runQuotaBatch` path. The async fast path is entered ONLY with proven headroom, so no tenant
  is served past `cap + 0` on the exact path and past at most `cap + BURST_MARGIN` transiently on the fast
  path — and `BURST_MARGIN` is chosen so that transient band is smaller than one tenant's max in-window
  traffic, i.e. the overshoot self-limits and is customer-favorable (requests are a CAP, not a $-metered
  quantity — an over-cap request is a 429, never a charge).
- **Writes never trust a cached storage verdict.** PUT/POST resolve storage live (`runQuotaBatch`/live SUM).
- **`d1Error` results are never cached** (B1 and B2) — fail-open posture (F21) preserved verbatim.
- **Fan-out sub-requests never re-meter** — the existing `isFanout` constant-time check still gates the
  increment; a fan-out leg is `counted:false`, unchanged.
- **No new PII in logs** (INV-NO-PII-IN-LOGS) — the shadow divergence signal carries tenant id only if the
  existing telemetry already may (match it; prefer a hash/verdict-only line).

## Alternatives considered

- **Per-tenant Durable Object counter** (the original F3.3 pitch): exact, hibernates idle. Rejected as
  over-engineering for a **fail-open** HIT counter — a DO adds a colo hop and cold-start tail on the very
  path we are trying to shorten, to buy exactness this counter's design explicitly does not require. The
  KV-verdict + async-write + near-cap-sync split gets the latency win with a smaller, bounded correctness
  cost that is already tolerated.
- **Remove storage enforcement on reads entirely** (skip the SUM on GET/HEAD): would kill the latency with
  no cache, but it is a **product/billing behaviour change** (over-storage tenants could read freely). Out
  of scope — needs an explicit owner decision; B1 keeps today's behaviour instead.
- **Keep `runQuotaBatch` on the warm path** (the status quo — one sync round trip): correct and already
  batched, but a synchronous cross-colo D1 write on every warm cache HIT is precisely the ~44 ms residual
  the target rules out. The fast path routes reads around it while keeping it for writes / near-cap.
