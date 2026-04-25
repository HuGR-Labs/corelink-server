---
id: AUDIT-SONNET-R5-S07-WI-REVIEW
parent_audit: specs/_audits/2026-04-25-agent-r4-s07-wi-review.md
sprint_contract: specs/04_sprints/S07/_spec_contract.md v1.1.0
tags: [audit, sota, lote-10.7, s-07, sonnet-r5, independent]
reviewer: Sonnet 4.6 (independent — round 5; different model lineage than Opus R4)
date: 2026-04-25
commit_reviewed: e8b829b
---

# Sonnet R5 — Lote 10.7 S-07 Independent Adversarial Review

## 0. Executive Summary

**SOTA score: 6.8/10.**

Five WIs are well-structured, lessons-absorbed, and cover the right territory. The
architecture choices (DO actor, reservation pattern, batch coalescing) are defensible.
However, there are **3 structural P0 defects** that Opus's API-semantics analysis
likely missed: (1) the UNIQUE INDEX on `(tenant_id, chunk_digest)` in `manifest_chunks`
is semantically incorrect — it blocks legitimate dedup by preventing two different blobs
from sharing the same chunk; (2) the 60-second quota reservation TTL is shorter than a
160 GiB multipart upload in the presence of slow networks, creating a confirmed
over-quota window; (3) `wasm_bindgen_futures::spawn_local` is cited as the fire-and-forget
mechanism for LRU hot-path updates but this API does not exist in the Cloudflare Workers
Rust runtime, making the hot-path implementation spec unshippable as written.

Additionally, there is a **P1 INV count divergence** between WI-S07-005 and the registry
(WI-005 claims 3 NEW but registry §3.18 registers 5), a **P1 statistical methodology gap**
(property tests claimed "concurrent" but use sequential proptest harness by default), and a
**P1 metrics cardinality bomb** (`bytes_reclaimed_total{tenant_id, tier}` at 100k tenants
reaches 500k series, blowing past INV-OBS-CARDINALITY-BUDGET).

What Opus likely missed: runtime/deployment constraints, quota TTL arithmetic against real
upload durations, the UNIQUE INDEX semantic inversion, and the proptest concurrency illusion.

---

## 1. Per-WI Scores

| WI | Cripto / Correctness | Completeness | Clarity | SOTA | Internal Consistency | Cross-WI Consistency | Overall |
|---|---|---|---|---|---|---|---|
| WI-S07-001 | 6/10 | 8/10 | 9/10 | 8/10 | 7/10 | 7/10 | **7.5/10** |
| WI-S07-002 | 7/10 | 8/10 | 9/10 | 8/10 | 7/10 | 7/10 | **7.7/10** |
| WI-S07-003 | 6/10 | 8/10 | 9/10 | 8/10 | 6/10 | 7/10 | **7.3/10** |
| WI-S07-004 | 5/10 | 7/10 | 8/10 | 7/10 | 5/10 | 6/10 | **6.3/10** |
| WI-S07-005 | 7/10 | 7/10 | 9/10 | 8/10 | 5/10 | 6/10 | **7.0/10** |

**Score rationale**: WI-S07-001 penalized for the UNIQUE INDEX P0. WI-S07-003 penalized
for the TTL arithmetic P0. WI-S07-004 penalized for the spawn_local runtime P0 plus the
LRU UNION race window mis-characterization. WI-S07-005 penalized for INV count divergence
and missing SLO-DEDUP-RATIO lifecycle gap.

---

## 2. P0 Findings

### P0-1 (NEW) — UNIQUE INDEX on `(tenant_id, chunk_digest)` inverts dedup semantics

**File**: WI-S07-001 §6.1.2, §1 SQL block, sprint contract §5 R-S07-1
**Classification**: NEW (not in Opus API-semantics review; this is a schema semantics bug)

**Finding**:

```sql
CREATE UNIQUE INDEX idx_manifest_chunks_tenant_digest
    ON manifest_chunks (tenant_id, chunk_digest);
```

The `manifest_chunks` table stores `(blob_digest, chunk_index, chunk_digest, tenant_id)`.
A single blob is decomposed into N chunks. The *entire point of dedup* is that two
**different blobs** (blob_digest=B1 and blob_digest=B2) can share the same chunk_digest
(same chunk bytes appear in both). This is the content-addressable dedup benefit.

A `UNIQUE INDEX` on `(tenant_id, chunk_digest)` means: for a given tenant, each
`chunk_digest` may appear in `manifest_chunks` only **once**. But after dedup:
- Blob B1 with chunk C → `INSERT (blob_digest=B1, chunk_index=0, chunk_digest=C, tenant_id=T)` — OK.
- Blob B2 also uses chunk C → `INSERT (blob_digest=B2, chunk_index=0, chunk_digest=C, tenant_id=T)` — **UNIQUE CONSTRAINT VIOLATION**.

This makes cross-blob chunk sharing impossible, which is **the core value proposition of
CAP-DEDUP-001**. The chaos test scenario 6 ("UNIQUE constraint violation attempt") will
fire on every legitimate dedup use case, not just on bugs.

The spec's own rationale (§1 comment) states "same chunk_digest → same R2 object reused"
— this requires multiple `manifest_chunks` rows to reference the same chunk_digest for
different blobs. A UNIQUE INDEX prevents exactly that.

