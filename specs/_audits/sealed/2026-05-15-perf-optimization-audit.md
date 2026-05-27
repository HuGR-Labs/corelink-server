---
id: "AUDIT-PERF-OPTIMIZATION-2026-05-15"
type: "audit"
doc_status: "REVIEW"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R-prep / R-6 staging entry follow-on"
parent_wi: "R-PREP-PERF-OPTIMIZATION-AUDIT"
owner: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "performance", "criterion", "slo", "p99", "latency", "hot-path", "optimization-plan"]
---

# Performance Optimization Audit — Top 5 hot spots, plan, and projected p99 wins

> **doc_status:** REVIEW · **scope:** identify where realistic latency
> wins live in the CoreLink hot path, given the criterion baseline from
> `2026-05-14-perf-baseline.md` and the mutation-tested invariants in
> `2026-05-14-mutation-baseline.md`. **No code changes** — this is the
> targeting document that informs Sprint-N optimization WIs.
>
> **Anchor:** the perf-nightly suite has 11 criterion bench files
> covering 8 hot paths; the laptop baseline shows ≥ 3x headroom on
> every SLO-bound bench. That headroom is a *floor*: production
> introduces R2 / D1 / KMS RTT, Worker isolate cold start, and
> per-request allocation pressure that the criterion benches do **not**
> measure. The wins below target the parts of the hot path that
> *will* dominate p99 in production once the network primitives stop
> being the limiting term.

---

## 1. Methodology

We did **not** run a fresh perf sweep — the
`2026-05-14-perf-baseline.md` numbers are authoritative for this
analysis. Instead we layered three lenses on top of that baseline:

1. **Bench-to-SLO traceback.** For each of the 27 baseline rows we
   asked: *which SLO does this bench bound, what is the SLO p99
   budget, and how much of that budget is the bench measuring?* Every
   bench that measures < 15% of its SLO budget got de-prioritized
   (the win lives elsewhere in the stack — likely network/RTT, not
   CPU). Benches in the 15-60% band are the candidates.
2. **Source-level hot-path read.** For each candidate we read the
   implementing module looking for: per-request allocations, lock
   choices (`std::sync::Mutex` vs `parking_lot`), JSON / hex / base64
   work in the inner loop, repeated BLAKE3 hasher initialization,
   `clone()` on `Vec<u8>` / `String`, and serialization round-trips
   that could be elided. Findings are tagged with `file:line`.
3. **Profiling assumptions.** We did **not** capture production flame
   graphs (the perf-nightly workflow has only one run in artifact
   storage). Where this audit calls a path "hot" we mean *expected to
   be hot under the production call mix described in the SLO
   catalog*. Production flame-graph capture is itself a followup
   (OPT-06 in `perf-optimization-followup-tickets.md`).

### 1.1 What we would expect a real flame graph to show

If we had `pprof-rs` + `cargo flamegraph` output from a staging worker
under the k6 `endurance-24h.js` load profile, the expected top frames
on the CAS GET path (the dominant SLO) would be, in descending order:

```
~38%  R2 GET I/O (network, not CPU; floor — not optimizable from Rust)
~14%  serde_json deserialization of D1 row → AcEnvelope / multipart manifest
~11%  worker-rs router + tower middleware tower chain (mostly per-req heap allocs)
~ 9%  BLAKE3 hash of response body (client-verify path)
~ 7%  HMAC-SHA256 tenant-prefix derive (per request — re-derived, not cached)
~ 6%  std::sync::Mutex contention on in-process caches (auth_ctx, KV memo)
~ 5%  serde_jcs canonicalize on audit emit (only on CAS PUT / mutating ops)
~ 4%  AES-256-GCM decrypt (BYOK only — gated by tier)
~ 6%  miscellaneous (logging, tracing span entry, error mapping)
```

The five frames in the 5%-15% band are the optimization candidates;
everything outside is either floor (network I/O) or noise (logging).
The ranking below targets exactly that band.

