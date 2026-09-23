---
type: "RequestFlow"
title: "CAS write flow"
description: "End-to-end path of a native CAS blob write: the tenant/scope/PAT/$-ceiling gauntlet at the route, then the byte-accounted commit through to the R2 S3 bucket."
source_files:
  - "crates/corelink-container/src/routes/cas/foundation_state.rs"
  - "crates/corelink-container/src/routes/cas/single_handlers.rs"
  - "crates/corelink-container/src/routes/cas/batch_write.rs"
  - "crates/corelink-container/src/routes/cas/single_setup.rs"
  - "crates/corelink-container/src/storage/r2_s3_parts/cas_builder.rs"
  - "crates/corelink-container/src/storage/r2_s3_parts/cas_write.rs"
  - "crates/corelink-container/src/byte_accounting/b126_m2_impl_01_part_02.rs"
source_blobs:
  - "crates/corelink-container/src/routes/cas/foundation_state.rs@8dcd20ecd23850e331763d5ccfcf4d0260489f0e"
  - "crates/corelink-container/src/routes/cas/single_handlers.rs@f4259f925d39cff5d70ae6fb8fc5996f2109dba2"
  - "crates/corelink-container/src/routes/cas/batch_write.rs@3561b0e3188b5ff05ff5998fdad3f33ee5f6ad78"
  - "crates/corelink-container/src/routes/cas/single_setup.rs@27e4090d053420fa0bfe07ee655d9c6ce6573439"
  - "crates/corelink-container/src/storage/r2_s3_parts/cas_builder.rs@46958333012cea23f7357887d91e105c5d4f50a2"
  - "crates/corelink-container/src/storage/r2_s3_parts/cas_write.rs@83438b03872e8204c62a32ad09137dd9b3da52fb"
  - "crates/corelink-container/src/byte_accounting/b126_m2_impl_01_part_02.rs@3e62bda2ed171001cf74b084d36bd61e6c1d0d39"
checkpoint_sha: "a65c7d7caed03adf00acd3a227dc20c4e857f7f0"
provenance: "AUTHORED"
tags: ["flows", "cas", "hot-path", "storage", "request-flow"]
timestamp: "2026-06-28T00:00:00Z"

---
# CAS write flow

A `PUT /v1/cas/:tenant/:hash` is the single most-walked billable path in CoreLink, and it is also the one a malicious tenant most wants to abuse — to write across a tenant boundary, to dodge the spend ceiling, or to exhaust the box with concurrent uploads. So the write handler runs a fixed gauntlet of cheap-to-expensive checks BEFORE it ever touches storage, then commits the bytes through ONE shared, byte-accounted chokepoint into the R2 S3 bucket. This concept traces that path across the container route plane (`routes/cas.rs`) and the native R2 storage adapter (`storage.rs`); the two heavyweight gates it invokes are documented in their own flows: the [PAT verification gauntlet](/flows/pat-gauntlet.md) and the [billing quota check](/flows/billing-quota-check.md).

# Role

The handler is the trust boundary between the Worker/DO edge (which injects the resolved `x-corelink-tenant-id`) and durable R2 storage. It enforces isolation, content-address integrity, write scope, possession, and the monthly $-ceiling — in that order, fail-closed — so storage is only ever reached by a request that has paid every toll.

# How it works