**What should exist**: `INDEX (NOT UNIQUE) ON manifest_chunks(tenant_id, chunk_digest)` for
O(1) lookup, plus the existing S-05 `(blob_digest, chunk_index)` UNIQUE to prevent duplicate
chunk positions within a single blob assembly.

**INV-DEDUP-CONSISTENCY** states `(tenant_id, chunk_digest) → chunk_body` is 1:1, which
is naturally true because BLAKE3 is collision-resistant — the same digest always maps to
the same bytes. This does NOT require a UNIQUE database constraint; the cryptographic
property is the guarantee, not the index uniqueness.

**Impact**: The migration `00X_dedup_index.sql` as written would cause `SQLITE_CONSTRAINT`
errors on every second upload of a workload with layer reuse. The dedup ratio metric would
read 0× (all second-blob inserts fail, nothing is actually deduplicated). This is a
latent production-break defect.

**Fix**: Drop UNIQUE, keep as plain INDEX. Add property test `prop_two_blobs_share_chunk`
that asserts two separate `manifest_chunks` rows with same `(tenant_id, chunk_digest)` but
different `(blob_digest, chunk_index)` can coexist.

---

### P0-2 (NEW) — Quota reservation 60s TTL too short for 160 GiB multipart uploads

**File**: WI-S07-003 §6.1.4, §1 (reservation pattern), §2 narrative
**Classification**: NEW (arithmetic gap; deployment context)

**Finding**:

The reservation pattern (check_and_reserve → commit_reservation within TTL=60s) is designed
to close FM-059. But the TTL is fixed at 60 seconds, and the middleware is mounted on:
- WI-S05-001 `SplitBlob` handler (per WI-S07-003 §6.1.2): `request_bytes = blob.size_bytes`

S-05 supports blobs up to 160 GiB (INV-MULTIPART-BOUNDED-PARSER). At a realistic
enterprise upload rate of 100 Mbps, a 160 GiB blob takes:

```
160 GiB × 8 bits/byte ÷ 100 Mbps ≈ 13,107 seconds ≈ 218 minutes
```

Even at 1 Gbps: `160 GiB ÷ 125 MB/s ≈ 1,311 seconds ≈ 22 minutes`.

The 60s TTL auto-release fires **long before** the write completes. The DO alarm will
release the reservation, the bytes are no longer counted as pending. A second concurrent
upload can then acquire a new reservation. Both uploads eventually commit. Result: the
tenant is over quota by up to 1× the blob size (e.g., 160 GiB over quota for enterprise).

The spec narrative (§2) acknowledges "write crashes; reservation expires in 60s and is
auto-released" as the happy case. It does NOT address the case where the write is in
flight and taking longer than 60s legitimately.

**No mitigation exists in the spec.** The existing chaos test #4 only tests "write crashes"
not "write takes 5 minutes". The property test `prop_quota_reservation_lifecycle` tests
check → commit OR release, not check → 60s TTL fires → commit (race).

**Fix options** (spec must pick one):
1. **Streaming reservation extension**: handler calls `extend_reservation(id, +60s)` every
   30s while upload is in flight. Adds complexity but solves correctness.
2. **Per-tier TTL floor**: minimum TTL = max(60s, estimated_upload_duration_by_request_size).
   E.g., for request_bytes > 1 GiB, TTL = 3600s.
3. **Pre-commit byte check**: before physical R2 PUT, verify reservation still valid; if
   expired, re-acquire (with re-check against quota). Acceptable for chunked uploads.

Until this is addressed, INV-QUOTA-ENFORCEMENT cannot be claimed green for large blobs.

---

### P0-3 (NEW) — `wasm_bindgen_futures::spawn_local` does not exist in Cloudflare Workers Rust runtime

**File**: WI-S07-004 §6.1.4, §2 narrative
**Classification**: NEW (runtime/deployment context; Opus API analysis misses this)

**Finding**:

WI-S07-004 §6.1.4 states:

> Spawned via `wasm_bindgen_futures::spawn_local` ou `tokio::spawn` (NOT awaited;
> latency neutral).

Neither is correct in the CF Workers Rust runtime:

1. `wasm_bindgen_futures::spawn_local` — This API exists in `wasm-bindgen-futures` for
   browser WASM environments running in a JavaScript event loop. CF Workers does NOT use
   the browser WASM event loop model. The `spawn_local` function maps to
   `wasm_bindgen::closure::Closure` + `Promise` chaining in a browser context. In CF
   Workers, there is no ambient JS microtask queue that `spawn_local` targets. Calling it
   would either panic (no executor) or be a no-op depending on the Workers runtime
   version. As of Q4 2025 CF Workers Rust (via `worker` crate 0.3+), there is no
   `wasm_bindgen_futures::spawn_local` equivalent that works for fire-and-forget async
   in the request handler context.

2. `tokio::spawn` — CF Workers does not run a Tokio runtime. Workers uses either the
   `worker` crate's async adapter (which is Futures-based, not Tokio) or direct
   `wasm_bindgen_futures::JsFuture`. `tokio::spawn` requires a Tokio reactor which is
   absent.

**What actually works** in CF Workers for fire-and-forget async:
- Schedule a DO alarm (`do_stub.schedule_alarm(timestamp)`) and have the DO handle the
  LRU batch flush on alarm fire — this is exactly what WI-S07-004 §6.1.3 describes for
  the DO flush! The issue is *initiating* the record_access from the handler.
