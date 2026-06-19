# CAS hot-path latency — measured root cause + remediation plan

> 2026-06-19 · CoreLink Server TL · in response to githugr handoff
> `2026-06-18-BLOCKER-corelink-server-tl-cas-endpoint-latency-3-to-7s`.
> **The handoff (and a first code-trace) blamed Argon2id. Direct measurement against
> corelink-prod disproves that.** This doc records the real, measured root cause and the
> remediation plan with the correctness invariants each fix must preserve.

## TL;DR

`GET/PUT /v1/cas/{tenant}/{key}` is slow because the request is served by the **native
container** (not the wasm Worker), and the container's per-request hot path makes **two
synchronous D1-over-HTTP control-plane round-trips** (`$`-ceiling quota charge + 410-Gone
tombstone check) to `api.cloudflare.com/.../d1/.../query` **before** the R2 GET. Cold-start
adds a second-order ~2.5 s. **Argon2id is NOT the dominant cost** — it only runs on a
PAT-gate cache miss and is skipped on the warm path.

## Measured evidence (corelink-prod, re-minted hugit PAT, absent key ⇒ guaranteed 404 miss)

```
req1 (cold):  total 4.00s  ttfb 4.00s  connect 0.10s
req2:         total 1.49s  ttfb 1.49s  connect 0.04s
req3:         total 1.40s  ttfb 1.40s  connect 0.04s
req4:         total 1.46s  ttfb 1.46s  connect 0.04s
req5:         total 1.69s  ttfb 1.69s  connect 0.04s
```

- `connect` ~40 ms ⇒ not network. `ttfb ≈ total`, 404 has no body ⇒ all server-side compute, not transfer.
- **Cold-start ≈ 2.5 s** (req1 − warm). DO starts the container on demand; `IDLE_TIMEOUT_MS = 5 min`.
- **Warm steady-state ≈ 1.5 s/req** and it does **not** decay — because the PAT-gate verify-cache
  (`VERIFY_CACHE_TTL = 5 s`, `native_pat_gate.rs`) makes req2–5 cache **hits** that **skip Argon2id**.
  So 1.5 s is the cost **with Argon2id already skipped** — proving Argon2id is not the warm dominator.

## Architecture (confirmed in code)

`worker/src/index.ts` routes `/v1/*` as `reapi_v1` → forwards to the per-tenant Durable Object
(`durable_object.ts`) → `proxyToContainer` to the **native Rust container** (TCP :50051). The Worker
does **HMAC only** and deliberately skips Argon2id (`index.ts:903-912`, documented posture).

Container CAS read hot path — `crates/corelink-container/src/routes/cas.rs::handle_read` (warm order):

| Step | file:line | Cost |
|------|-----------|------|
| cross-tenant / digest / scope checks | cas.rs:527-541 | in-memory, ~0 |
| PAT possession gate (`pat_gate_reject`) | cas.rs:545 | **warm = cache hit, Argon2id skipped**; miss = Argon2id |
| **`$`-ceiling quota charge** `quota.check()` | cas.rs:552 → `tenant_quota.rs` (`D1QuotaStore` over `d1_http`) | **D1-over-HTTP round-trip (~0.3–0.7 s)** |
| **410 tombstone check** `is_tombstoned()` | cas.rs:564 → `cas_erase.rs` (`D1TombstoneStore` over `d1_http`) | **D1-over-HTTP round-trip (~0.3–0.7 s)** |
| R2 GET (miss) | cas.rs:585 | S3 round-trip (~0.05–0.15 s) |

`D1HttpClient` (`storage/d1_http.rs:92`) targets
`https://api.cloudflare.com/client/v4/accounts/{acct}/d1/database/{db}/query` — the **remote
control-plane HTTP API** (native containers don't get the Worker's fast D1 binding). Two of those in
series + R2 = the measured ~1.5 s warm.

## Remediation — 4 work-packages (separate PRs; distinct risk domains)

Ordered by unblock value. Each WP names the invariant it MUST NOT break.

### WP-1 — Bulk / packfile PUT endpoint  *(biggest ingest win; additive; cross-team contract)*
One request carries many objects ⇒ **one** auth, **one** quota charge, **one** R2 multipart/batch.
Collapses thousands of round-trips (git object closure) into one. Directly answers handoff ask #3.
- **Invariant:** hash-equality enforced per object before commit (no WP-1 shortcut around the
  existing PUT content-addressing check); quota charged for the true byte/op total of the batch
  (no under-charge bypass — sibling of the #318 `$`-ceiling fix).
- **Contract:** payload shape (NDJSON of `{hash,len}` + concatenated bytes, or a real packfile) is a
  hugit↔corelink interface — frozen with the hugit TL before client work. Server side is mine.

### WP-2 — Drop the two D1-over-HTTP hops from the single-object hot path  *(billing + GDPR sensitive)*
- **Quota:** in-container cached remaining-ceiling with async/batched charge accounting.
  - **Invariant (fail-CLOSED, ADR-0068):** must keep the ceiling fail-closed with a **bounded,
    documented, reconciled overshoot** — a charge is NEVER lost (durable accrual), and a tenant
    **definitely** over-ceiling is still refused. Bounded staleness on a *monthly* ceiling is
    acceptable; silent unbounded bypass is not (that was #318).
- **Tombstone:** per-tenant in-memory **bloom filter / set** of erased digests; consult D1 only on a
  bloom hit.
  - **Invariant (GDPR Art.17):** the structure may have **false positives** (extra D1 check — safe)
    but **NEVER false negatives** (an erased object served as 200/404 instead of 410 is a compliance
    breach). Bloom refresh cadence bounds the window in which a freshly-erased digest could be missed;
    that window must be ≤ the existing tombstone-gate staleness posture (today it already fails OPEN on
    a D1 blip — so a bounded refresh is not a regression, but the bound must be explicit + tested).

### WP-3 — Keep the container warm  *(infra; COGS tradeoff — stakeholder-visible)*
Kill the ~2.5 s cold-start for active tenants. The DO already runs a 30 s health alarm; option is to
let active sessions hold the container past `IDLE_TIMEOUT_MS`, or a small warm pool.
- **Invariant (margin):** CoreLink is self-serve SMB with ~80% target margin — warming **every**
  tenant 24/7 is a COGS hit. Warm must be scoped to **recently-active** tenants (session-bounded),
  not global. This WP is a cost/perf tradeoff and is surfaced to the stakeholder, not silently maxed.

### WP-4 — (lower) Argon2id is category-mismatched for PATs
PATs are high-entropy random tokens; Argon2id (memory-hard, for low-entropy passwords) is the wrong
primitive for per-request verification. Cheap interim: raise `VERIFY_CACHE_TTL`. Proper: verify
high-entropy PATs with a fast keyed hash (HMAC/SHA-256 of the stored secret) — preserves the
defense-in-depth (finding #4) second factor without the memory-hard cost. Needs a stored-hash-format
migration ⇒ its own ADR. Only bites on cache miss, so lowest priority.

## Expected result

WP-2 + WP-3 take single-object GET to **≈ R2-GET-only (sub-100 ms warm)**, restoring the viability of
hugit's eager closure prefetch. WP-1 makes bulk ingest a single round-trip regardless. The hugit TL's
prefetch-vs-lazy-LRU decision should be re-made against the **post-WP-2 floor (sub-100 ms)**, not the
current 1.5 s.