1. Pre-body concurrency reservation: a `CasPutGuard` extractor is declared AHEAD of the body so axum runs it before the upload is buffered; it reserves one per-tenant in-flight slot and rejects over-cap writes 429 before any bytes are read (`crates/corelink-container/src/routes/cas/foundation_state.rs:163-205`).
2. Cross-tenant denial: the client-echoed path `:tenant` must equal the authenticated tenant `auth.0` (the sole isolation key); a mismatch is a 403 before any storage access (`crates/corelink-container/src/routes/cas/single_handlers.rs:133-211`).
3. Hash validation: a non-canonical CAS digest is rejected 400 before it can derive an R2 key (`crates/corelink-container/src/routes/cas/single_handlers.rs:133-211`).
4. Write-scope gate (fail-closed): the PAT must carry a cache-WRITE capability; a read-only token is rejected 403 here (`crates/corelink-container/src/routes/cas/single_handlers.rs:133-211`).
5. Native PAT write-capability gate: `pat_gate_reject_write` reads the bearer PAT and independently re-derives the PAT's D1-stored `can_write` capability at the container via `NativePatGate::verify_write` (not just tenant possession — the full Argon2id [PAT verification gauntlet](/flows/pat-gauntlet.md) plus the write bit), AFTER scope and BEFORE storage; a read-only PAT is rejected 403 even if the Worker-set scope header claimed write, upholding the Option-B invariant that a compromised Worker cannot grant write on its own (`crates/corelink-container/src/routes/cas/single_handlers.rs:155-167`; helper at `crates/corelink-container/src/routes/cas/single_setup.rs:310-329`).
6. Monthly $-ceiling gate: when wired, `state.quota.check(&auth.0)` runs the [billing quota check](/flows/billing-quota-check.md) and rejects 402/503 before storage (`crates/corelink-container/src/routes/cas/single_handlers.rs:133-211`).
7. Storage-cap threading: the Worker-resolved per-tier storage cap header is threaded into the write request so the accounting decorator seeds a real cap, never treating an unseeded tenant as unlimited (`crates/corelink-container/src/routes/cas/single_handlers.rs:175-198`).
8. Commit: `state.write.write(req)` goes through the byte-accounting decorator — the single chokepoint every CAS surface shares — which reserves/commits the cap atomically before the R2 PUT (`crates/corelink-container/src/routes/cas/single_handlers.rs:189-212`; `crates/corelink-container/src/byte_accounting/b126_m2_impl_01_part_02.rs:227-318`).
9. The production write target is the native R2 S3 adapter (`R2CasHandler`), built from S3 credentials only when they are present in env (`crates/corelink-container/src/routes/cas/single_setup.rs:73-127`); the adapter module and its credential loader live in storage (`crates/corelink-container/src/storage/r2_s3_parts/cas_builder.rs:10-38`, `crates/corelink-container/src/routes/cas/single_setup.rs:80-127`).
10. Result: a fresh durable write returns 201 CREATED, an idempotent already-present write returns 200 OK (`crates/corelink-container/src/routes/cas/single_handlers.rs:133-211`).
11. Bulk reads are symmetrically capped: a `CasReadConcurrencyGuard` extractor — declared AHEAD of the body on `handle_batch_read`/`handle_batch_exists`, like `CasPutGuard` on writes — reserves one per-tenant in-flight READ slot from a SEPARATE `read_inflight` pool (cap 8) and rejects over-cap bulk reads 429 before any payload is buffered (`crates/corelink-container/src/routes/cas/foundation_state.rs:262-305`).
12. After a successful commit, the handler records a fire-and-forget `Write` usage-metering event into the in-process display aggregator [`crate::usage_meter`] (both a fresh 201 and an idempotent 200 count as a write) — no await / no I/O on the hot path, DISPLAY telemetry only, never gating the commit (`crates/corelink-container/src/routes/cas/single_handlers.rs:198-210`).

# Invariants