- Use `worker::send_future` (or equivalent from the `worker` crate) which registers a
  future to complete after the response is sent. This exists in CF Workers Rust as of
  `worker` crate 0.2+.
- Send an HTTP request to the DO stub (non-blocking) and let the DO handler write to its
  buffer. This is the correct pattern for fire-and-forget to a DO.

**Impact**: If the implementation follows the spec literally, it will fail to compile or
panic at runtime. The fire-and-forget mechanism is unspecified in any correct form,
meaning the hot-path integration (§6.1.4) is unimplementable as written.

**Fix**: Replace "wasm_bindgen_futures::spawn_local ou tokio::spawn" with:
"DO stub stub.fetch() non-awaited via `worker::send_future(do_stub.fetch(req))` OR
`.then()` JS Promise chain via `wasm_bindgen_futures::JsFuture::from(promise)` with
response discarded." Add cargo feature gate ensuring `tokio` is NOT in `[dependencies]`
for the worker crate (to catch accidental import).

---

## 3. P1 / P2 / P3 Findings

### P1-1 (NEW) — INV count divergence: WI-S07-005 claims 3 NEW INVs; registry §3.18 has 5

**File**: WI-S07-005 §1 (intent), §6.1.7, §12; invariant_registry.md §3.18
**Classification**: NEW; process gap at ship gate

**Finding**:

WI-S07-005 title, §1, §6.1.7, and §8 (Gherkin "Cumulative INV §3.X promotion") all
consistently state **"3 NEW"** INVs:
- INV-EVICT-SOFT-DELETE-FIRST
- INV-EVICT-CASCADE-PREVENTED
- INV-LRU-CONSISTENCY

But invariant_registry.md §3.18 (already committed per e8b829b) registers **5 INVs**:
- INV-EVICT-SOFT-DELETE-FIRST
- INV-EVICT-CASCADE-PREVENTED
- **INV-EVICT-TTL-CAP-RESPECTED** ← NOT in WI-005 list
- INV-LRU-CONSISTENCY
- **INV-QUOTA-RESERVATION-TTL** ← NOT in WI-005 list

WI-S07-002 §12 declares INV-EVICT-TTL-CAP-RESPECTED as "NEW promovida". WI-S07-003 §12
declares INV-QUOTA-RESERVATION-TTL as "NEW promovida". Both should be in WI-005's
cumulative promotion list.

**Impact**: `validate_inv_promotion.py` CI gate (WI-005 §6.1.7; sprint contract §6 DoD
"cumulative INV §3.X promotion") compares WI-declared INVs against the registry. If
validate_inv_promotion checks in the other direction (WIs → registry), it will pass
(registry has all 5). But if WI-005 generates the "3 NEW" list to validate against
the registry, the validator will expect 3 but find 5, or vice versa. The ship gate
scenario §8 Gherkin "validate_inv_promotion.py runs" will produce an ambiguous result
unless the validator logic is clarified. The existing sprint contract §6 DoD says
"Property test 10k iter verde cobrindo GC+Evict race" — the INV count in the ship gate
checklist is mismatched and will cause confusion at sign-off.

**Fix**: Update WI-S07-005 §1, title, §6.1.7, and Gherkin to say "5 NEW" INVs and list
all five. Update `s07-ship-gate.yml` to validate all 5.

---

### P1-2 (NEW) — Metrics cardinality bomb: `bytes_reclaimed_total{tenant_id, tier}` at scale

**File**: WI-S07-002 §6.1.10 metrics, WI-S07-003 §6.1.10 metrics; invariant_registry.md §3.12
**Classification**: NEW (statistical/ops; Opus depth-first API analysis misses this)

**Finding**:

WI-S07-002 declares:
```
corelink.evict.bytes_reclaimed_total{tenant_id, tier}
```

WI-S07-003 declares:
```
corelink.quota.committed_bytes_total{tenant_id}
corelink.quota.released_bytes_total{tenant_id}
corelink.quota.utilization_pct{tenant_id}
corelink.quota.95pct_breach_total{tenant_id}
corelink.quota.100pct_breach_total{tenant_id}
```

At 100k tenants × 5 tiers = **500,000 unique series** for `bytes_reclaimed_total` alone.
The quota metrics with `{tenant_id}` alone = 100k series × 5 metrics = 500k additional
series. Total from S-07 alone: ~1M cardinality.

INV-OBS-CARDINALITY-BUDGET (§3.12, S-09): "Nenhuma métrica excede 20k séries únicas;
total ≤ 100k." This invariant is **violated by design** in S-07 WI-002 and WI-003.

The WI-S07-005 DASH-DEDUP partially acknowledges this ("top-50 tenants only em heatmap")
but this is a dashboard-layer mitigation, not a metrics-layer fix. The counters are
emitted globally regardless.

**Fix**: Replace `{tenant_id}` labels with `{tenant_tier}` for cardinality-bounded
global aggregates. Use a separate high-cardinality store (Grafana Mimir tenant-level
aggregations, or CF Analytics Engine) for per-tenant breakdowns. Cap the Prometheus
scrape to aggregate-only. This requires ADR update for observability model.

---

### P1-3 (NEW) — Property test `prop_quota_atomic_no_race` is sequential, not concurrent

**File**: WI-S07-003 §6.1.11 property tests, §10.s07.003.4 chaos test "1000 concurrent"
**Classification**: NEW (statistical methodology; Opus misses runtime harness constraints)

