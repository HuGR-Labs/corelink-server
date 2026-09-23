---
type: "StorageComponent"
title: "R2 CAS bucket topology"
description: "How the native container durably stores content-addressed blobs in a single R2 bucket, with tenant + region encoded in the object key over the S3-compatible API."
source_files:
  - "crates/corelink-container/src/storage.rs"
  - "crates/corelink-container/src/storage/cas_write_fence.rs"
  - "crates/corelink-container/src/storage/r2_s3_parts/client.rs"
  - "crates/corelink-container/src/storage/r2_s3_parts/cas_core.rs"
  - "crates/corelink-container/src/storage/r2_s3_parts/cas_ops.rs"
  - "crates/corelink-container/src/storage/r2_s3_parts/cas_batch.rs"
  - "crates/corelink-container/src/storage/r2_s3_parts/cas_builder.rs"
  - "crates/corelink-container/src/storage/r2_s3_parts/client_impl.rs"
  - "crates/corelink-container/src/storage/r2_s3_parts/client_types.rs"
  - "crates/corelink-container/src/storage/r2_s3_parts/ac_core.rs"
  - "crates/corelink-container/src/sli_aggregate.rs"
  - "crates/corelink-region/src/region.rs"
source_blobs:
  - "crates/corelink-container/src/storage.rs@a8a86b64682ebe3c66b960d4301778ad3321d771"
  - "crates/corelink-container/src/storage/cas_write_fence.rs@a267f99ce0c11cf18cb577560554b4c7347b7f3b"
  - "crates/corelink-container/src/storage/r2_s3_parts/client.rs@22766306979eea1f5180a0418f08a405acf0c25f"
  - "crates/corelink-container/src/storage/r2_s3_parts/cas_core.rs@613fdde5ed6e70e5ba2079903c013e3dc33f27fb"
  - "crates/corelink-container/src/storage/r2_s3_parts/cas_ops.rs@8762b858933aadb0c6577be5cef37d9826dec42a"
  - "crates/corelink-container/src/storage/r2_s3_parts/ac_core.rs@9d0f2ddda14e30d0358283bc9d96577a94d8ad29"
  - "crates/corelink-container/src/sli_aggregate.rs@9e830ac543a06095cff49857cb91edff6b0ce7f2"
  - "crates/corelink-region/src/region.rs@6fbe2220d22d66f6c07207800db0f2dbca21cd1b"
  - "crates/corelink-container/src/storage/r2_s3_parts/cas_batch.rs@9255013bd5f08e8ea4ee2d66f6dffc5aa466852b"
  - "crates/corelink-container/src/storage/r2_s3_parts/cas_builder.rs@46958333012cea23f7357887d91e105c5d4f50a2"
  - "crates/corelink-container/src/storage/r2_s3_parts/client_impl.rs@7e700bc86c038465a13e60d56fe2c3c7b81229a4"
  - "crates/corelink-container/src/storage/r2_s3_parts/client_types.rs@acb304cc10ef4e48fd5da1b779565ca27588c156"
checkpoint_sha: "a65c7d7caed03adf00acd3a227dc20c4e857f7f0"
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
bucket names — `crates/corelink-region/src/region.rs:83-87` — the regional-bucket topology the native
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
  (`crates/corelink-container/src/storage/r2_s3_parts/client_types.rs:130-139`).

# How it works
1. R2 is reached via the S3-compatible API over egress; this module is the native complement to the
   Worker's wasm bindings (`crates/corelink-container/src/storage.rs:1-30`).
2. All S3/D1 config is sourced from env all-or-nothing — `from_env` returns `Some` only if every
   required variable is present and non-empty (`crates/corelink-container/src/storage.rs:124-139`).
3. The S3 config is built directly from explicit static R2 credentials, deliberately bypassing the AWS
   credential-provider chain (`crates/corelink-container/src/storage/r2_s3_parts/client_impl.rs:21-34`).
4. CAS is a single bucket; the tenant and region are encoded in the object KEY
   `<region>/<tenant_prefix_16>/<digest>`, not in the bucket name
   (`crates/corelink-container/src/storage/r2_s3_parts/client_types.rs:130-139`).
