---
type: "StorageComponent"
title: "R2 CAS bucket topology"
description: "How the native container durably stores content-addressed blobs in a single R2 bucket, with tenant + region encoded in the object key over the S3-compatible API."
source_files:
  - "crates/corelink-container/src/storage.rs"
  - "crates/corelink-container/src/storage/r2_s3.rs"
  - "crates/corelink-region/src/region.rs"
checkpoint_sha: "325a0a2abdbc78ef7318e866d671ee974e12b6aa"
provenance: "AUTHORED"
tags: ["storage", "r2", "cas", "s3", "tenant-isolation"]
timestamp: "2026-06-29T00:00:00Z"
---

# R2 CAS bucket topology

The CAS bucket is where every content-addressed blob actually lands. The native Firecracker container
cannot use the Worker's R2 binding (that is wasm-only), so it reaches R2 through the S3-compatible API
over egress (`aws-sdk-s3` pointed at `https://<account>.r2.cloudflarestorage.com`). Unlike the
[Action Cache](/storage/r2-ac-regional.md), in the native S3 adapter CAS is **one** bucket: the residency
region and the tenant are not separate buckets but segments baked into the object key, so tenant
co-residence is structurally impossible and a region migration is a key-prefix change, not a bucket move.
(Separately, `corelink-region::Region::r2_bucket_name()` does define per-region `corelink-cas-{region}`
bucket names — `crates/corelink-region/src/region.rs:66-70` — the regional-bucket topology the native
adapter here does not itself use.) This is the durable tier
behind the [native CAS surface](/surfaces/native-cas.md) and the [CAS write flow](/flows/cas-write.md).
For a `tenant_byok_config.state='active'` tenant the blob bytes that land here are now stored
ENVELOPED (Mode A convergent OR Mode B random AES-256-GCM, Wave 3c) rather than as raw plaintext, AND
the *physical* R2 object key for such a tenant embeds the §4-hardened digest
`HMAC-SHA256(TCS, plaintext_digest)` (computed on-the-fly, never persisted) in place of the raw digest —
both layers are wired into this adapter's write/read path via `byok_cas.rs`; see
[BYOK envelope encryption at rest](/storage/byok-envelope-encryption.md). The key *scheme*
(`<region>/<tenant_prefix_16>/<digest>`), tenant isolation, and credential handling described below are
unchanged — only the `<digest>` component is hardened for an active tenant.

# Role
- The native-side durable storage adapter for R2, the complement to the Worker-only wasm R2 bindings
  (`crates/corelink-container/src/storage.rs:1-30`).
- The S3 client + tenant-scoped key deriver that every CAS handler writes through
  (`crates/corelink-container/src/storage/r2_s3.rs:1-19`).

# How it works
1. R2 is reached via the S3-compatible API over egress; this module is the native complement to the
   Worker's wasm bindings (`crates/corelink-container/src/storage.rs:1-30`).
2. All S3/D1 config is sourced from env all-or-nothing — `from_env` returns `Some` only if every
   required variable is present and non-empty (`crates/corelink-container/src/storage.rs:99-116`).
3. The S3 config is built directly from explicit static R2 credentials, deliberately bypassing the AWS
   credential-provider chain (`crates/corelink-container/src/storage/r2_s3.rs:102-135`).
4. CAS is a single bucket; the tenant and region are encoded in the object KEY
   `<region>/<tenant_prefix_16>/<digest>`, not in the bucket name
   (`crates/corelink-container/src/storage/r2_s3.rs:1-19`).
5. The client serializes measure-and-delete per object key so two racing deletes cannot both report the
   same bytes reclaimed (`crates/corelink-container/src/storage/r2_s3.rs:74-91`).
6. Bucket / region env reads route through `env_or` so an absent OR empty value falls back to the
   container default instead of producing a request-breaking empty bucket name
   (`crates/corelink-container/src/storage.rs:129-141`).
7. The production CAS handler builder wires a DURABLE audit trail, not a volatile one: with storage creds
   present (the real data plane) `build_r2_cas_handler_from_env` constructs the D1 `audit_outbox` sink via
   `cas_audit_sink_from_d1_concrete(D1HttpClient::new(&env))` and REFUSES to build the handler if that sink
   cannot be constructed — the route then mounts the fail-CLOSED 503 handler rather than falling back to
   the in-memory `InMemoryAuditSink` (which would lose every CAS audit event on restart and make the
   route's `AuditFailed → 503` guard dead code). The builder keeps the sink CONCRETE (`Arc<D1AuditOutboxSink>`,
   not just the type-erased `Arc<dyn AuditSink>`) and wires the SAME `Arc` into the handler twice — once
   coerced for the sync trait object, once via `.with_async_audit(..)` — so the sync and concurrent-list
   audit paths (point 8) are provably one sink instance
   (`crates/corelink-container/src/storage/r2_s3.rs:1932-1970`).