**Finding**:

`prop_quota_atomic_no_race` is described as "1000 concurrent check_and_reserve at
boundary 99%". In a standard `proptest` harness, strategies generate test cases
**sequentially** — there is no concurrency within a single `proptest!{}` block. The
"1000 concurrent" behavior requires either:
- `tokio::test` + `tokio::spawn` for N concurrent futures, OR
- `proptest` with a custom executor that parallelizes strategy evaluation, OR
- A dedicated load-test harness (locust, wrk, criterion-specific concurrency).

The spec does not specify which. As written, the property test will execute 1000
sequential check_and_reserve calls, which will trivially show no race (there is no
concurrent access). The actual FM-059 race requires two goroutines/tasks to execute
check_and_reserve *simultaneously* against the same DO actor.

The chaos test §6.1.12 scenario 1 ("1000 concurrent writes at 99.9% quota → DO actor
serializes") is the correct approach for race validation — but chaos tests and property
tests are treated as separate artifacts. The property test claim of "concurrent" is
misleading.

**Fix**: Clarify `prop_quota_atomic_no_race` uses `tokio::test` with `N` concurrent
`tokio::spawn` tasks, each calling `check_and_reserve`. Add explicit harness specification.
Alternatively, rename the property test to reflect its actual sequential nature and
document that concurrency validation is in the chaos test exclusively.

---

### P1-4 (NEW) — SLO-DEDUP-RATIO lifecycle gap: forward stub not scheduled for registry promotion

**File**: WI-S07-005 §4 capability mapping, invariant_registry.md §3.18 cross-references
**Classification**: NEW (version-lifecycle gap)

**Finding**:

WI-S07-005 §4 references `slo_catalog.md SLO-DEDUP-RATIO` and the invariant_registry
§3.18 cross-references also mention it. But SLO-DEDUP-RATIO is not yet defined in
`slo_catalog.md` — it's described as "forward; defined in WI-S07-005 dashboard."

The sprint contract §6 DoD requires:
> "Dedup ratio measurable: ≥ 3 tenants em staging com workloads Docker pulls;
> `corelink_dedup_ratio{type=chunk}` ≥ 2.5× sustained 7d"

This is an operational SLO claim. But without a formal entry in `slo_catalog.md`, the
`validate_slo_references.py` CI gate (referenced as mandatory in prior sprint audits
for Lote 10.4-10.6) will fail on SLO-DEDUP-RATIO if it checks for forward-referenced
SLOs. There is no waiver, no explicit "forward stub approved" declaration, and no sprint
(S-08? S-09?) assigned to formalize it.

**Fix**: Either (a) add SLO-DEDUP-RATIO to `slo_catalog.md` as part of WI-S07-005
artifacts, or (b) create an explicit forward-stub declaration with an expiry sprint (S-09
latest) and add it to the ship gate `s07-ship-gate.yml` as a blocker if still missing
at SEAL.

---

### P2-1 (NEW) — LRU UNION race window is 30s, not "race-free"

**File**: WI-S07-004 §2 narrative, §6.1.7 `last_accessed_at_authoritative`
**Classification**: NEW (formal composition gap)

**Finding**:

WI-S07-004 §2 claims the `last_accessed_at_authoritative` UNION resolves the eviction
race. The analysis is incomplete. Consider:

- T0: GET fires → `record_access(tenant, digest)` called → DO buffer add scheduled
- T0+50ms: eviction worker fires LRU scan — reads D1 base (stale; 8d ago) AND calls
  `last_accessed_at_authoritative` — which reads DO buffer
- **But**: the `record_access` DO buffer-add is fire-and-forget. In the CF Workers
  model, the DO stub call (non-awaited) is a separate HTTP request to the DO actor.
  This request is in-flight from the handler but has NOT yet been processed by the
  DO actor at T0+50ms.
- The eviction worker at T0+50ms calls `last_accessed_at_authoritative` → reads DO
  buffer → buffer does NOT yet contain T0 (the add is still in-flight)
- Eviction reads only D1 base (8d ago) → decides to evict → soft-delete

The race window equals the DO actor processing latency for the fire-and-forget add, which
is bounded by:
- Network RTT from worker to DO: ~5ms typical, up to ~100ms under load
- DO actor queue depth: under thundering herd (1000 concurrent GETs all firing
  record_access simultaneously), the DO actor becomes a serialization bottleneck,
  queue depth grows, latency increases

WI-S07-004 §2 says "Race-free if DO authoritative em hot path" — but the DO buffer add
is itself asynchronous (fire-and-forget by design). The authoritative lookup and the
buffer add are two separate DO requests. There is no happens-before guarantee between
the non-awaited add and the subsequent authoritative read by the eviction worker.

**Severity assessment**: The actual race probability is low — eviction runs daily at
02:00 UTC, not continuously. But the spec claims "race-free" which is stronger than
the implementation guarantees. The property test `prop_lru_eviction_race` as described
(running in a single-threaded proptest context) will not catch this.

**Fix**: Weaken the claim from "race-free" to "race window bounded by DO queue latency
(< 100ms p99; measured; tolerable for LRU policy)". Document this explicitly in §2
and in INV-LRU-CONSISTENCY description. Remove "race-free" language from INV registry.

---

### P2-2 (NEW) — Cascade prevention SQL race vs in-flight UpdateActionResult

**File**: WI-S07-002 §6.1.6 reachable check SQL, §8 Gherkin "GC race"
**Classification**: NEW (formal composition; cross-WI)

**Finding**:

The cascade prevention reachable check SQL:
```sql
SELECT
    (SELECT COUNT(*) FROM manifest_chunks WHERE tenant_id = ? AND chunk_digest = ?) +
    (SELECT COUNT(*) FROM ac_meta a, json_each(a.blob_refs) j
     WHERE a.tenant_id = ? AND j.value = ? AND a.deleted_at_ms IS NULL)
    AS active_refcount;
```

This check queries D1 at time T0. Consider:
- T0: cascade check → `ac_meta` has 0 rows referencing blob B (refcount=0)
- T0+100ms: `UpdateActionResult` fires (client uploads result with blob B in output list)
  → D1 INSERT into `ac_meta.blob_refs` → refcount becomes 1
- T0+200ms: eviction soft-deletes blob B (cascade check returned 0 at T0)

This is the same race that INV-GC-004 (mark-phase-aware re-ref protection) addresses for
the GC mark phase. Eviction inherits INV-GC-001 but the WIs do NOT explicitly inherit
INV-GC-004's protection for the eviction code path.

WI-S07-002 §8 Gherkin "GC race" scenario shows:
> `eviction observes ac_meta.created_at >= T_mark_start (re-ref protection)`

But the eviction's reachable check SQL does NOT include a `mark_started_at` equivalent.
For GC, this is handled by `gc_correctness.tla` obligation `InvGCReRefProtected`. For
eviction, there is no equivalent formal mechanism — the eviction worker does not have a
corresponding `evict_started_at` watermark checked against `ac_meta.created_at`.

**Severity**: Within the 72h grace period, S-06 reconcile may catch the inconsistency.
But there is a window where a blob is soft-deleted (will be hard-deleted in 72h) while
a new AC entry referencing it exists. Within that window, a client retrieving the AC
entry would find the blob in `blob_meta` (deleted_at_ms set but not yet physically
deleted) and would need to re-upload. This is not data loss but is a false cache miss.
If S-06 reconcile runs and undeletes the blob (reconcile catches drift), the AC entry
is valid again. But the reconcile is daily — up to 24h of false cache misses.

**Fix**: Add `evict_started_at_ms` watermark to eviction worker. In the reachable check,
add `AND a.created_at < evict_started_at_ms` to exclude AC entries created after
eviction started (conservative; may over-retain but safe). Alternatively, formally
document this race window and add it to FM catalog.

---

### P2-3 (NEW) — DO migration/hibernation during quota check not analyzed

**File**: WI-S07-003 §2 ("DO actor model serializes"), sprint contract §8 INV-QUOTA-ENFORCEMENT
**Classification**: NEW (runtime deployment context)

**Finding**:

WI-S07-003 claims "race-free via DO actor model" with full confidence. CF Durable Objects
do provide single-threaded actor semantics for in-flight requests, but there are edge cases
not addressed:

1. **DO hibernation (DO Hibernation API)**: CF Workers DOs using the Hibernation API can
   be hibernated between requests. During hibernation, in-memory state (`pending_reservations`
   HashMap) is NOT persisted unless explicitly written to DO storage. The spec states
   "DO durable storage persists pending_reservations" (§6.1.3) — but only if the impl
   explicitly writes to DO storage on every mutation. This is an implementation requirement
   not a runtime guarantee. If the impl uses in-memory `HashMap` without per-mutation
   durable writes (e.g., only flushing every alarm tick), a hibernation event loses pending
   reservation state.

2. **DO migration**: CF may migrate a DO to a different data center during low-traffic
   periods. Migration involves a state transfer that, for large `pending_reservations` maps
   (under 50k reservation entries, unlikely in practice at ≤100 per tenant normally), should
   be transparent — but the spec does not verify this. The 50 MB per-DO storage limit (CF
   Q4 2025 docs) is not analyzed.

3. **DO cold start latency**: The spec's latency SLO of "≤3ms p99 quota check" assumes
   the DO is warm. Cold-start latency for a DO (including state recovery from storage) is
   typically 20-100ms. Under a traffic surge where thousands of tenants make their first
   write simultaneously, cold-start latency will blow the 3ms p99 SLO. The spec notes
   "DO sticky per region; warm cache" as mitigation but doesn't bound worst-case.

**Fix**: Add explicit statement that pending_reservations are written to DO storage on
every mutation (not only on alarm). Add cold-start SLO exemption clause ("p99 under warm
conditions; cold-start excluded from SLO measurement for ≤ first 30s after last request").

---

### P3-1 — SOTA bench claims "sustained 7d": no methodology for rolling avg vs instant minimum

**File**: Sprint contract §6 DoD, WI-S07-005 §2 narrative
**Classification**: P3 (methodology precision)

"≥ 2.5× sustained 7d" is used as a DoD gate. "Sustained" is undefined: does this mean:
- Rolling 7d average ≥ 2.5×? (tolerates dips during workload changes)
- All-instant minimum ≥ 2.5× over 7d? (zero tolerance for dip below 2.5×)
- Percentile-based (e.g., p50 ≥ 2.5× over 7d)?

Without a precise definition, the ship gate measurement is ambiguous. Two reviewers
can reach different pass/fail verdicts.

**Fix**: Pin to "7d trailing average of `avg(corelink_dedup_ratio{type=chunk})` per
3-tenant aggregate ≥ 2.5×; measured once at end of 7d staging run."

---

### P3-2 — Sprint contract §16 lists 3 baselines; bench scenarios omit bazel-remote

**File**: Sprint contract §16, WI-S07-005 §1 SOTA bench
**Classification**: P3 (minor contract drift)

Sprint contract §16 lists 3 SOTA baselines: NativeLink, BuildBuddy, **Bazel disk cache
(1× baseline)**. WI-S07-005 §1 and §6.1.4 cite "NativeLink + BuildBuddy" — 2 baselines.
"Bazel disk cache local (1×)" from the contract is omitted in the WI bench methodology.
Without the 1× baseline, the comparison loses its zero-reference anchor, making the
≥2.5× claim harder to contextualize for customers.

**Fix**: Add bazel-remote/bazel disk cache as bench scenario 3.

---

### P3-3 — `prop_lru_dedup` assertion targets flush output, not buffer state

**File**: WI-S07-004 §6.1.10 property tests `prop_lru_dedup`
**Classification**: P3 (test methodology)

`prop_lru_dedup`: "1000 records same key within 60s; assert ≤ 1 D1 UPDATE per flush
window." The assertion must be on the number of D1 UPDATEs **flushed**, not the number
of buffer entries (which is correctly 1 due to latest-write-wins). The buffer can be
updated N times in-memory by `record_access` (each call checks the 60s threshold), but
if the implementation short-circuits correctly, only 1 buffer entry exists. The flush
should produce exactly 1 D1 UPDATE.

However, the test must mock D1 to count UPDATEs, not just check buffer state. If the
test only asserts buffer.len() == 1, it does not verify the D1 flush behavior. The
property test description is ambiguous on which layer is being asserted.

**Fix**: Clarify test asserts on intercepted D1 UPDATE count from mock backend, not
buffer internal state.

---

## 4. Cross-WI Consistency + Race Window Analysis

### 4.1 LRU UNION Drift Window

The WI-S07-004 spec correctly identifies the eviction race problem and proposes
`last_accessed_at_authoritative` as the solution. The remaining gap (P2-1 above) is
that the fire-and-forget DO buffer add is not synchronous — there is a latency window
between the `record_access` call leaving the GET handler and the DO actor processing it.

Quantified: at 30s flush window + typical DO queue processing latency < 1s (non-storm),
the effective drift window is ~30s for the flush, plus up to ~100ms for the DO actor
to process the in-flight add. The eviction cron runs daily. The probability that an
eviction worker fires within this ~30s window after a GET that reset the LRU clock is:

```
P(race) ≈ 30s / 86400s ≈ 0.035% per blob per day
```

At 10M blobs under active eviction pressure, expected daily false-evictions: ~3,500.
For a free tier with 7d TTL, a blob accessed 7d 0h 30m ago (30min inside eviction window)
could be falsely evicted if its last GET fired 30s before eviction. This is bounded and
tolerable for LRU policy, but the spec should document this number rather than claiming
"race-free."

### 4.2 Quota TTL vs Multipart Upload Timing (P0-2 Extended Analysis)

The reservation TTL is tenant-global for all write types. For CAS PUT (WI-S01-001), a
single blob write takes seconds — 60s TTL is sufficient. For multipart SplitBlob
(WI-S05-001), a 160 GiB blob upload takes minutes to hours.

The spec does not distinguish between write handler types when applying the 60s TTL.
The three handlers mounted in §6.1.2 have radically different duration profiles:
- CAS PUT: < 30s for blobs up to ~1 GB
- UpdateActionResult: < 10s (metadata only)
- SplitBlob: up to 218 minutes for 160 GiB at 100 Mbps

A single reservation TTL policy cannot serve all three. This is a fundamental
design gap that propagates to INV-QUOTA-ENFORCEMENT correctness.

### 4.3 Cascade Prevention vs UpdateAR Race (P2-2 Extended)

The cascade check race (T0 check → T0+100ms UpdateAR → T0+200ms soft-delete) is mitigated
within 72h by S-06 reconcile. The INV-GC-001 violation is temporary (the blob is
soft-deleted but still has grace), not permanent (physical delete happens 72h later,
and reconcile runs daily). So the blast radius is: up to 24h of false cache misses for
AC entries referencing the soft-deleted blob, followed by auto-fix by S-06 reconcile.

This should be formally documented as an accepted bounded inconsistency (similar to
INV-AC-OUTPUTS-VALID-EVENTUAL-CONSISTENCY from §3.15). The current spec treats the
cascade check as providing stronger guarantees than it does.

### 4.4 WI-S07-004 → WI-S07-002 Integration Contract Gap

WI-S07-002 (eviction worker) MUST use `last_accessed_at_authoritative` from WI-S07-004.
This is stated in WI-S07-004 §6.1.7: "Eviction worker (WI-S07-002) MUST use this method."
However, WI-S07-002's LRU scan SQL (§6.1.5) does NOT reference `last_accessed_at_authoritative`.
The scan reads directly from D1:

```sql
SELECT digest, size_bytes, last_accessed_at, deleted_at_ms FROM blob_meta
WHERE tenant_id = ? AND deleted_at_ms IS NULL AND last_accessed_at < (? - ?)
ORDER BY last_accessed_at ASC LIMIT 250;
```

This initial scan uses D1-only `last_accessed_at`. Then the eviction worker SHOULD call
`last_accessed_at_authoritative` for each candidate before committing the soft-delete.
But this second authoritative check is mentioned in the eviction flow description only
implicitly — it's referenced in the cascade prevention section and in INV-LRU-CONSISTENCY,
but not in the eviction's scan SQL or the `execute_daily` flow steps.

A developer implementing WI-S07-002 without reading WI-S07-004 §6.1.7 would not know
to add this second authoritative check. The integration contract needs to be explicit
in WI-S07-002 (not just in WI-S07-004).

**Fix**: Add a numbered step in WI-S07-002 §6.1.5: "Step 3: For each candidate from LRU
scan, call `LruTracker::last_accessed_at_authoritative(tenant, digest)`. If returned
value >= now - tier_lru_window, skip candidate (DO buffer has more recent access)."

---

## 5. Verdict per WI

### WI-S07-001 — HOLD (P0 defect)

UNIQUE INDEX on `(tenant_id, chunk_digest)` inverts dedup semantics. The index must be
NON-UNIQUE. All other aspects (CTRL-ISO-005 gate, batch ≤250, REAPI conformance,
TenantCtx enforcement) are well-specified. **Cannot SEAL until P0-1 fixed.**

### WI-S07-002 — CONDITIONAL (P2 defect)

Core architecture (soft-delete-first, cascade prevention, jitter, audit fail-closed)
is sound. P2-2 (cascade check race vs UpdateAR) is real but bounded by S-06 reconcile
grace. The authoritative LRU consultation integration with WI-S07-004 is implicit —
must be made explicit. The 30d sustained chaos gate is appropriate. **Can proceed with
P2 fixes in Lote 10.7-tris.**

### WI-S07-003 — HOLD (P0 defect)

The reservation TTL architecture is fundamentally correct for short writes. For multipart
uploads (SplitBlob), the 60s TTL is insufficient. This is a P0 correctness defect for
the INV-QUOTA-ENFORCEMENT claim under large blob uploads. **Cannot SEAL until P0-2
addressed via streaming reservation extension or per-size TTL floor.**

Also: the DO hibernation risk (P2-3) should be documented. The cold-start latency
exception should be added to the SLO clause.

### WI-S07-004 — HOLD (P0 defect)

`wasm_bindgen_futures::spawn_local` is not available in CF Workers Rust runtime. The
hot-path fire-and-forget mechanism is unimplementable as specified. The LRU UNION race
window is real but tolerable if accurately characterized (not "race-free"). **Cannot
SEAL until P0-3 fixed with correct CF Workers async primitive.**

### WI-S07-005 — CONDITIONAL (P1 defect)

INV count mismatch (3 vs 5) will cause ship gate ambiguity. SLO-DEDUP-RATIO lifecycle
gap needs resolution. bazel-remote missing from bench scenarios. Cardinality bomb (P1-2)
propagates here through the dashboard metrics. **Can proceed after P1 fixes; does not
require new architecture.**

---

## 6. Sonnet vs Opus Differential — What Sonnet Sees That Opus Likely Didn't

Opus (Agent R4) applies depth-first API semantics analysis: trait signatures, error
types, Gherkin scenario completeness, REAPI conformance. This is valuable for catching
handler logic gaps, missing error variants, and protocol conformance issues.

Sonnet R5's differential value in this review:

1. **Schema semantics inversion (P0-1)**: Opus would likely check that the UNIQUE INDEX
   exists and is referenced correctly, not *whether UNIQUE is the right constraint*.
   Semantic inversion — where a constraint that sounds correct (UNIQUE for consistency)
   is actually wrong for the use case (dedup requires shared chunk digests) — requires
   understanding the data flow, not just the schema declaration.

2. **TTL arithmetic against upload duration (P0-2)**: Opus checks reservation lifecycle
   completeness. Sonnet checks the arithmetic: 160 GiB ÷ 100 Mbps = 218 minutes vs
   60s TTL = hard contradiction. This is a deployment-context calculation, not an API
   analysis.

3. **Runtime API availability (P0-3)**: `wasm_bindgen_futures::spawn_local` is a Cargo
   feature that exists in crates.io but is meaningless in CF Workers Rust runtime. Opus
   would validate that the API is used correctly; Sonnet asks whether it *exists at all*
   in the target runtime.

4. **Cardinality arithmetic (P1-2)**: INV-OBS-CARDINALITY-BUDGET says ≤100k total series.
   S-07 metrics at 100k tenants × 5 tiers = 500k. This is multiplication, not protocol
   analysis.

5. **PropTest concurrency illusion (P1-3)**: Opus would check that the property test
   covers the right invariant. Sonnet checks whether the test harness *can physically
   execute concurrently* — sequential proptest cannot catch concurrent race conditions
   regardless of how many iterations are run.

6. **DO hibernation state loss (P2-3)**: Opus checks DO actor single-thread claim.
   Sonnet checks CF's actual hibernation API behavior — in-memory state vs durable storage
   semantics under hibernation and migration.

---

## 7. Lote 10.7-tris Fix Plan

Based on this review, the following P0 fixes are required before S-07 can SEAL:

### Fix 1 (P0-1): Correct the UNIQUE INDEX to a non-unique INDEX

```sql
-- Remove:
CREATE UNIQUE INDEX idx_manifest_chunks_tenant_digest
    ON manifest_chunks (tenant_id, chunk_digest);

-- Replace with:
CREATE INDEX idx_manifest_chunks_tenant_digest
    ON manifest_chunks (tenant_id, chunk_digest);
```

Update WI-S07-001 §1, §6.1.2, sprint contract §5 R-S07-1, INV-DEDUP-CONSISTENCY
description in registry. Add property test `prop_two_blobs_share_chunk` (two
`manifest_chunks` rows, same tenant+chunk_digest, different blob_digest → both succeed).
Chaos test scenario 6 must be updated: the UNIQUE constraint violation test is now
a test for INV-CAS-INTEGRITY (same digest = same content, enforced by BLAKE3), not
a database constraint.

### Fix 2 (P0-2): Tiered reservation TTL based on request_bytes

Add `estimate_ttl(request_bytes: u64) -> Duration` to `QuotaEnforcer`:
```rust
fn estimate_ttl(request_bytes: u64) -> Duration {
    // Assume minimum upload rate 1 Mbps; add 2× safety margin
    let min_rate_bps = 1_000_000u64 / 8; // 125 KB/s
    let estimated_seconds = (request_bytes / min_rate_bps).max(60);
    Duration::from_secs(estimated_seconds * 2)
}
```

Cap at 24h (86400s) for enterprise. Update WI-S07-003 §6.1.4, §1, §8 Gherkin
scenario. Add integration test: reserve 160 GiB → verify reservation TTL > 3600s;
simulate 5-minute upload → commit succeeds (reservation still valid).

### Fix 3 (P0-3): Replace spawn_local with correct CF Workers async primitive

In WI-S07-004 §6.1.4, replace:
> "Spawned via `wasm_bindgen_futures::spawn_local` ou `tokio::spawn`"

With:
> "Sends non-blocking DO stub fetch to `lru-tracker-<region>` DO via
> `worker::send_future(do_stub.fetch(add_request))` (CF Workers after-response
> future) OR issues fire-and-forget DO stub RPC; handler does not await response.
> `tokio::spawn` and `wasm_bindgen_futures::spawn_local` are NOT available in
> CF Workers Rust runtime — use worker crate primitives exclusively."

Add to `Cargo.toml` corelink-lru: `[dev-dependencies]` assertion test that
`tokio` is not in `[dependencies]` for the worker target.

### P1 Fixes (required for ship gate but not block on SEAL prep):

- **Fix 4 (P1-1)**: Update WI-S07-005 to list 5 NEW INVs throughout.
- **Fix 5 (P1-2)**: Replace `{tenant_id}` labels with `{tenant_tier}` for
  Prometheus counters; route per-tenant cardinality to CF Analytics Engine.
- **Fix 6 (P1-3)**: Clarify property test harness: `prop_quota_atomic_no_race`
  uses `tokio::test` concurrent spawn, documented explicitly.
- **Fix 7 (P1-4)**: Add SLO-DEDUP-RATIO to `slo_catalog.md` or create formal
  forward-stub entry with expiry sprint ≤ S-09.

### P2 Fixes (recommended before 30d chaos gate):

- **Fix 8 (P2-1)**: Document LRU race window as "< 30s bounded; tolerable for LRU
  policy"; remove "race-free" language from INV-LRU-CONSISTENCY description.
- **Fix 9 (P2-2)**: Add `evict_started_at_ms` watermark to cascade prevention SQL
  OR formally document the UpdateAR race window as accepted bounded inconsistency
  (add to FM catalog as FM-306 "eviction-UpdateAR race window").
- **Fix 10 (P2-3)**: Add DO hibernation durability requirement to WI-S07-003
  §6.1.3: "pending_reservations MUST be written to DO durable storage on every
  mutation (not only on alarm flush)."
- **Fix 11 (Cross-WI)**: Add explicit authoritative LRU check step in WI-S07-002
  §6.1.5 eviction flow (cross-reference WI-S07-004 §6.1.7).

---

## Appendix: ADR Status Cross-Check

| ADR | Status in doc | doc_status | Claim in WI-S07-005 | Assessment |
|---|---|---|---|---|
| ADR-0019 | DRAFT | "DRAFT" (not FROZEN) | "ratificação FROZEN" | **Mismatch**: ADR-0019 is doc_status DRAFT; WI-005 assumes FROZEN. Must update ADR to FROZEN before ship gate. |
| ADR-0020 | DRAFT | "DRAFT" (not FROZEN) | "ratificação FROZEN" | **Same issue**: ADR-0020 is DRAFT; must be FROZEN. |

WI-S07-005 §6.1.6 refers to "ADR-0019 file at FROZEN" in the Gherkin acceptance
scenario. Both ADRs have `doc_status: DRAFT` and `audit_status: ACTIVE`. They must be
promoted to `doc_status: FROZEN` as part of the sprint seal process — this is a
pre-condition for the ship gate Gherkin scenario to pass, and it's currently not in
any WI's sub-tasks.

---

*Review complete. 3 P0 findings, 4 P1 findings, 3 P2 findings, 3 P3 findings.
Overall S-07 SOTA score: 6.8/10. HOLD on WI-001, WI-003, WI-004 pending P0 fixes.
Conditional proceed on WI-002 and WI-005.*
