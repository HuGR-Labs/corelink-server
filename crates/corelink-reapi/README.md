# corelink-reapi

CoreLink REAPI v2 handlers (WI-S01-005). Tonic-based gRPC services that
orchestrate the S-01 CAS write path:

```
gRPC inbound → PatValidator → BLAKE3 verify (corelink-hash)
                              ↓
                     R2 PUT If-None-Match (corelink-worker)
                              ↓
                  D1 batch INSERT blob_meta + audit_outbox
                              (corelink-meta)
                              ↓
                    gRPC per-blob status → client
```

## Surface

| Service | RPC | Status |
|---|---|---|
| `ContentAddressableStorage` | `BatchUpdateBlobs` | ✅ S-01 |
| `ContentAddressableStorage` | `BatchReadBlobs` | 🚧 S-02 (`unimplemented` stub) |
| `Capabilities` | `GetCapabilities` | ✅ S-01 |
| `ByteStream` | `Write` | ✅ S-01 (≤ 5 MiB single blob) |
| `ByteStream` | `Read` | 🚧 S-02 |
| `ByteStream` | `QueryWriteStatus` | 🚧 S-02 |

## Canonical wire-level constants (WI-S01-005 §6.1.3)

- `MaxBatchTotalSizeBytes` = `4 * 1024 * 1024` (4 MiB)
- `max_cas_blob_size_bytes` = `5 * 1024 * 1024` (5 MiB)
- `digest_functions` = `[BLAKE3]` (S-01: BLAKE3-only — see
  `src/capabilities.rs` rustdoc; multi-algo support lands in S-12)
- `low_api_version = high_api_version = 2.12.0`
  (`bazelbuild/remote-apis @ v2.12.0`)

## Idempotent retry contract

Every per-blob `BatchUpdateBlobs` row gets a derived
`audit_outbox.request_id` of shape:

```
<client_request_id>:<tenant_uuid>:<digest_canonical_text>
```

That makes:

1. Multiple blobs in a single batch produce distinct dedup keys (a single
   `x-request-id` is enough — no per-blob client cooperation needed).
2. Two unrelated tenants reusing the same `x-request-id` cannot collide
   on the global `UNIQUE (request_id, event_type)` constraint
   (`audit_outbox` schema in `migrations/d1/0001_blob_meta.sql`).
3. Same client retrying the same `(client_request_id, body)` produces the
   same dedup key → meta layer's idempotency surfaces as
   `InsertOutcome::AlreadyExists`, no double-emit downstream.

The CloudEvents 1.0 envelope `data.request_id` field carries the **raw
client correlation id** for SIEM linking; only the audit-row PK is
derived.

## CloudEvents 1.0 envelope

Persisted to `audit_outbox.payload_json`:

```json
{
  "specversion": "1.0",
  "id": "<UUIDv7>",
  "source": "corelink://tenant/<tenant_uuid>",
  "type": "corelink.cas.put_completed",
  "subject": "blake3:<hex>",
  "datacontenttype": "application/json",
  "data": {
    "tenant_id": "<tenant_uuid>",
    "principal_id": "<pat_owner_uuid>",
    "digest": "blake3:<hex>",
    "size_bytes": 12345,
    "region": "wnam",
    "request_id": "<client_request_id>"
  }
}
```

The CE `time` attribute is **deliberately omitted** so that the row's
`payload_json` is content-stable across legitimate retries (different
wall-clock, same logical event). Ingest timestamp lives in
`audit_outbox.enqueued_at` (separate column).

## Feature gating

- `host-server` (default-on): pulls tonic + tokio + prost; provides
  `CasWriteService`, `CapabilitiesService`, `ByteStreamWriteService`.
- No-default-features build is `wasm32-unknown-unknown`-clean — the pure-
  logic modules (`audit`, `capabilities`, `error_map`, `orchestrator`,
  `pat`) compile to wasm32 so a future `tonic-web` /
  `worker::Router` Cloudflare Workers adapter reuses the same
  orchestration logic without rework.

## Out-of-scope (deferred)

- Real Cloudflare R2 binding adapter (the test harness uses
  `InMemoryR2`); the production R2 binding shim lands together with the
  miniflare integration tier in the WI-S01-005 follow-up.
- `bazelbuild/remote-apis` REAPI conformance test suite executed against
  the host-server harness (the canonical vectors + 10 e2e gRPC scenarios
  in `tests/handler_e2e.rs` cover the documented Gherkin AC matrix).
- `R2Backend::delete` extension and the production reconciler's R2
  delete call (`R2DeleteReconciler` is wired but currently logs the
  orphan + defers to GC sweep S-06; the trait extension lands as a
  small follow-up WI).
- `tonic-web` Cloudflare-Workers deploy plumbing (S-01 is host-server +
  in-memory backends; real CF deploy follows in the post-S-01 lane).

## Quality bar

- Lints: `forbid(unsafe_code)` + clippy `deny` for unwrap/expect/panic/
  indexing/dbg/print + crate-level `result_large_err` allow (tonic
  `Status` is canonical, sized by the upstream library).
- Test surface: 33 lib unit + 6 canonical-vector integration + 10 gRPC
  e2e + 4 property tests = **54 tests** verde (debug + release).
- `cargo clippy --workspace --all-targets -- -D warnings` clean.
- `cargo check -p corelink-reapi --no-default-features --target
  wasm32-unknown-unknown` clean.

## Cross-references

- `specs/04_sprints/_sealed/S01/work_items/WI-S01-005-reapi-batchupdateblobs.md`
- `specs/03_architecture/auth_stub_contract.md` (S-02↔S-03 bridge)
- `specs/03_architecture/error_taxonomy.md` (v0.2.0+ for the 4 new
  CAS codes registered by this WI)
- `specs/03_architecture/data_model.md §4.2` (audit_outbox schema)
- ADR-0027 — Dual-write reconciliation contract.