---

## 2. Top 5 hot spots (ranked by `current p99 × call frequency × SLO budget cost`)

### Ranking formula

```
score = bench_p99_us × est_calls_per_request × (1 / slo_budget_us)
       × tier_multiplier(enterprise=2.0, team=1.0)
```

Higher score = bigger reward per unit of optimization effort. We
present the five highest-scoring hot spots; everything below the cut
goes into the followup backlog as P3.

### OPT-01 — `derive_prefix` HMAC-SHA256 not cached per (tdk_version, tenant_id) — **LANDED 2026-05-15**

| Field | Value |
|-------|-------|
| Code | `crates/tenant-path/src/prefix.rs:148-166` (`derive_prefix`) |
| Bench | `tenant_path/derive_prefix` — ~1.5 µs single-shot (~66x SLO headroom locally) |
| Frequency | **Every CAS GET / PUT / AC request** — called once per request to namespace storage keys (`R2 path = "{prefix}/cas/{hash}"`, `D1 binds = (prefix, hash)`) |
| SLO bound | SLO-LAT-CAS-GET enterprise 200ms p99 (`slo_catalog.md §4.6`); per-req cost ~1.5 µs = 0.75% of budget today, but call-shape is `n_workers × rps × 1.5µs` → on a 10k rps tenant that's ~15ms/s of CPU spent **per worker isolate** re-deriving the same value |
| Root cause hypothesis | `derive_prefix` is a deterministic PRF of `(tdk_bytes, tenant_uuid)`. The TDK rotates on a 7d cadence (`key_management.md §3.2.1`); the tenant_uuid is per-request constant. Across a single isolate's lifetime (~5-30 min CF Worker) we re-compute HMAC-SHA256 ~10⁶ times for the same N tenants. Pure waste. |
| Optimization approach | **Caching, with rotation awareness.** Add a `parking_lot::RwLock<HashMap<(TdkVersion, Uuid), TenantPrefix>>` keyed by `(tdk_version, tenant_id)`. Cache hit returns 16-byte `Copy` prefix in ~50ns. Cache miss falls through to the existing `derive_prefix` path. **Critical invariant:** cache key MUST include `tdk_version` so a rotation transparently invalidates (`INV-KEY-NO-SKIP` from `key_management.md`). Size cap: 4096 entries with random eviction (a single worker isolate serves ≤ a few thousand distinct tenants per cold-start cycle). |
| Projected p99 reduction | **~1.4 µs saved per request × call frequency** = 0.7% of CAS GET p99 budget reclaimed. Small in % terms; meaningful in *cumulative CPU* terms because every endpoint touches it. Lifts effective worker throughput by an estimated 3-5% under heavy multi-tenant load. |
| Effort | **S** (single struct + 2 properties + 1 mutation-survival regression — see `perf-optimization-followup-tickets.md` OPT-01) |
| Risk | **Low.** Cache is per-isolate, no cross-tenant leak possible (key is keyed by tenant_id itself). Rotation-correctness is the one trap → mandatory property test `cache_invalidates_on_tdk_version_bump` (kill-rate ≥ 75% required, per mutation baseline). |
| SLO it tightens | SLO-LAT-CAS-GET (enterprise) p99 200ms → projected ~198ms (post-rollout). Minor on its own; multiplicative with OPT-03. |

### OPT-02 — `serde_jcs::to_vec` allocates a fresh `Vec<u8>` per audit event — **LANDED 2026-05-15**

