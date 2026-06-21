# Finding: cargo/sccache surface is broken in prod (502 HashMismatch)

> **✅ RESOLVED 2026-06-21** — fixed in PR #434 (route cargo through the 2-level `MoatCache`,
> per-tenant namespace), pinned + deployed to all 5 prod envs as image `d443af5f-r1`, and
> **verified LIVE**: `PUT /cargo/<t>/<key>` → 200 + `GET` round-trip MATCH (the 502 is gone).
> Original diagnosis preserved below for the record.

> **Found:** 2026-06-21, e2e gated-surface black-box validation (the "#2" sweep after the runner key-split).
> **Severity:** surface-broken, but **NOT a launch-blocker** — sccache/cargo is the
> **expansion campaign #1 (CI/build-acceleration, phase 3, post-launch)** surface, not the launch
> cache+governance product. Track for the CI expansion, not the go-live gate.

## Symptom
`PUT /cargo/<tenant>/<sccache-key>` (valid 64-char hex key, real PAT, cas:rw) → **HTTP 502**.
GET returns empty. Every other cache surface validated live the same day works:
native CAS/AC ✅, brew ✅ (after the 5-cause fix), npm ✅, pip ✅, turbo ✅, **bazel REAPI v2 CAS+AC ✅**
(`PUT /bazel/v2/<t>/uploads/<uuid>/blobs/<sha256>/<size>` → 204, round-trip match), OCI registry ✅
(up; `/v2/` → 401 + correct `www-authenticate` realm; full docker-push gated on a real client).

## Root cause (definitive)
The CoreLink CAS write **verifies content integrity**: it recomputes the digest of the bytes under the
keyspace's canonical hash and rejects a mismatch —
`crates/corelink-handler-cas/src/handler.rs:417` (`if actual_hash != req.claimed_hash → HashMismatch`,
"write_hash_mismatch_aborts_before_storing") and `storage/r2_s3.rs:658-675`.

- **bazel** passes `sha256(content)` as the digest → matches → stores. ✅
- **cargo** passes the **sccache key** as the digest (`cargo/bridge.rs:92` → `CasWriteRequest::new(tenant, digest_hex=key, …)`).
  The sccache key is `blake3(rustc-cmdline + input-fingerprints)` (`cargo/translate.rs:4`) — a hash of the
  COMPILE INPUTS, **not** of the cached OUTPUT bytes. So `actual=blake3(content) != claimed=key` → HashMismatch → 502.

cargo/sccache is fundamentally a **key→value** cache (opaque key, arbitrary value); it was wired directly
onto a **content-addressed, integrity-verifying** CAS as if the key were the content hash. It never worked
against the verifying CAS.

## Fix (the SOTA way — mirror brew's 2-level cache, per-tenant)
Route the cargo adapter through `MoatCache` (the existing 2-level `(namespace, key) → content_hash → blob`
cache) exactly like `routes/brew.rs::BrewMoatStore`, but with **namespace = the tenant id** (cargo cache is
PRIVATE per-tenant, not the shared `_public`):
- `put(tenant, key, bytes)` → `moat.put(tenant, key, bytes, quota)`: compute `content_hash=blake3(bytes)`,
  write the blob content-addressed (verify now passes — claimed==actual), upsert `(tenant, key)→content_hash`
  in the url-map. Gains dedup for free (identical outputs collapse).
- `get(tenant, key)` → `moat.get(tenant, key)`: resolve `(tenant,key)→content_hash`, fetch the blob.
- New `CargoMoatStore` in `routes/cargo.rs`; build a `MoatCache` (cas_read/cas_write + D1 `UrlMapStore` +
  blake3 hasher + principal), same as brew.

### Byte-accounting caveat (same gotcha that bit brew — see [[brew-public-namespace-storage-chain]])
`MoatCache.put` threads `storage_quota_bytes`. A tenant that has used native CAS already has a
`tenant_storage_state` row (seeded with its tier cap) → cargo writes accrue against it. A tenant that ONLY
uses cargo (no native write yet, no row) → `AccrueOutcome::Indeterminate` → fail-CLOSED. Either pass the
resolved tier cap from the Worker into the cargo `MoatCache.put` (preferred), or accept "cargo works after
the first native write." Decide at implementation.

## Effort
Container-plane Rust change (`routes/cargo.rs` + the cargo adapter's `CasStore` wiring) → rebuild
(~8 min on the Mac) → push 5 registries → pin → deploy 5 → verify round-trip live. A brew-sized cycle.
The diagnostic header pattern (see [[brew-public-namespace-storage-chain]]) applies if it surfaces a 2nd cause.
