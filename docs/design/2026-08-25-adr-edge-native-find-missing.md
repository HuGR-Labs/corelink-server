# ADR — `findMissingBlobs` belongs at the edge, not in the container

- **Status:** proposed
- **Date:** 2026-08-25
- **Supersedes nothing.** Extends the F3.3 edge-native read
  (`2026-08-16-adr-worker-native-public-cache-read.md`) from the `_public`
  namespace to a per-tenant, authenticated batch route.
- **Related:** #1328 (batched audit + concurrent probes),
  `2026-08-25-adr-container-d1-transport.md` (the same "the public frontend is
  the cost" finding, on D1 rather than R2).

## The target

The owner set the ceiling for the authed hot path at **15 ms**, and asked for at
least a 70 % cut on `findMissingBlobs`. Today n=100 costs **8.6-9.1 s**. 70 %
would be 2.7 s. The ceiling implies something far below that.

## What is measured (all from inside the fabric, colo CMH/IAD, same tenant)

Baseline before #1328, and after it:

| n digests | before #1328 | after #1328 |
|---|---|---|
| 5 | 2.10 s | 0.65-1.22 s |
| 20 | 5.74 s | 2.20-2.24 s |
| 50 | 13.48 s | 4.68-4.85 s |
| 100 | 26.86 s | 8.98-9.70 s |

#1328 removed the per-digest audit `INSERT` (~183 ms/digest of the original
268 ms). What remains is ~85 ms per digest — and it is **linear**.

The size ladder (run 32902312119) shows no plateau where the 16-way `buffered`
fan-out should have produced one:

| n | 1 | 2 | 4 | 8 | 16 | 32 | 64 | 100 |
|---|---|---|---|---|---|---|---|---|
| s | 0.35 | 0.46 | 0.58 | 1.04 | 1.67 | 3.06 | 5.68 | 8.65 |

Marginal cost is flat: 87 ms/digest from 16→32, 82 ms from 32→64.

The discriminator (run 32903468602) separates "our fan-out is broken" from
"the container is a shared wall":

- **16 SEPARATE single-digest requests, fired concurrently: 1.19 s wall.**
  Serially they would be ~5.6 s, so the container *does* overlap work across
  tasks — but only ~4.7x, not 16x.
- **ONE n=16 request: 1.59-1.82 s** — no better than the 16 independent
  requests, and slightly worse.

So the ceiling is **shared and outside our fan-out**: roughly **13 digests per
second** on a 0.25-vCPU instance talking to R2 over the public S3 endpoint. Two
mechanisms fit (CPU-bound TLS on a quarter-core; R2 throttling the burst with
SDK backoff) and this ADR does not need to choose between them, because the
remedy is the same and neither is fixable by tuning our own concurrency.

**The consequence that matters:** 100 digests in 2.7 s needs ~37 digests/s —
about 3x the observed ceiling. In-container tuning cannot get there. The 15 ms
ceiling is not even in the same universe.

## Decision

Serve `findMissingBlobs` **at the Worker edge**, using the native `CAS_BUCKET`
R2 binding, and keep the container only as the fallback.

The Worker already has everything the route needs:

1. **PAT verify** — already at the edge, 3-tier cached, ~0 ms warm.
2. **Scope gate** — the Worker sets the server-trusted scope header; read ⊇
   find-missing (ADR-0071) is a string check.
3. **Tenant == `:instance`** — already enforced at the edge (spoof guard).
4. **Key derivation** — `<region>/<tenant_prefix>/bazel/sha256/<digest>` for
   REAPI sha256 digests, `<region>/<tenant_prefix>/<digest>` for native BLAKE3
   (`R2S3Client::blob_key`). `tenant_prefix` is
   `base64url_nopad(HMAC-SHA256(tdk, uuid16_be))[..16]` — the SAME primitive
   `edge_public_read.ts::derivePublicPrefix` already implements and proves
   against a real prod TDK; it needs generalising from the `_public` UUID to
   an arbitrary tenant UUID.
5. **Existence probe** — `env.CAS_BUCKET.head(key)`, in-colo, no TLS handshake,
   no SigV4, genuinely concurrent.
6. **Audit rows** — the same JSON1 batch `INSERT` #1328 already writes, via the
   `CONFIG_DB` binding instead of the D1 REST frontend (~9 ms from our edge vs
   ~95 ms via `api.cloudflare.com`, measured in the D1-transport ADR). The
   `region` column MUST keep the
   `COALESCE((SELECT primary_region FROM tenant …), 'wnam')` form — relying on
   the column default is what caused the 2026-07-17 residency-trigger incident.
7. **Quota** — already one batched call, charged per digest (F12).

Expected shape: one D1 quota call + N concurrent R2 `head`s + one D1 audit
batch. That is roughly `9 ms + max(head) + 9 ms` rather than `N × 85 ms`.

## F1 SHADOW RESULT (2026-08-25, measured — supersedes every estimate above)

Flag armed on `prod` (iad) only, driven from the fabric, `wrangler tail` read
back 43 shadow verdicts.

**Parity: 43/43 `match`, zero divergence.** The edge answer equalled the
container's on every request, at every size.

Edge wall time, from the shadow's own clock:

| n | 1 | 2 | 4 | 8 | 16 | 32 | 64 | 100 |
|---|---|---|---|---|---|---|---|---|
| edge ms | 110-195 | 114-147 | 136-155 | 151-291 | 346-371 | 467-591 | 1148-1304 | 2084-2141 |