5. The client serializes measure-and-delete per object key so two racing deletes cannot both report the
   same bytes reclaimed (`crates/corelink-container/src/storage/r2_s3_parts/client.rs:88-103`; delete serialization at `crates/corelink-container/src/storage/r2_s3_parts/client_impl.rs:362-410`).
6. Bucket / region env reads route through `env_or` so an absent OR empty value falls back to the
   container default instead of producing a request-breaking empty bucket name
   (`crates/corelink-container/src/storage.rs:153-165`).
7. The production CAS handler builder wires a DURABLE audit trail, not a volatile one: with storage creds
   present (the real data plane) `build_r2_cas_handler_from_env` constructs the D1 `audit_outbox` sink via
   `cas_audit_sink_from_d1_concrete(D1HttpClient::new(&env))` and REFUSES to build the handler if that sink
   cannot be constructed — the route then mounts the fail-CLOSED 503 handler rather than falling back to
   the in-memory `InMemoryAuditSink` (which would lose every CAS audit event on restart and make the
   route's `AuditFailed → 503` guard dead code). The builder keeps the sink CONCRETE (`Arc<D1AuditOutboxSink>`,
   not just the type-erased `Arc<dyn AuditSink>`) and wires the same `Arc` into the handler twice — once
   coerced for the synchronous audit trait and once via `.with_async_audit(..)` for the batched
   `exists_batch` audit path. CAS list keeps the synchronous audit-before-enumeration order
   (`crates/corelink-container/src/storage/r2_s3_parts/cas_builder.rs:10-88`).
8. Native CAS GET/PUT/DELETE/LIST calls use the synchronous `block_in_place` bridge to the async R2 client; the GET path is the representative timed storage operation (`crates/corelink-container/src/storage/r2_s3_parts/cas_ops.rs:163-169`). The CAS `list()` path commits `ListAttempted` before the tenant-prefix R2 enumeration and checks the audit result before returning any rows (`crates/corelink-container/src/storage/r2_s3_parts/ac_core.rs:337-343,385-418`).
9. The Bazel REAPI `findMissingBlobs` existence probe is the seam that removes an O(N) endpoint.
   `CasReadHandler::exists` answers about ONE digest, so probing N of them cost N audit writes and N
   HEADs — measured in prod at ~268 ms per digest, i.e. ~18 minutes at the 4096-digest cap.
   `exists_batch` collapses the audit half into ONE batched D1 statement and runs the HEADs with
   bounded concurrency (`crates/corelink-container/src/storage/r2_s3_parts/cas_batch.rs:4-124`), the bound deliberately small because the container is a
   0.25-vCPU `basic` instance and R2 request limits are shared across tenants on the account
   (`crates/corelink-container/src/storage/r2_s3_parts/client_types.rs:35-40`). Results come back in REQUEST order, not completion order, and the first error wins.
   The capability is OPTIONAL — a handler without the async audit sink advertises none
   (`crates/corelink-container/src/storage/r2_s3_parts/cas_ops.rs:367-372`) and the caller keeps the unchanged per-digest `exists()` loop. The storage half
   of a probe is ONE module-private helper that deliberately emits NO audit row (`crates/corelink-container/src/storage/r2_s3_parts/cas_batch.rs:175-210`),
   shared by this seam and the single-digest `exists()` seam so the two cannot drift on which key they
   HEAD; it is safe only because BOTH callers refuse to return any of its results unless their audit
   write committed, and it must never be widened into a generally reachable un-audited CAS probe.

# Invariants
- S3 credentials come only from env and are redacted in both `Debug` and `Display` — a `{:?}` must
  never leak the R2 secret key or CF token (`crates/corelink-container/src/storage.rs:91-114`).
- `StorageEnv::from_env` is all-or-nothing: a partial credential set yields `None` and the caller falls
  back to in-memory fakes rather than a half-configured client
  (`crates/corelink-container/src/storage.rs:124-139`).
- The tenant prefix segment is derived via `derive_prefix`, so cross-tenant key co-residence is
  impossible — layer 5 of `INV-TENANT-ISOLATION`
  (`crates/corelink-container/src/storage/r2_s3_parts/client_types.rs:130-139`).
