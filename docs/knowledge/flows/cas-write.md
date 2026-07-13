---
type: "RequestFlow"
title: "CAS write flow"
description: "End-to-end path of a native CAS blob write: the tenant/scope/PAT/$-ceiling gauntlet at the route, then the byte-accounted commit through to the R2 S3 bucket."
source_files:
  - "crates/corelink-container/src/routes/cas.rs"
  - "crates/corelink-container/src/storage.rs"
checkpoint_sha: "ab4e1a5f80d90d5e95e4ec8f477c3f64cbaa5984"
provenance: "AUTHORED"
tags: ["flows", "cas", "hot-path", "storage", "request-flow"]
timestamp: "2026-06-28T00:00:00Z"
---

# CAS write flow

A `PUT /v1/cas/:tenant/:hash` is the single most-walked billable path in CoreLink, and it is also the one a malicious tenant most wants to abuse — to write across a tenant boundary, to dodge the spend ceiling, or to exhaust the box with concurrent uploads. So the write handler runs a fixed gauntlet of cheap-to-expensive checks BEFORE it ever touches storage, then commits the bytes through ONE shared, byte-accounted chokepoint into the R2 S3 bucket. This concept traces that path across the container route plane (`routes/cas.rs`) and the native R2 storage adapter (`storage.rs`); the two heavyweight gates it invokes are documented in their own flows: the [PAT verification gauntlet](/flows/pat-gauntlet.md) and the [billing quota check](/flows/billing-quota-check.md).

# Role

The handler is the trust boundary between the Worker/DO edge (which injects the resolved `x-corelink-tenant-id`) and durable R2 storage. It enforces isolation, content-address integrity, write scope, possession, and the monthly $-ceiling — in that order, fail-closed — so storage is only ever reached by a request that has paid every toll.

# How it works

1. Pre-body concurrency reservation: a `CasPutGuard` extractor is declared AHEAD of the body so axum runs it before the upload is buffered; it reserves one per-tenant in-flight slot and rejects over-cap writes 429 before any bytes are read (`crates/corelink-container/src/routes/cas.rs:284-336`).
2. Cross-tenant denial: the client-echoed path `:tenant` must equal the authenticated tenant `auth.0` (the sole isolation key); a mismatch is a 403 before any storage access (`crates/corelink-container/src/routes/cas.rs:891-892`).
3. Hash validation: a non-canonical CAS digest is rejected 400 before it can derive an R2 key (`crates/corelink-container/src/routes/cas.rs:895-896`).
4. Write-scope gate (fail-closed): the PAT must carry a cache-WRITE capability; a read-only token is rejected 403 here (`crates/corelink-container/src/routes/cas.rs:901-902`).
5. Native PAT write-capability gate: `pat_gate_reject_write` reads the bearer PAT and independently re-derives the PAT's D1-stored `can_write` capability at the container via `NativePatGate::verify_write` (not just tenant possession — the full Argon2id [PAT verification gauntlet](/flows/pat-gauntlet.md) plus the write bit), AFTER scope and BEFORE storage; a read-only PAT is rejected 403 even if the Worker-set scope header claimed write, upholding the Option-B invariant that a compromised Worker cannot grant write on its own (`crates/corelink-container/src/routes/cas.rs:908`; helper at `crates/corelink-container/src/routes/cas.rs:753-764`, `verify_write` call at `crates/corelink-container/src/routes/cas.rs:763`).
6. Monthly $-ceiling gate: when wired, `state.quota.check(&auth.0)` runs the [billing quota check](/flows/billing-quota-check.md) and rejects 402/503 before storage (`crates/corelink-container/src/routes/cas.rs:913-917`).
7. Storage-cap threading: the Worker-resolved per-tier storage cap header is threaded into the write request so the accounting decorator seeds a real cap, never treating an unseeded tenant as unlimited (`crates/corelink-container/src/routes/cas.rs:919-931`).
8. Commit: `state.write.write(req)` goes through the byte-accounting decorator — the single chokepoint every CAS surface shares — which reserves/commits the cap atomically before the R2 PUT (`crates/corelink-container/src/routes/cas.rs:941-956`).
9. The production write target is the native R2 S3 adapter (`R2CasHandler`), built from S3 credentials only when they are present in env (`crates/corelink-container/src/routes/cas.rs:526-638`); the adapter module and its credential loader live in storage (`crates/corelink-container/src/storage.rs:35`, `crates/corelink-container/src/storage.rs:98-114`).
10. Result: a fresh durable write returns 201 CREATED, an idempotent already-present write returns 200 OK (`crates/corelink-container/src/routes/cas.rs:941-956`).
11. Bulk reads are symmetrically capped: a `CasReadConcurrencyGuard` extractor — declared AHEAD of the body on `handle_batch_read`/`handle_batch_exists`, like `CasPutGuard` on writes — reserves one per-tenant in-flight READ slot from a SEPARATE `read_inflight` pool (cap 8) and rejects over-cap bulk reads 429 before any payload is buffered (`crates/corelink-container/src/routes/cas.rs:382-426`).
12. After a successful commit, the handler records a fire-and-forget `Write` usage-metering event into the in-process display aggregator [`crate::usage_meter`] (both a fresh 201 and an idempotent 200 count as a write) — no await / no I/O on the hot path, DISPLAY telemetry only, never gating the commit (`crates/corelink-container/src/routes/cas.rs:945-947`).