Against the container on the same route: **8.65 s → 2.10 s at n=100, a 76 % cut**,
and ~85 ms/digest → ~20 ms/digest marginal.

That clears the 70 % ask. It does NOT clear the 15 ms ceiling, and the shape says
exactly why: **the edge is linear too.** n=1 costs ~110 ms, and n=100 costs
almost precisely 17 × that. Workers cap simultaneous outbound connections at 6,
so 100 `head()`s are ~17 waves of ~120 ms, not one parallel burst. `Promise.all`
does not buy what it looks like it buys.

**The corrected conclusion: the probe itself is the wrong primitive.** Moving it
in-colo removed the container's 85 ms constant and replaced it with a 20 ms one;
it did not remove the per-digest round trip, and no amount of concurrency tuning
will, because the limit is the connection cap rather than the work.

To reach 15 ms, `findMissingBlobs` must become ONE lookup rather than N. The
candidate already exists: `blob_meta` (migration 0001) is
`PRIMARY KEY (tenant_id, digest)` with a `deleted_at IS NULL` partial index and
is described as the single source of truth for CAS existence, written in the
same `db.batch` as the audit row. If that holds for the REAPI/bazel plane, the
whole route collapses to one `json_each` query over the `CONFIG_DB` binding —
~9 ms from our edge, independent of n.

**REFUTED, same day, before anything was built on it.** `blob_meta` in prod
CONFIG_DB is **empty — 0 rows, 0 tenants, no `created_at`** — while
`audit_outbox` took 3939 read-audit rows across 8 tenants in the same 7 days. The
live container CAS write path does not populate it; the only writers in the tree
are GC (`refcount` / `deleted_at` UPDATEs) and the LRU tracker
(`update_last_accessed_at_ms`), both of which UPDATE rows that nothing INSERTs.
Answering `findMissingBlobs` from that table would report **every** blob as
missing, and a cache client responds to that by re-uploading its entire build.

This is the `designed-vs-wired` trap in its purest form: the migration comment
calls the table "single source of truth for CAS existence" and the schema is
exactly right for the job. The table is real. The maintenance is not.

**Consequences.** F2 ships the R2-binding path measured above — a real, parity-
proven 76 %. A flat-in-n answer still needs an index, but building one is a
project, not a query: the write path has to maintain it, existing blobs need a
backfill, and someone has to decide what happens when the index says "missing"
and R2 disagrees (the safe answer — treat the index as authoritative only for
PRESENT and fall back to a probe for absent — costs exactly the round trips it
was meant to remove for a cold cache). That is its own ADR, with its own
measurement, not a footnote to this one.

The original text is kept below for the record of what was believed:

**That "if" was load-bearing and was NOT verified when written.** If `blob_meta` is not
maintained for bazel-plane blobs, answering from it would report PRESENT blobs
as missing, and a cache client responds to that by re-uploading everything. The
next step is to prove which writers maintain it, not to assume the table means
what its comment says (`designed-vs-wired`). Until then, F2 ships the R2-binding
edge path measured above — a real 76 % — and the index is a separate decision
with its own proof.

## Explicitly NOT decided here

- **A number.** No latency is promised in this ADR. The F1 shadow measures it,
  and if the edge does not beat the container the flag is never flipped.

## Fail-closed constraints (violating any is a security regression)

- **BYOK tenants fall back to the container, always.** The physical R2 key for
  an active BYOK tenant is an HMAC of the logical digest
  (`ByokResolved::physical_digest`), resolved through the TCS. The edge does not
  have that resolver. A BYOK tenant probed with a plaintext key would report
  every blob missing — a correctness AND isolation failure. Gate on the tenant's
  BYOK config being absent/inactive, fail-closed to the container when unknown.
- **Cross-tenant denial before any dispatch.** The whole digest list is scanned
  before a single `head` is issued, exactly as `exists_batch` does — a poisoned
  digest must not let the good ones touch R2 first.
- **The audit result gates the response.** No probe result reaches the caller
  unless its audit rows committed. Concurrency may change what is *dispatched*,
  never what can *reach the caller*.
- **Subrequest budget.** Each `head` is a subrequest. `FIND_MISSING_BLOB_CAP` is
  4096, well over a Worker's per-request subrequest limit, so the edge path
  serves up to a documented N and falls back to the container above it. The cap
  is a constant with a test, not a guess.
- **Region.** The edge read uses THIS worker's own `R2_CAS_REGION` and bucket,
  which is correct by construction only *after* the residency fan-out has early-
  returned for non-local tenants — the same placement F3.3 relies on.

## Rollout

- **F0 (no deploy)** — generalise `derivePublicPrefix` to any tenant UUID; port
  `blob_key`; parity-test both against Rust-emitted vectors, including a
  sha256/REAPI key and a BLAKE3 key.
- **F1 shadow** — compute the edge answer in `ctx.waitUntil`, serve the
  container's, log divergence. Require **100 % parity** on real traffic before
  anything flips, and measure the edge's own timing in the same pass.
- **F2 serve** — flag flip; any miss/error/uncertainty falls through to the
  container unchanged. Rollback is unsetting the flag.
- **F3 red-team** — cross-tenant, BYOK-active, revoked-PAT, spoofed instance,
  and over-cap batches, before the flag is on for anyone but us.

## Why not just fix the container

Because the measurement says the container is not the thing to fix: 16
independent requests hit the same ~13 digests/s ceiling as one batched request,
so the limit is the instance and its path to R2, not our code. Raising the
instance size buys a linear multiple of a bad constant and costs money on every
tenant; moving the probe in-colo removes the constant.