| Field | Value |
|-------|-------|
| Code | `crates/corelink-audit-chain/src/chain.rs:72-74` (`compute_canonical_bytes`) called from `link_chain_hash` at line 91-92 |
| Bench | `audit_chain/append_single` ~20 µs (10x headroom on 200µs SLO); `audit_chain/append_10k/sequential` ~250 ms |
| Frequency | **Every audit-emitting operation** — CAS PUT (1x), CAS GET on cache miss (1x), AC update (1x), admin ops (1x). Estimated ~30% of total worker req-handling time goes through an audit emit. |
| SLO bound | SLO-LATENCY-AUDIT-EMIT 200 µs p99 (`2026-05-14-perf-baseline.md §3`); 10% of CAS PUT p99 budget. |
| Root cause hypothesis | `serde_jcs::to_vec(event)` returns a freshly-allocated `Vec<u8>` for every call. The `Vec` is then passed to BLAKE3 `Hasher::update(&buf)` and dropped at end-of-scope. We do this 10k+ times on the `append_10k/sequential` bench → 10k heap allocs + 10k frees, plus L1-cache pollution from the hasher state being re-initialized each time. |
| Optimization approach | **Allocation elision via streaming serializer.** Two parts: (a) thread-local reusable `Vec<u8>` buffer (or a `bumpalo::Bump` arena scoped to a request) — `Vec::clear()` before each `to_vec_into(&mut buf, event)`; (b) since the chain-link hash is `BLAKE3(prev_hash ‖ canonical_bytes)`, we can feed canonical bytes directly into a `blake3::Hasher` via `serde_jcs::to_writer(&mut hasher_adapter, event)` (the `Hasher` already implements `std::io::Write`). This **eliminates** the intermediate buffer entirely on the producer path. The verifier still needs the canonical bytes since they're persisted to R2 NDJSON — verifier keeps the `Vec` path. |
| Projected p99 reduction | Eliminate **2 allocs + 1 free per audit event** (the `Vec` itself + the `String` from the error mapper on the happy path is already elided via `?`). On the `append_10k/sequential` bench we project 250 ms → 180-200 ms (-20-28%). On a single audit emit p99 200 µs → 130-150 µs (-25-35%). |
| Effort | **M** — touches the public surface of `chain.rs` (we must add `link_chain_hash_streaming` without breaking `link_chain_hash`); requires careful interaction with `serde_jcs`'s `to_writer` semantics. |
| Risk | **Medium.** The canonical-bytes determinism property (asserted by `prop_jcs_canonicalization_deterministic`, 10k iterations) MUST continue to hold across both code paths — the streaming version must produce byte-identical output to `to_vec`. Mitigation: cross-equivalence property test `streaming_and_to_vec_produce_identical_hash` (10k cases, must be added to the WI's mutation baseline). |
| SLO it tightens | SLO-LATENCY-AUDIT-EMIT 200 µs → projected 150 µs p99. Cascades into SLO-LAT-CAS-PUT (audit emit is in the synchronous PUT path). |

### OPT-03 — `serde_json::from_slice` on every D1 row read (AC envelope / multipart manifest)

| Field | Value |
|-------|-------|
| Code | `crates/corelink-worker/src/reapi/ac/handler.rs:304` and the AC meta reader path; also `crates/corelink-worker/src/storage/r2.rs` for multipart manifest deserialization |
| Bench | None today — this is a gap (see OPT-06 followup: "add `serde_json::from_slice` micro-bench"). Anchored to baseline §3 SLO-LAT-AC-HIT 150 ms p99. |
| Frequency | **Every AC GET** = 1× from_slice; **every multipart CAS GET** = 1× from_slice. AC HIT is the most-frequent request shape in a healthy build farm (≥ 80% of REAPI traffic). |
| SLO bound | SLO-LAT-AC-HIT 150 ms p99 (`slo_catalog.md §4.8`). Production p99 dominated by D1 RTT (~10-30 ms) + R2 GET (~20-50 ms); from_slice cost estimated at 30-80 µs per AC envelope (assuming ~512 B envelope). 0.05% of SLO budget today, but it sits **inside the synchronous critical path** and runs *before* we can return — every µs here is on the wire latency. |
| Root cause hypothesis | We deserialize `AcEnvelope` (and similar D1 rows) via `serde_json::from_slice` on the hot read path. `serde_json` allocates `String` per field (for owned-string types). The envelope schema has ~8 string fields → ~8 heap allocs per AC GET. |
| Optimization approach | Two-part: (a) Switch envelope owned `String` → `Cow<'a, str>` where the deserializer borrows from the input buffer (`#[serde(borrow)]`). For envelope data persisted as JSON in D1 BLOB columns this works — the buffer lives across the synchronous handler. (b) Where (a) is impractical (cross-`await` lifetimes), switch storage encoding to a **postcard / bincode** binary format for in-house schemas — drop JSON entirely for D1 envelopes the customer never sees. Keep JSON only on the public REAPI wire. |
| Projected p99 reduction | (a) alone: ~20-40 µs saved per AC GET (allocation pressure). (b) end-to-end: 30-80 µs saved + GC pressure relief; AC HIT p99 ~150 ms → projected ~149.95 ms (negligible in absolute terms; meaningful at scale: ~3% worker CPU recovered under sustained AC load). |
| Effort | (a) **S** — opt-in `#[serde(borrow)]` + lifetime threading on the envelope type. (b) **L** — schema migration on D1 BLOB columns; requires read-both / write-one transition; not a Sprint-N candidate. |
| Risk | (a) **Low.** Borrowed deserialization is well-trodden in serde; the only trap is accidentally extending the borrow across an `.await`. (b) **High** — schema migration on a live D1 row. Recommend (a) only for the first wave; (b) only behind a feature flag + post-GA. |
| SLO it tightens | SLO-LAT-AC-HIT 150 ms p99 (marginal); SLO-AVAIL-AC (indirect — fewer GC pauses = fewer 5xx). |

### OPT-04 — `std::sync::Mutex` on in-process caches (KV memo, R2 backend mock, AC handler memo, sessions, assembler, chunk store)

| Field | Value |
|-------|-------|
| Code | `crates/corelink-worker/src/cache/kv.rs:41,199`; `crates/corelink-worker/src/storage/r2.rs:43,369`; `crates/corelink-worker/src/reapi/cas/session.rs:48,403`; `crates/corelink-worker/src/reapi/cas/assembler.rs:319`; `crates/corelink-worker/src/reapi/cas/chunk_store.rs:39,209`; `crates/corelink-worker/src/reapi/ac/handler.rs:304,325`; `crates/corelink-worker/src/reapi/ac/outputs.rs:36`; `crates/corelink-worker/src/reapi/ac/meta.rs:44`; `crates/corelink-audit-chain/src/audit.rs:30` |
| Bench | No direct bench; contention is *latent* at single-threaded bench time. Manifests under k6 endurance-24h.js multi-VU load. |
| Frequency | 10 distinct sites — read-heavy in production (≥ 95% of locks are read accesses on these structures). |
| SLO bound | SLO-LAT-CAS-GET (read-heavy contention is in the GET path); SLO-AVAIL-CAS-GET (lock poisoning under panic → 5xx). |
| Root cause hypothesis | `std::sync::Mutex` in Rust 1.81 uses a futex-based fast path on Linux but **still incurs poison-flag bookkeeping** + `Result<MutexGuard>` unwrap noise. `parking_lot::Mutex` is faster on uncontended fast path (~20% less overhead per lock/unlock) and **does not poison** (panics inside the critical section don't permanently disable the lock — this is desirable for in-process caches where the cache state is reconstructible). For read-heavy workloads `parking_lot::RwLock` gives proper reader-parallelism that `std::sync::Mutex` cannot. |
| Optimization approach | **Migration in two phases:** (1) Mechanical `std::sync::Mutex` → `parking_lot::Mutex` swap for the 10 callsites where the lock-as-mutex semantics are correct (sessions, assembler, chunk_store, audit emit). (2) Audit each site: where 95%+ of accesses are reads (KV memo, R2 backend mock, AC handler memo), convert to `parking_lot::RwLock`. Add `parking_lot = "0.12"` (already a transitive dep). |
| Projected p99 reduction | Per-lock micro-savings ~50-100 ns; multiply by ~6-12 lock acquisitions per request → 300 ns - 1.2 µs saved per request. The bigger win is under **contention**: at 10k rps the futex-tail effect of `std::sync::Mutex` adds p99 tail-latency in the 100-500 µs range that doesn't appear in single-threaded benches. **Conservative range: 0.5-2% of p99 SLO budget reclaimed; aggressive range: 5% under heavy load.** |
| Effort | **S** (mechanical for phase 1) → **M** (audit + RwLock conversion for phase 2). |
| Risk | **Low** — `parking_lot` API is a near-drop-in. Watch: (a) `parking_lot::Mutex` doesn't poison → callers must not rely on `Result<Guard>` for cross-thread error signaling (none of our sites do); (b) `wasm32-unknown-unknown` (CF Worker target) — parking_lot supports wasm32 since 0.12 but `_unknown` triple is the right one to confirm. |
| SLO it tightens | SLO-LAT-CAS-GET (tail-latency reduction); SLO-AVAIL-CAS-GET (no more poison-induced 5xx on panic-in-critical-section). |

### OPT-05 — Repeated `blake3::Hasher::new()` per request (no hasher-state reuse on streaming uploads) — **LANDED 2026-05-15 (substituted for OPT-03(a) in DEBT-013 PARTIAL batch)**

| Field | Value |
|-------|-------|
| Code | `crates/corelink-audit-chain/src/chain.rs:104` (`Hasher::new()` per link); `crates/corelink-hash/src/digest.rs:31` (one-shot `blake3::hash` per body — fine, *but* the streaming-upload path in `reapi/cas/assembler.rs` constructs a fresh hasher per chunk too) |
| Bench | `blake3/hash/1MiB` ~330 µs; `blake3/hash/100MiB` ~3.3 GiB/s (saturating). |
| Frequency | Every CAS PUT body (1×); every audit chain append (1×); every multipart-upload chunk (N× per PUT). |
| SLO bound | SLO-LAT-CAS-PUT 1s (team) / 600ms (enterprise) p99. BLAKE3 contributes the *cryptographic floor* — we cannot make it faster than what `blake3` crate already gives. |
| Root cause hypothesis | `blake3::Hasher::new()` is cheap (~70 ns per `blake3/hash/1B` bench), but on the multipart-assembler path we construct one hasher per chunk and then *separately* compute the final root from the merkle tree of chunk hashes. That re-hashing of intermediate states is intrinsic to the BLAKE3 root construction — **we cannot eliminate it**. But we **can** elide the *per-message* `Hasher` initialization in tight loops like `link_chain_hash_from_canonical` (chain.rs:100) where we already know the buffer layout: pre-build a thread-local `Hasher` template, `clone()` (cheap — internal `[u32; 16]` state copy) per use rather than `new()`. |
| Optimization approach | **Hasher cloning over re-init** at: (a) `chain.rs:104` — use `Hasher::new_keyed(&[0u8; 32]).clone()` from a `thread_local` (saves ~50 ns per audit append × 10⁶/sec = 50ms/s CPU); (b) any audit-chain hot loop that hashes ≥ 100 events back-to-back. **Do NOT apply to one-shot `Digest::compute`** — it already uses the most-optimal `blake3::hash` free fn. |
| Projected p99 reduction | Tiny in absolute terms (~50 ns saved per audit append) but **the only SLO-floor improvement available**: since BLAKE3 is the cryptographic floor of CAS integrity, this is one of the only places we can move the floor at all. Cumulative on `append_10k/sequential`: 250 ms → projected 240-245 ms (-2-4%). |
| Effort | **XS** (3-line change at chain.rs:104; thread-local hasher template). |
| Risk | **Very low** — `Hasher::clone()` is documented public API of `blake3`; semantics identical to `new()` on the fresh state. |
| SLO it tightens | SLO-LATENCY-AUDIT-EMIT (marginal, but cumulative with OPT-02). |

---

## 3. Cross-reference: which SLOs tighten if all 5 land

| SLO | Today's target | Projected post-OPT-01..05 | Delta |
|-----|----------------|---------------------------|-------|
| SLO-LAT-CAS-GET (enterprise) | 99% < 200 ms | 99% < 195-198 ms | -2-5 ms |
| SLO-LAT-CAS-GET (team) | 99% < 300 ms | 99% < 294-298 ms | -2-6 ms |
| SLO-LAT-CAS-PUT (enterprise) | 99% < 600 ms | 99% < 580-595 ms | -5-20 ms |
| SLO-LAT-AC-HIT | 99% < 150 ms | 99% < 148-150 ms | -0.05-2 ms |
| SLO-LATENCY-AUDIT-EMIT (R-prep / internal) | p99 < 200 µs | p99 < 130-150 µs | **-25-35%** |
| SLO-AVAIL-CAS-GET | 99.95% | 99.95% (unchanged; tail-fix only) | structural improvement (fewer poison-5xx) |

**Headline:** the largest *percentage* win is SLO-LATENCY-AUDIT-EMIT
(internal SLI, not customer-facing) at **-25-35%**. The largest
*customer-facing* absolute win is SLO-LAT-CAS-PUT at **-5-20 ms**.
The **range estimate for headline customer-facing p99 reduction is
3-10%** (mid: ~5%) across the CAS read+write path, weighted by typical
build-farm call mix.

> **Important:** these are projections, not measurements. They become
> defensible only when the optimization WI lands and the
> `perf-nightly.yml` workflow produces an A/B baseline.json
> comparison. See OPT-06 (followup) for the projection-validation
> protocol.

---

## 4. Anti-patterns observed (cross-cutting)

These don't rise to "top 5 hot spot" individually but appear at
enough sites to warrant a global cleanup wave (tracked as OPT-07 in
followup):

1. **`String::clone()` in tower middleware** —
   `crates/corelink-worker/src/middleware/auth.rs:265,497,502,597,632,637`
   has 6+ String clones per request through the auth chain. Most are
   identifiers (`org_id`, principal IDs) where `Arc<str>` would be
   strictly better than re-cloning the `String` heap allocation per
   request. **Wave:** convert hot-path `String` fields on shared
   contexts to `Arc<str>` (cheap clone, no extra alloc).
2. **`format!()` in error mappers** — `chain.rs:73`,
   `envelope.rs:85,189,207,214`. Error paths are usually cold, but
   `BYOKError::AesGcm(e.to_string())` runs on every successful path
   for the `?` propagation (clippy false-positive: format-string is
   eager). Use `thiserror::Error`'s `#[source]` instead of stringifying.
3. **Re-derivation across cold-start boundaries** — every Worker
   isolate cold start re-derives every tenant prefix it has ever
   seen (OPT-01 fixes this *within* an isolate but cold start still
   pays the full cost). Long-term: persist a *signed* prefix cache
   to KV with TTL; load on cold start. **Out of scope** for OPT-prep
   wave; tracked as OPT-08.
4. **`Vec::clone()` on response bodies** — `r2.rs:208` clones the
   body for retry idempotency. Should be `Bytes::clone` (cheap
   refcount bump) — verify the type is `Bytes` end-to-end on the PUT
   path; convert if not.
5. **`serde_json::Value` for AAD construction** — `envelope.rs:219`
   uses `serde_json::json!()` to build the AAD object. This allocates
   a `Map<String, Value>`, then serializes to bytes on the KMS call.
   For a fixed-shape AAD a hand-rolled `format!()`-into-buffer (or
   even `concat_bytes!`-like compile-time template) would save ~3 allocs
   per BYOK write. Marginal (~1-2 µs) but happens on every BYOK PUT.
6. **`.expect()` and `.unwrap()` on `Mutex` locks** — when we keep
   `std::sync::Mutex` *and* the lock is on a long-lived structure,
   `.unwrap()` on a poisoned lock will cascade panics. Per OPT-04 we
   should remove the poison axis entirely; until then audit all
   `.lock().unwrap()` sites.

---

## 5. What we explicitly did NOT find

To avoid the appearance of "we just renamed `clone()` calls and called
it optimization", flag what is **not** broken:

- **BLAKE3 implementation is already SOTA.** The `blake3` crate uses
  SIMD (SSE4.1 / AVX2 / AVX-512 / NEON) and saturates ~3 GiB/s on a
  contemporary arm64 laptop. **Do not** consider replacing with
  SHA-256 (slower) or hand-rolled hashing. The cryptographic floor
  is what it is.
- **HMAC-SHA256 in `derive_prefix` is correctly bounded.** Pre-OPT-01
  it's ~1.5 µs/call — already at the floor for HMAC-SHA256 on 16
  bytes of input. The win is caching the *output*, not the algorithm.
- **`Aes256Gcm` (BYOK) is hardware-accelerated.** The `aes-gcm` crate
  uses AES-NI / ARMv8 Crypto Extensions where available. ~3-5 µs
  roundtrip on the InMemory bench is at the hardware floor for a
  typical small DEK-wrap operation; production KMS RTT dominates.
- **Tokio scheduler** is not on the worker path (CF Worker uses
  worker-rs's own single-threaded executor; tokio appears only in
  the audit chain and DR drill harnesses). No win from
  `tokio::spawn_blocking` tricks.
- **Constant-time timing-padding middleware** —
  `middleware/timing_padding.rs` deliberately introduces a sleep on
  404; this is **security-critical** (timing oracle defense) and
  must not be optimized away. We confirmed the padding policy is
  the right *bounded* sleep target (sleep_until, not sleep_for); no
  optimization owed here.

---

## 6. Effort vs reward map

```
                  Effort →
                XS    S     M     L
  Reward ↑   ┌─────┬─────┬─────┬─────┐
   high      │     │OPT-1│OPT-2│OPT-3│
             │     │     │     │ (b) │
             ├─────┼─────┼─────┼─────┤
   medium    │OPT-5│OPT-4│OPT-3│     │
             │     │ ph1 │ ph2 │     │
             │     │     │     │     │
             ├─────┼─────┼─────┼─────┤
   low       │     │ AP-5│     │     │
             │     │ AP-2│     │     │
             └─────┴─────┴─────┴─────┘

Recommended Sprint-N batch: OPT-01 + OPT-05 + OPT-04 phase 1
(all S/XS, cumulative ~3-5% p99 reduction, low risk).
Sprint-(N+1): OPT-02 + OPT-03(a) + OPT-04 phase 2 (audit-emit win
+ AC borrow win + RwLock conversion; medium effort).
Defer to post-GA: OPT-03(b), OPT-08 (cold-start prefix persistence).
```

---

## 7. References

- `specs/_audits/2026-05-14-perf-baseline.md` — criterion baseline.
- `specs/_audits/2026-05-14-mutation-baseline.md` — kill-rate bar
  preserved by every optimization WI.
- `specs/_audits/2026-05-14-coverage-baseline.md` — coverage floor
  preserved by every optimization WI.
- `specs/03_architecture/slo_catalog.md` — SLO targets we project
  against.
- `specs/03_architecture/key_management.md §3.2.1` — TDK rotation
  cadence that constrains OPT-01 cache invalidation.
- `docs/internal/PERFORMANCE-PLAYBOOK.md` — engineering patterns
  derived from this audit (companion doc).
- `specs/_audits/perf-optimization-followup-tickets.md` — Sprint-ready
  WI candidates.

---

**Fim de AUDIT-PERF-OPTIMIZATION-2026-05-15.**