8. Every native CAS/AC handler's R2/S3 object GET/PUT/DELETE/LIST call — made through the sync
   `block_in_place` bridge described in point 3 — is now timed into the container's `ostore`
   Server-Timing phase rather than falling into the unattributed `oother` residue; the read path's GET is
   the representative example (`crates/corelink-container/src/storage/r2_s3.rs:1190-1194`). The `list()`
   call goes further: the mandatory `ListAttempted` audit write and this R2 enumeration now run
   CONCURRENTLY (`tokio::join!`) rather than serially, when the handler was built with the async audit
   seam wired (production shape) — fail-CLOSED is preserved because no rows are ever returned to the
   caller until the audit result is checked FIRST, in the same order the old serial code checked it; a
   handler without that seam (every test handler) keeps the original fully serial path
   (`crates/corelink-container/src/storage/r2_s3.rs:1753-1867`). See
   [the request-flow concept](/planes/request-flow.md) for the full phase list and the timing-honesty
   argument for why the joined window is attributed once, not twice.

# Invariants
- S3 credentials come only from env and are redacted in both `Debug` and `Display` — a `{:?}` must
  never leak the R2 secret key or CF token (`crates/corelink-container/src/storage.rs:67-91`).
- `StorageEnv::from_env` is all-or-nothing: a partial credential set yields `None` and the caller falls
  back to in-memory fakes rather than a half-configured client
  (`crates/corelink-container/src/storage.rs:99-116`).
- The tenant prefix segment is derived via `derive_prefix`, so cross-tenant key co-residence is
  impossible — layer 5 of `INV-TENANT-ISOLATION`
  (`crates/corelink-container/src/storage/r2_s3.rs:1-19`).
- The S3 config MUST be built from explicit static credentials; calling `aws_config::defaults` would
  trigger IMDS probes that have no endpoint in CF Containers and burn 60-90s of cold-start
  (`crates/corelink-container/src/storage/r2_s3.rs:102-135`).
- On the production data plane the CAS audit sink MUST be durable: the builder wires the D1 `audit_outbox`
  sink and fails CLOSED (refuses to mount the handler) if it cannot, never silently falling back to the
  volatile in-memory sink (`crates/corelink-container/src/storage/r2_s3.rs:1958`).
- The native `list()` concurrent seam never weakens the audit-before-response guarantee: the audit
  result is checked and can return `AuditFailed` BEFORE the store result is ever inspected, regardless
  of whether the R2 call was dispatched concurrently — no rows are served on a failed audit write
  (`crates/corelink-container/src/storage/r2_s3.rs:1869-1872`).

# Gotchas
- Empty-string env is the trap, not just absent env: the DO forwards container env as `this.env.X ?? ""`,
  so a bare `var().unwrap_or(default)` would accept `""` and yield an S3 client that fails every request
  (the 2026-06-05 AC-500 dogfood incident) — always route bucket/region reads through `env_or`.
- CAS keeps one bucket with region-in-key; AC keeps region-in-bucket-name. Do not assume the two
  surfaces share a topology.

# Citations
1. `crates/corelink-container/src/storage.rs:1-30` — module charter: native R2 access via the S3-compatible API over egress.
2. `crates/corelink-container/src/storage.rs:67-91` — credential-redacting `Debug`/`Display` impls.
3. `crates/corelink-container/src/storage.rs:99-116` — all-or-nothing `StorageEnv::from_env`.
4. `crates/corelink-container/src/storage.rs:129-141` — `env_or` (absent OR empty → default; AC-500 incident).
5. `crates/corelink-container/src/storage/r2_s3.rs:1-19` — CAS key scheme `<region>/<tenant_prefix_16>/<digest>` + tenant isolation.
6. `crates/corelink-container/src/storage/r2_s3.rs:74-91` — `R2S3Client` bucket field + per-key delete-serialization locks.
7. `crates/corelink-container/src/storage/r2_s3.rs:102-135` — direct static-credential S3 config; IMDS-bypass cold-start fix.
8. `crates/corelink-container/src/storage/r2_s3.rs:1932-1970` — `build_r2_cas_handler_from_env` wires the DURABLE D1 `audit_outbox` sink (`cas_audit_sink_from_d1_concrete`), fails CLOSED, and keeps the sink CONCRETE so `.with_async_audit(..)` (point 11) wires the SAME instance as the sync `audit` field, replacing the former volatile `InMemoryAuditSink`.
9. `crates/corelink-region/src/region.rs:66-70` — `Region::r2_bucket_name()` → per-region `corelink-cas-{region}` (the regional-bucket topology this native adapter does not use).
10. `crates/corelink-container/src/storage/r2_s3.rs:1190-1194` — `R2CasHandler::read`'s R2 GET wrapped into `Phase::Store` (`ostore`) via `PhaseScope::enter`, the same treatment every other native-plane R2 GET/PUT/HEAD/DELETE/LIST call in this file now gets.
11. `crates/corelink-container/src/storage/r2_s3.rs:1753-1867` — `R2CasHandler::list`: when `audit_async` is wired (production), the mandatory `ListAttempted` audit write and the R2 `ListObjectsV2` call run CONCURRENTLY (`tokio::join!`) under one `block_in_place`/`block_on`; the audit result is checked first (`crates/corelink-container/src/storage/r2_s3.rs:1869-1872`) so no rows are served on a failed audit write. `R2AcHandler::list` mirrors this exactly.
