# `corelink-r2-multipart` — R2 Multipart Adapter (WI-S05-003)

R2 multipart upload adapter for the CoreLink CAS chunked-upload pipeline.

Ships:

1. **`MultipartAdapter` trait** — the integration boundary every
   consumer codes against.
2. **`InMemoryMultipartAdapter`** — host-side fake honouring every
   load-bearing semantic the production R2 binding will inherit.
3. **`AlwaysFailingMultipartAdapter`** — `Backend` error every call
   for chaos tests.

The production `aws-sdk-s3` Cloudflare R2 binding shim is **deferred**
to WI-S05-006 (alongside the conformance suite) per the charter
trait-abstraction-defer pattern. The handler that consumes this crate
depends on the trait surface only; the in-memory fake here is the
canonical reference impl that the production shim must observably
match.

## Cripto-driven invariants

1. **R2 path tenant-scoped** — every `object_key` is composed via
   `object_key::compose(...)` which structurally embeds the tenant
   prefix between the bucket family and the content digest. Client-
   supplied path components are structurally unreachable. Honours
   `INV-MULTIPART-PATH-TENANT-SCOPED` (5-Layer Defense, Layer 4).

2. **Idempotent lifecycle** — `initiate` / `upload_part` /
   `complete` / `abort` are all idempotent on retries. Honours
   `INV-MULTIPART-IDEMPOTENT`.

3. **Cross-tenant upload-id rejection** — `MultipartUpload` carries
   a bound `tenant_id`; every method takes `tenant_id` explicitly
   and rejects mismatches with `MultipartError::CrossTenantUpload`.

4. **Bounded per-tenant concurrency** — 8 permits per tenant by
   default (`bounds::DEFAULT_PER_TENANT_PARALLEL_PARTS`); tunable
   per tier in S-13.

5. **Bounded parser** — part numbers `1..=10_000` (R2 hard limit);
   per-part bytes `≤ 5 GiB`; max single multipart blob ≈ 156.25 GiB.

6. **Orphan enumeration** — `list_orphans()` is the only legal way
   for the WI-S05-006 sweeper to discover sessions older than
   `max_age` and abort them. Honours
   `INV-MULTIPART-ORPHAN-DETECTABLE`.

## Anti-scope

Not in this WI:

- Resumable upload mid-stream restart.
- Multi-region replication of multipart sessions.
- HEAD-before-initiate pre-flight validation.
- Customer-tunable part size at GA.
- Real `aws-sdk-s3` Cloudflare R2 binding shim (lands in WI-S05-006).
- D1 `multipart_sessions` schema mounting (lands in WI-S05-004).

## Test pyramid

| Tier | Files | Coverage |
|---|---|---|
| Unit (lib + module) | `src/**/tests` | 46 tests — every method, every error variant. |
| Property (10k iter PR / 100k via `PROPTEST_CASES`) | `tests/prop_r2_multipart.rs` | 7 tests — tenant isolation, cross-tenant replay, idempotency (initiate / upload_part / complete), abort safety, part ordering, list_orphans filters. |
| Chaos | `tests/chaos_r2_multipart.rs` | 6 scenarios — client disconnect, R2 backend 5xx, max parts exceeded, cross-tenant replay, concurrency limit, orphan sweeper consumes list_orphans, ETag mismatch. |
| Examples | `examples/*.rs` | 4 examples — happy path, orphan sweeper, cross-tenant replay rejection, concurrency limit. |

## Quality bar

- `#![forbid(unsafe_code)]` in `src/lib.rs` + crate-level lints
  (`unwrap_used` / `expect_used` / `panic` / `indexing_slicing` /
  `print_*` all `deny`).
- `mod_module_files = "deny"` — sub-modules use `<name>.rs` parent
  files (no `mod.rs`).
- F-001 closure preserved — every adapter instance owns its state;
  no globals.
- wasm32-clean — only `tokio::sync::Semaphore` from tokio (no
  reactor); same artifact runs in the Cloudflare Workers WASM
  bundle and the host-side test harness.

## License

Internal HuGR Labs / CoreLink — UNLICENSED.