- The S3 config MUST be built from explicit static credentials; calling `aws_config::defaults` would
  trigger IMDS probes that have no endpoint in CF Containers and burn 60-90s of cold-start
  (`crates/corelink-container/src/storage/r2_s3_parts/client_impl.rs:21-34`).
- On the production data plane the CAS audit sink MUST be durable: the builder wires the D1 `audit_outbox`
  sink and fails CLOSED (refuses to mount the handler) if it cannot, never silently falling back to the
  volatile in-memory sink (`crates/corelink-container/src/storage/r2_s3_parts/cas_builder.rs:40-88`).
- The native CAS `list()` path checks the durable `ListAttempted` audit result before returning the
  tenant-prefix R2 enumeration; an audit failure returns `AuditFailed` and no rows are served
  (`crates/corelink-container/src/storage/r2_s3_parts/ac_core.rs:337-343,415-418`).
- The `findMissingBlobs` batch seam obeys the same rule: cross-tenant denial is evaluated over the
  WHOLE request before anything is dispatched, and still emits its own `ReadDenied` row
  (`crates/corelink-container/src/storage/r2_s3_parts/cas_batch.rs:23-46`); the batched audit result is then evaluated BEFORE any probe result is read, so a
  failed audit returns `AuditFailed` and no probe outcome reaches the caller (`crates/corelink-container/src/storage/r2_s3_parts/cas_batch.rs:74-103`).
  Concurrency changes only what was dispatched, never what can reach the caller.

# Gotchas
- Empty-string env is the trap, not just absent env: the DO forwards container env as `this.env.X ?? ""`,
  so a bare `var().unwrap_or(default)` would accept `""` and yield an S3 client that fails every request
  (the 2026-06-05 AC-500 dogfood incident) — always route bucket/region reads through `env_or`.
- CAS keeps one bucket with region-in-key; AC keeps region-in-bucket-name. Do not assume the two
  surfaces share a topology.


## What the handlers report, and to whom

Every CAS and AC entry emits an availability + latency `SliObservation` pair before returning
(`crates/corelink-container/src/storage/r2_s3_parts/cas_core.rs:354-377` — `R2CasHandler::emit_sli`). Where those
observations GO was, until 2026-08-29, nowhere: the deployed builders wired
`corelink_handler_cas::InMemorySliObserver`, a capture-everything `Mutex<Vec<..>>` whose
`snapshot()` / `count()` is called only from tests, built once at process start and never drained —
so it grew for the life of the container while `AvailCasGet`, `LatencyCasGetP99`, `AvailAcLookup`
and `CorrectnessCas` were computed by nothing. Every call site also passed a literal `latency_us: 0`,
including for the p99-latency SLIs.

The deployed sink is now `CountingSliObserver`
(`crates/corelink-container/src/sli_aggregate.rs:80-136`), which keeps ONE counter set per `Sli`
variant instead of one row per observation — bounded by the closed 18-variant taxonomy regardless of
traffic — and carries `(errors, total)` in bounded 5-minute buckets, which is exactly the shape
`corelink_slo::BurnRateCalculator` consumes for the canonical 1h/6h/24h/3d range windows. It implements BOTH handler observer traits
(`crates/corelink-container/src/sli_aggregate.rs:156-165`), which is why it lives in the container
rather than in either handler crate: those stay free of a `tracing` dependency. Every 256
observations of an SLI it evaluates with the canonical `corelink_slo::BurnRateCalculator` and
publishes both the aggregate and structured burn-rate decision on the container's ordinary
`tracing` stream (`crates/corelink-container/src/sli_aggregate.rs`), and the call sites now pass
real elapsed microseconds measured from handler entry. Latency SLIs remain latency summaries and
are not misclassified as availability burn. The `LatencyAcHitP99` catalog is limited to AC lookup
round-trips; LIST contributes availability latency only because it is not a lookup hit. The consumer is fail-open for the data path while the
structured non-quiet decision is available to the operational log/metrics alert channel.