- The authenticated tenant `auth.0`, not the path `:tenant`, is the isolation key; the path is a client echo that must match or the request 403s before storage (`crates/corelink-container/src/routes/cas/single_handlers.rs:133-211`).
- Every gate runs BEFORE storage and fails closed: scope (`crates/corelink-container/src/routes/cas/single_handlers.rs:155-160`), write-capability (`pat_gate_reject_write` re-derives D1 `can_write` at the container, `crates/corelink-container/src/routes/cas/single_handlers.rs:161-167`, `crates/corelink-container/src/routes/cas/single_setup.rs:310-329`), $-ceiling (`crates/corelink-container/src/routes/cas/single_handlers.rs:168-174`).
- The per-tenant write-concurrency cap is checked before the body is buffered, bounding peak per-tenant memory (`crates/corelink-container/src/routes/cas/foundation_state.rs:163-205`); the bulk-read path mirrors this on a SEPARATE per-tenant cap (`crates/corelink-container/src/routes/cas/foundation_state.rs:262-305`).
- The byte-cap is enforced atomically at the shared write trait object, not in the route, so every surface accounts identically and the route never double-counts (`crates/corelink-container/src/routes/cas/single_handlers.rs:189-198`, `crates/corelink-container/src/byte_accounting/b126_m2_impl_01_part_02.rs:227-318`).
- Usage metering is off the commit path: the `Write` `record(...)` runs only AFTER a successful `state.write.write` and never gates or fails the write — DISPLAY telemetry, not billing (`crates/corelink-container/src/routes/cas/single_handlers.rs:198-210`).
- R2 S3 credentials are env-sourced only and the real adapter is constructed solely when all creds are present; absent creds fall back to the dev/in-memory path (`crates/corelink-container/src/routes/cas/single_setup.rs:80-127`; `crates/corelink-container/src/routes/cas/single_setup.rs:73-127`).

# Gotchas

- A CAS 401 on this path means the PAT failed the possession gate (bad key OR no D1 row), NOT a malformed hash — that is a distinct 400. A 402 means the monthly ceiling tripped; a 503 means a gate's backend (D1 / quota store) was unreachable and the write fail-closed rather than serve uncounted.
- `state.write.write` is synchronous in the handler, but the byte-accounting + R2 PUT happen inside the decorated trait object; an over-cap or accounting fault surfaces as a sentinel-tagged error mapped to 402/503, not as a panic.
- The batch route (`POST /v1/cas/:tenant/batch`) mirrors this exact gate sequence but charges the quota ONCE for the whole batch, never per object.
- After the gauntlet, for a `tenant_byok_config.state='active'` tenant the committed bytes are enveloped at rest (convergent AES-256-GCM) inside the R2 adapter before the PUT, and decrypted on read before the content-address integrity check. The shipped image selects the real KMS boundary; owner credentials and active tenant provisioning remain deployment evidence. See [BYOK envelope encryption at rest](/storage/byok-envelope-encryption.md).

# Citations

1. `crates/corelink-container/src/routes/cas/single_handlers.rs:133-211` — single PUT gates and ordering: authenticated tenant, canonical digest, write scope, native PAT write capability, quota, then the shared write handler and 201/200 result.
2. `crates/corelink-container/src/routes/cas/batch_write.rs:45-197` — batch PUT preserves the corresponding gate order, validates framing and caps before commits, charges quota once for the batch, then writes each object through the same handler.
3. `crates/corelink-container/src/routes/cas/foundation_state.rs:163-205` — per-tenant write reservation before body buffering; `crates/corelink-container/src/routes/cas/foundation_state.rs:262-305` — separate per-tenant read reservation.
4. `crates/corelink-container/src/routes/cas/single_setup.rs:73-127` — the route builds R2 handlers when storage credentials exist and mounts a fail-closed unavailable handler if construction refuses; `crates/corelink-container/src/routes/cas/single_setup.rs:115-127` shows the shared read/write handler instance.
5. `crates/corelink-container/src/storage/r2_s3_parts/cas_builder.rs:10-38` — R2 handler construction requires the tenant-derivation key when storage credentials are configured.
6. `crates/corelink-container/src/storage/r2_s3_parts/cas_write.rs:2-76` — the durable writer validates the claimed digest before committing.
7. `crates/corelink-container/src/byte_accounting/b126_m2_impl_01_part_02.rs:227-318` — shared accounting decorator reserves and commits the byte cap at the write trait object.