# Invariants

- The authenticated tenant `auth.0`, not the path `:tenant`, is the isolation key; the path is a client echo that must match or the request 403s before storage (`crates/corelink-container/src/routes/cas.rs:891-892`).
- Every gate runs BEFORE storage and fails closed: scope (`crates/corelink-container/src/routes/cas.rs:901-902`), write-capability (`pat_gate_reject_write` re-derives D1 `can_write` at the container, `crates/corelink-container/src/routes/cas.rs:908`), $-ceiling (`crates/corelink-container/src/routes/cas.rs:913-917`).
- The per-tenant write-concurrency cap is checked before the body is buffered, bounding peak per-tenant memory (`crates/corelink-container/src/routes/cas.rs:284-336`); the bulk-read path mirrors this on a SEPARATE per-tenant cap (`crates/corelink-container/src/routes/cas.rs:382-426`).
- The byte-cap is enforced atomically at the shared write trait object, not in the route, so every surface accounts identically and the route never double-counts (`crates/corelink-container/src/routes/cas.rs:941-956`).
- Usage metering is off the commit path: the `Write` `record(...)` runs only AFTER a successful `state.write.write` and never gates or fails the write — DISPLAY telemetry, not billing (`crates/corelink-container/src/routes/cas.rs:945-947`).
- R2 S3 credentials are env-sourced only and the real adapter is constructed solely when all creds are present; absent creds fall back to the dev/in-memory path (`crates/corelink-container/src/storage.rs:98-114`; `crates/corelink-container/src/routes/cas.rs:516-626`).

# Gotchas

- A CAS 401 on this path means the PAT failed the possession gate (bad key OR no D1 row), NOT a malformed hash — that is a distinct 400. A 402 means the monthly ceiling tripped; a 503 means a gate's backend (D1 / quota store) was unreachable and the write fail-closed rather than serve uncounted.
- `state.write.write` is synchronous in the handler, but the byte-accounting + R2 PUT happen inside the decorated trait object; an over-cap or accounting fault surfaces as a sentinel-tagged error mapped to 402/503, not as a panic.
- The batch route (`POST /v1/cas/:tenant/batch`) mirrors this exact gate sequence but charges the quota ONCE for the whole batch, never per object.
- After the gauntlet, for a `tenant_byok_config.state='active'` tenant the committed bytes are enveloped at rest (convergent AES-256-GCM) inside the R2 adapter before the PUT, and decrypted on read before the content-address integrity check — gated-inert today (zero active tenants); see [BYOK envelope encryption at rest](/storage/byok-envelope-encryption.md).

# Citations

1. `crates/corelink-container/src/routes/cas.rs:284-336` — pre-body per-tenant write-concurrency reservation (429 before buffering).
2. `crates/corelink-container/src/routes/cas.rs:526-638` — `build_handlers`: builds the real `R2CasHandler` only with S3 creds present, else in-memory.
3. `crates/corelink-container/src/routes/cas.rs:753-764` — `pat_gate_reject_write` helper: re-derives the PAT's D1 `can_write` via `NativePatGate::verify_write` (line 763), the write-path variant of `pat_gate_reject`.
4. `crates/corelink-container/src/routes/cas.rs:876-956` — `handle_write`: cross-tenant / hash / scope / PAT-write-capability / quota gates, then the byte-accounted commit and 201/200 result.
5. `crates/corelink-container/src/routes/cas.rs:891-892` — cross-tenant 403 (authenticated tenant is the isolation key).
6. `crates/corelink-container/src/routes/cas.rs:895-917` — hash, write-scope, PAT-write-capability, and $-ceiling gates, all before storage.
6b. `crates/corelink-container/src/routes/cas.rs:908` — the `pat_gate_reject_write` call in `handle_write` (read-only PAT ⇒ 403 even if scope header claimed write).
7. `crates/corelink-container/src/routes/cas.rs:919-956` — storage-cap header threading and the byte-accounted commit chokepoint.
8. `crates/corelink-container/src/routes/cas.rs:382-426` — `CasReadConcurrencyGuard`: per-tenant bulk-READ concurrency cap (separate `read_inflight` pool), 429 before buffering — the read twin of `CasPutGuard`.
9. `crates/corelink-container/src/storage.rs:35` — the native `r2_s3` adapter module (R2 over the S3-compatible API).
10. `crates/corelink-container/src/storage.rs:98-114` — `StorageEnv::from_env`: env-only R2/D1 credential loading.
11. `crates/corelink-container/src/routes/cas.rs:945-947` — the fire-and-forget `Write` usage-metering `record` (DISPLAY telemetry, off the commit path).
</content>