# Citations
1. `crates/corelink-container/src/storage.rs:1-30` — module charter: native R2 access via the S3-compatible API over egress.
2. `crates/corelink-container/src/storage.rs:91-114` — credential-redacting `Debug`/`Display` impls.
3. `crates/corelink-container/src/storage.rs:124-139` — all-or-nothing `StorageEnv::from_env`.
4. `crates/corelink-container/src/storage.rs:153-165` — `env_or` (absent OR empty → default; AC-500 incident).
5. `crates/corelink-container/src/storage/cas_write_fence.rs:1-80` — the D1-backed CAS writer lease that fences GC from concurrent R2 writes.
6. `crates/corelink-container/src/storage/r2_s3_parts/client_types.rs:130-139` — CAS key scheme `<region>/<tenant_prefix_16>/<digest>` + tenant isolation.
6. `crates/corelink-container/src/storage/r2_s3_parts/client.rs:88-103` — `R2S3Client` bucket field and per-key lock map; delete serialization at `crates/corelink-container/src/storage/r2_s3_parts/client_impl.rs:362-410`.
7. `crates/corelink-container/src/storage/r2_s3_parts/client_impl.rs:21-34` — direct static-credential S3 config; IMDS-bypass cold-start fix.
8. `crates/corelink-container/src/storage/r2_s3_parts/cas_builder.rs:10-88` — `build_r2_cas_handler_from_env` wires the DURABLE D1 `audit_outbox` sink (`cas_audit_sink_from_d1_concrete`), fails CLOSED, and keeps the sink CONCRETE so `.with_async_audit(..)` (point 11) wires the SAME instance as the sync `audit` field, replacing the former volatile `InMemoryAuditSink`.
9. `crates/corelink-region/src/region.rs:83-87` — `Region::r2_bucket_name()` → per-region `corelink-cas-{region}` (the regional-bucket topology this native adapter does not use).
10. `crates/corelink-container/src/storage/r2_s3_parts/cas_ops.rs:163-169` — `R2CasHandler::read`'s R2 GET wrapped into `Phase::Store` (`ostore`) via `PhaseScope::enter`, the same treatment every other native-plane R2 GET/PUT/HEAD/DELETE/LIST call in this file now gets.
11. `crates/corelink-container/src/storage/r2_s3_parts/ac_core.rs:290-418` — `R2CasHandler::list`: the mandatory `ListAttempted` audit write completes before the tenant-prefix R2 enumeration, and the audit result is checked before any rows are returned (`crates/corelink-container/src/storage/r2_s3_parts/ac_core.rs:337-343,385-418`).
12. `crates/corelink-container/src/storage/r2_s3_parts/cas_batch.rs:4-124` — `R2CasHandler::exists_batch_inner`: the `findMissingBlobs` seam — cross-tenant denial first, one batched `ReadAttempted` write completed before bounded-concurrency R2 HEADs are dispatched.
13. `crates/corelink-container/src/storage/r2_s3_parts/client_types.rs:35-40` — `MAX_CONCURRENT_EXISTS_PROBES`: the in-flight bound on those HEADs (0.25-vCPU instance; R2 limits shared across tenants).
14. `crates/corelink-container/src/storage/r2_s3_parts/cas_batch.rs:175-210` — `probe_existence_unaudited`: the ONE deliberately non-auditing probe body, shared by the single-digest and batch seams; safe only because both callers gate every result on their audit write.
15. `crates/corelink-region/src/region.rs:1` — declared source anchor.
16. `crates/corelink-container/src/storage/r2_s3_parts/cas_builder.rs:10-88` — CAS production handler builder and fail-closed durable audit/fence wiring.
17. `crates/corelink-container/src/storage/r2_s3_parts/client_impl.rs:10-44,362-410` — static credential construction and per-key delete serialization.
18. `crates/corelink-container/src/storage/r2_s3_parts/client_types.rs:35-40,130-139` — bounded existence-probe concurrency and physical CAS key layout.
19. `crates/corelink-container/src/storage/r2_s3_parts/cas_batch.rs:4-124,175-210` — fail-closed audited batch existence probes and their private storage helper.
