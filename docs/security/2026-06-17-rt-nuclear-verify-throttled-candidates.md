# rt-nuclear VERIFICATION run (post-#324) — THROTTLED, inconclusive + 1 lead-TL-confirmed finding

> Run `wf_696c6cd5-d4e` (2026-06-17). Verification pass against hardened main after the r34 fix-wave
> (#324). **The run was killed by a TRANSIENT PLATFORM throttle** ("Server is temporarily limiting
> requests — not your usage limit"): nearly all r1-r4 hunters AND every refuter failed. So
> `confirmed_exploitable: 0` means **refutation never ran**, NOT "all clean." 20 candidates were
> generated (4 r1 + 16 r2) but none could be refuted. **Re-run scheduled for madrugada (~03:17)** when
> the throttle is lighter. This doc records the candidate leads + the one the lead cold-confirmed.

## Candidate classes that leaked from the failed-refute labels (UNVERIFIED leads)
- **Turbo concurrent** ← LEAD-CONFIRMED below (real HIGH)
- Turbo /v8/artifacts, Turbo overwrite, Turbo failed-PUT, Turbo PUT cross-tenant
- Brew tag-manifest (cache-poisoning — the brew adapter; analogous to the npm metadata fix #320, NOT yet checked)
- OCI bearer token
- Native plane (PAT forgery / backstop)
- CAS/AC delete

These are leads only — the madrugada re-run must refute them; the lead will cold-check each survivor.

## CONFIRMED (lead cold-check, not yet fixed) — Turbo concurrent-PUT `prior_len` TOCTOU re-opens WP-C
**Severity:** HIGH · **Reach:** any free tenant with a `cas:rw` PAT · **Re-open of:** this session's WP-C
(#324, r34 #3/#4/#5/#6 byte-delta fix).

**Root cause:** the byte-delta fix reads `prior_len` via a separate `backend.get()` probe in
`crates/corelink-container/src/storage/r2_kv.rs::write` (~:190), then the route
`crates/corelink-container/src/routes/turbo_v8.rs::handle_put` (~:599) `release`s that `prior_len`.
The `ByteAccountant::accrue`/`release` are each DB-atomic, but the **probe is NOT atomic with the
release across concurrent same-key PUTs**. `TURBO_PUT_CONCURRENCY_LIMIT = 4` allows up to 4 concurrent
PUTs per tenant.

**Exploit chain:**
1. PUT key `K` with a 100 MiB body (one PUT). `bytes_used += 100 MiB`. R2 holds 100 MiB at `K`.
2. Fire 2-4 CONCURRENT 1-byte PUTs to the SAME key `K` (within the cap). Interleaving:
   probe_A → probe_B (both read `prior=100 MiB`) → put_A(1B) → release_A(100 MiB) → put_B(1B) →
   release_B(100 MiB).
3. Net: `bytes_used += 2` then `-= 200 MiB` (saturating). The tenant's counter drops by ~100 MiB
   while real on-disk usage is unchanged (K is now 1 byte; OTHER objects untouched).
4. Re-grow K to 100 MiB (one PUT) and repeat. Each cycle: `+100 MiB (regrow) − 200 MiB (double
   release) = −100 MiB`. Drive `bytes_used → 0` while real storage stays at the quota → store up to
   `bytes_quota` of real data for $0. **Storage-cap / margin evasion — exactly what WP-C set out to
   close, re-opened via concurrency.** Rate-limited by the cap (≤4×/round) but unbounded over rounds.

**Fix options (lead to decide at fix time):**
- (A) **Per-(tenant,key) async serialization** around accrue→put→reconcile in `handle_put` (different
  keys still concurrent up to the cap). Single-instance-correct, minimal. Residual: multi-writer
  across container instances (note + assess whether the data-plane container is per-tenant-singleton
  behind its DO — if so, fully correct).
- (B) **R2 conditional write (If-Match/etag) with retry** in the Turbo KV path so `prior_len` is the
  truly-overwritten size (the CAS plane already uses R2 If-Match for multi-writer durability). Robust
  multi-writer; bigger change (extend the `KvBackend` port). PREFERRED if the container scales out.

**Plan:** fix in the post-madrugada bundle (CI economy — the re-run will likely surface more to bundle),
or sooner on owner request. Add a regression test: N concurrent same-key shrink-PUTs must net the true
delta (no double-release / no underflow).

---

## FULL TRIAGE — all 7 leaked candidates, LEAD-confirmed against current main (2026-06-17, owner: carry on)
The owner asked to act on what the throttled run found. The lead cold-checked all 7 candidate classes
against the code (the refuters never ran). **All 7 confirmed real.** Deduped → 4 work-packages, fixed
in one bundle (the team implemented A/B/D; the lead did C + integration). Mapping:

| ID | Finding (confirmed) | Plane | Sev | WP | Fix |
|----|---------------------|-------|-----|----|-----|
| C1 | Turbo write byte-accounting TOCTOU: concurrent same-key PUTs double-release `prior_len` → unbounded free storage (3 candidate variants: overwrite / concurrent-under-count / failed-PUT — ONE root: no per-key serialization on the write path) | Turbo | HIGH | A | per-(tenant,key) sharded async lock around accrue→put→release in `handle_put` (1024 shards, memory-bounded) |
| C4 | `/v8/artifacts/events` OOM: inherits the 100 MiB body limit, no `PutConcurrencyGuard` → concurrent telemetry POSTs OOM the shared container | Turbo | MED-HIGH | A | events route gets its own 64 KiB `DefaultBodyLimit` + the global guard |
| C5 | No GLOBAL in-flight byte budget: per-tenant cap (4×100 MiB) only → N tenants → OOM | Turbo/container | MED-HIGH | A | process-wide `Semaphore` (16 permits) reserved in a `FromRequestParts` extractor before body buffering; 503 on saturation |
| C2 | CAS write-vs-delete byte race: write & delete decorators don't share a per-key lock → stale `reclaimed_bytes` release under-counts `bytes_used` | native CAS | MED-HIGH | B | per-(tenant,hash) sharded lock (256) lifted to `AccountingCasHandler`, held by BOTH `write()` and `delete()` |
| C3 | Native plane honors a revoked PAT for the verify-cache TTL (cache hit returns Ok with no D1 re-check) | native | MED | C | `VERIFY_CACHE_TTL` 60s → 5s (bounded revocation latency; no hot-path D1) |
| C6 | OCI realm bearer (stateless HMAC, 1h) never re-checks D1 revocation → revoked PAT keeps registry r/w up to 60 min | OCI | MED | C | `TOKEN_TTL_SECS` 3600 → 300 (industry-standard registry bearer TTL; clients re-auth on 401) |
| C7 | Brew non-digest bytes stored into the SHARED `_public` namespace with `verified_sha256=false` → *claimed* cross-tenant cache-poisoning | brew | **DEFERRED** | D (reverted) | **NOT shipped** — passthrough fix REVERTED (broke the all-public moat cache feature + 3 tests; C7 unrefuted). See note below. |

### C7 — REVERTED + DEFERRED to the madrugada refutation (lead decision, 2026-06-17)
On integration, WP-D's passthrough broke the brew caching feature + 3 behavior tests
(`tenant_isolation_holds`, `second_request_is_a_cache_hit`, `url_variants_collapse_to_single_cache_entry`)
because **brew is INTENTIONALLY all-public-shared** (`BrewMoatStore` ignores `tenant_id` →
`PUBLIC_NAMESPACE`, the network-effect moat) and Homebrew bottles are fetched by **named non-digest
paths** (`curl-8.5.0.bottle.tar.gz`), not `sha256:` — so passthrough disables bottle caching for the
common case, not just tiny manifests. Re-examining exploitability: the cache key `blake3(path)` is tied
to the **ghcr origin path**, so two tenants requesting the same path fetch the same authoritative ghcr
bytes — no clean cross-tenant poison for a stable bottle; the real residual is mutable-tag staleness
(lower severity). And C7 was **never refuted** (5-skeptic panel throttled). Shipping a feature-breaking
change for an unconfirmed finding is the wrong rigor → WP-D reverted. **C7 → madrugada nuclear for proper
refutation;** if confirmed, the fix is digest-binding / mutable-tag revalidation, NOT cache-disable. This
wave ships **C1-C6 only.**

**Residual / follow-ups (surfaced, not silent debt):**
- AC plane (update-vs-delete) is a likely sibling of C2 — WP-B was frozen+verified for CAS; flagged by the
  agent. Re-check in the madrugada run; fix if confirmed.
- C3/C6 use bounded-TTL (the industry-standard control for cached/stateless creds; revocation ≤5s native,
  ≤5min OCI; zero added hot-path D1 latency). A cross-process revocation epoch (sub-second propagation
  without per-request D1) is an OPTIONAL future perf optimization, not a security gap.
- The madrugada nuclear re-run must re-verify all 7 are closed + refute the OCI-bearer/brew claims with
  the full 5-skeptic panel (this pass was lead-confirmed only, refuters throttled).
