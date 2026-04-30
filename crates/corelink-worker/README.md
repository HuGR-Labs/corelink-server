# corelink-worker

Storage adapters for the CoreLink CAS hot path.

This crate hosts the **R2 single-blob adapter** (WI-S01-003) — `R2Writer` /
`R2Reader` over a tiny `R2Backend` trait, with strict tenant-path /
idempotency / metrics / error-taxonomy enforcement. The real Cloudflare
binding shim (`worker::R2Bucket`) lands as a thin wrapper in WI-S01-005
alongside the REAPI handler, where miniflare/wrangler-dev integration tests
are available.

## Load-bearing invariants

- **INV-TENANT-ISOLATION** (CRITICAL, layer 5): every R2 key is prefixed by
  `HMAC16 = b64url(HMAC_SHA256(TDK, tenant_id))[..16]`. The constructor
  `TenantCtx::new(&tdk, tenant_id, region)` derives the prefix internally —
  callers cannot forge it.
- **INV-CAS-IMMUTABILITY**: every PUT carries `If-None-Match: *`; the
  backend rejects overwrites with `BackendPutOutcome::AlreadyExists`,
  surfaced as `PutOutcome::Duplicate`.
- **INV-CAS-IDEMPOTENCY**: a `Duplicate` outcome maps to `Ok(())` on the
  `BlobStoreWrite` trait surface — the body is durably stored either way.
- **INV-DATA-RESIDENCY**: writers and readers are pinned to one region;
  `ctx.region() != self.region` returns `R2Error::RegionMismatch`
  (`COR_INTERNAL`).
- **INV-CAS-INTEGRITY**: writes accept only `&VerifiedBody`; the type
  system enforces the BLAKE3 verify before any storage operation.

## Quickstart

```rust
use bytes::Bytes;
use corelink_hash::{Digest, VerifiedBody};
use corelink_tenant_path::TenantDerivationKey;
use corelink_worker::{Region, TenantCtx};
use corelink_worker::storage::r2::{InMemoryR2, R2Reader, R2Writer};
use std::sync::Arc;
use uuid::Uuid;
use zeroize::Zeroizing;

# async fn ex() -> Result<(), Box<dyn std::error::Error>> {
let tdk = TenantDerivationKey::from_bytes(Zeroizing::new([0u8; 32]));
let backend = Arc::new(InMemoryR2::new());
let writer = R2Writer::new(Region::Wnam, Arc::clone(&backend));
let reader = R2Reader::new(Region::Wnam, backend);

let tenant_id = Uuid::parse_str("01938af0-abcd-7123-8456-000000000001")?;
let ctx = TenantCtx::new(&tdk, tenant_id, Region::Wnam);

let body = Bytes::from_static(b"hello world");
let claimed = Digest::from_hex(
    "d74981efa70a0c880b8d8c1985d075dbcbf679b99a5f9914e5aaf96b831a9e24",
)?;
let vb = VerifiedBody::new(body.clone(), claimed)?;

writer.put(&ctx, &vb).await?;
let got = reader.get(&ctx, &claimed).await?;
assert_eq!(got, body);
# Ok(()) }
```

## Test plan

- **Canonical key vectors** (`tests/canonical_keys.rs`): hardcoded R2 keys
  cross-validated against `corelink-tenant-path` canonical vectors.
- **Property tests 100k iter** (`tests/prop_r2_path.rs`): cross-tenant
  disjointness driven through the production `R2Writer::put`, region
  segregation, path determinism.
- **Integration / chaos tests** (`tests/integration_r2.rs`): boundary 5
  MiB, oversize rejection, `BlobStoreWrite` seam, cross-tenant 404 oracle
  closure, region-mismatch enforcement, metrics observer wiring,
  PutSizeBucket boundary points, concurrent same-tenant + cross-tenant
  write races (chaos #4).
- **Fuzz harnesses** (`fuzz/fuzz_targets/`): full path-construction +
  PUT-then-GET round-trip with cross-tenant oracle assertion + size-cap
  rejection path.

## Coverage map

WI-S01-003 §10 completeness criteria:

| AC | Coverage |
|---|---|
| 10.3.1 100k iter cross-tenant disjointness | `prop_cross_tenant_keys_distinct_100k` |
| 10.3.4 idempotent duplicate PUT | `concurrent_same_tenant_duplicate_writes_idempotent` + `chaos_4_concurrent_writes_across_distinct_tenants` |
| 10.3.6 per-region binding | `per_region_binding_isolates_buckets` + `region_mismatch_writer_rejects_with_internal` + `region_mismatch_reader_rejects_with_internal` |

## CI

`.github/workflows/corelink-worker.yml` — PR lane (clippy + tests + release
+ wasm-build + 60s fuzz smoke + canonical key vectors) + nightly lane
(1h fuzz per target + cargo-mutants real-test kill rate ≥ 80% + cargo-audit).
The mutants job runs `cargo mutants` (no `--check`) so it actually executes
the test suite against each generated mutant. Local baseline at SEAL time:
**76 viable mutants, 72 caught, 4 missed → 94.7% kill rate**.

## Anti-patterns (do NOT)

- Construct R2 keys from `tenant_id` plaintext anywhere outside this
  crate. Always go through `TenantCtx::new(&tdk, tenant_id, region)` +
  `R2Writer::put`.
- Add a "put_unverified" shortcut. The `&VerifiedBody` requirement is the
  load-bearing seam.
- Bypass `If-None-Match: *` with a side-door PUT. CAS immutability is
  enforced through the trait contract.
