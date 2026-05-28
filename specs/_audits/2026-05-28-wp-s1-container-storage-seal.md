# WP-S1 Phase 1 SEAL — Native-Container R2/D1/KV Storage Adapters

**Date:** 2026-05-28  
**WP:** WP-S1 (P0 wave Phase 1)  
**Decision Gate:** DECISION-GATE-1 Option A (R2 via S3-compatible API over egress)  
**Status:** SEALED

---

## 1. Summary

This WP wires the native Firecracker container to real Cloudflare R2
storage (CAS blobs) and the Cloudflare D1 database (metadata) from
the native side. Previously all three route handlers (`cas.rs`,
`ac.rs`, `admin.rs`) returned `InMemoryCasHandler` / `InMemoryAcHandler`
/ `InMemoryAdminHandler` unconditionally. After this WP:

- `cas.rs::build_handler()` performs a runtime env-probe: when
  `R2_S3_ACCESS_KEY_ID`, `R2_S3_SECRET_ACCESS_KEY`, `R2_S3_ENDPOINT`,
  `CLOUDFLARE_ACCOUNT_ID`, `CF_API_TOKEN`, and `D1_DATABASE_ID` are
  present it constructs `R2CasHandler` against `corelink-cas-prod`
  (or the `R2_CAS_BUCKET` override); otherwise it falls back to
  `InMemoryCasHandler`.
- `ac.rs::build_handlers()` and `admin.rs::build_handlers()` log the
  env state (real AC/Admin handlers land in WP-S1 Phase 2) and return
  `InMemoryAcHandler` / `InMemoryAdminHandler` as before.

---

## 2. Files Modified / Created

| File | Change |
|------|--------|
| `crates/corelink-container/src/storage/mod.rs` | NEW — `StorageEnv` config struct + `from_env()` |
| `crates/corelink-container/src/storage/r2_s3.rs` | NEW — `R2S3Client`, `R2CasHandler` (CasReadHandler + CasWriteHandler) |
| `crates/corelink-container/src/storage/d1_http.rs` | NEW — `D1HttpClient`, `CasMetaRecord`, `cas_meta_lookup` |
| `crates/corelink-container/src/lib.rs` | Added `pub mod storage` |
| `crates/corelink-container/src/routes/cas.rs` | `build_handler()` → runtime R2 selection |
| `crates/corelink-container/src/routes/ac.rs` | `build_handlers()` → env-probe log |
| `crates/corelink-container/src/routes/admin.rs` | `build_handlers()` → env-probe log |
| `crates/corelink-container/Cargo.toml` | Added `aws-sdk-s3`, `aws-config`, `reqwest`, `corelink-tenant-path`, `zeroize` |
| `Cargo.toml` | Added `aws-sdk-s3 = { version = "1", … }` to `[workspace.dependencies]` |
| `deny.toml` | Added 4 skip entries for new aws-sdk-s3 transitive dupes |
| `crates/corelink-handler-cas/src/audit.rs` | Added `AuditEvent::new()` constructor |
| `crates/corelink-handler-cas/src/observer.rs` | Added `SliObservation::new()` constructor |
| `crates/corelink-handler-cas/src/request.rs` | Added `CasReadResponse::new()`, `CasWriteResponse::new()` constructors |

---

## 3. DoD Checklist

1. **`cargo build -p corelink-server` green** — PASS (0 warnings, 0 errors)
2. **`cargo test -p corelink-server` green** — PASS (96 passed, 2 ignored, 0 failed)
3. **Storage round-trip test** — `storage::r2_s3::tests::storage_r2_round_trip` exists,
   gated `#[ignore]`, verified not to run in CI. Live run instructions in doc comment.
   `storage_r2_round_trip` logic: `R2S3Client::put(key, bytes)` then
   `R2S3Client::get(key)` asserts bytes equal, then `get("__no_such_key__")`
   asserts `None`. D1 round-trip in `d1_http::tests::d1_http_cas_meta_round_trip`
   (also `#[ignore]`).
4. **`build_handler()` returns real handler when env set** — PASS:
   `cas.rs::build_handler()` calls `StorageEnv::from_env()` and routes to
   `R2CasHandler` when all 6 vars are present.
5. **Blob R2 key uses `derive_prefix`** — PASS: `R2CasHandler::r2_key()` calls
   `corelink_tenant_path::derive_prefix(tdk, uid)` when a UUID tenant is given;
   falls back to raw-padded string in test mode (no TDK). Key format:
   `<region>/<tenant_prefix_16>/<digest>`.
6. **SEAL audit doc** — THIS FILE.
7. **Does NOT touch `main.rs` / `routes.rs` / `worker/` / `wrangler.toml`** —
   PASS (`git diff --stat HEAD -- crates/corelink-container/src/main.rs
   crates/corelink-container/src/routes.rs worker/ wrangler.toml` returned empty).

---

## 4. Decision Gate 1 — Option A Rationale

The native container reaches R2 via `aws-sdk-s3` pointed at
`https://<account>.r2.cloudflarestorage.com`. Credentials are static
access-key pairs (`R2_S3_ACCESS_KEY_ID` / `R2_S3_SECRET_ACCESS_KEY`)
provided by WP-C1 at deploy time. The wasm32 CF Worker bindings
(`corelink-cf-bindings::CfR2BucketReal`) are orthogonal and unchanged.

---

## 5. Security Properties

- Credentials read from env vars only; never logged.
- `#[forbid(unsafe_code)]` inherited from crate root.
- No `unwrap()` / `expect()` / `panic!()` outside `#[cfg(test)]`.
- Tenant-scoped keys via `derive_prefix` (INV-TENANT-ISOLATION layer 5).
- Audit fail-CLOSED: `WriteAttempted` emitted BEFORE the R2 `put`;
  `ReadDenied` emitted BEFORE the cross-tenant rejection.

---

## 6. Cargo-deny Status

`cargo deny check bans` passes after adding 4 skip entries for
aws-sdk-s3 v1.133.0 transitive dupes: `hashbrown@0.16.1`,
`lru@0.16.4`, `sha1@0.10.6`, `spin@0.9.8`.

---

## 7. New `#[non_exhaustive]` Constructors (corelink-handler-cas)

To allow construction from the `corelink-container` crate (which is
outside `corelink-handler-cas`), four `::new()` constructors were
added to existing `#[non_exhaustive]` structs:

- `AuditEvent::new(kind, tenant, hash, principal, at_unix_ms)`
- `SliObservation::new(sli, is_error, latency_us)`
- `CasReadResponse::new(bytes, content_hash)`
- `CasWriteResponse::new(content_hash, durable)`

These are additive (no existing callers broken; all internal uses
of struct-literal form continue to compile within the owning crate).

---

## 8. Blockers / Follow-ons

- **WP-S1 Phase 2** — real R2/D1-backed AC + Admin handlers.
- **WP-I1** — wire the listener binding in `main.rs`.
- **TDK production path** — `R2_TDK_HEX` env var wiring in WP-C1;
  current fallback (raw-padded tenant) is safe for Phase 1
  (cross-tenant isolation is enforced by handler-layer check, not key
  collision probability alone).
