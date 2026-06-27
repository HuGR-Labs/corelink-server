---
type: "StorageComponent"
title: "R2 CAS bucket topology"
description: "How the native container durably stores content-addressed blobs in a single R2 bucket, with tenant + region encoded in the object key over the S3-compatible API."
source_files:
  - "crates/corelink-container/src/storage.rs"
  - "crates/corelink-container/src/storage/r2_s3.rs"
  - "crates/corelink-region/src/region.rs"
  - "specs/03_architecture/adrs/ADR-S14-009-cas-residency-single-bucket-launch-posture.md"
checkpoint_sha: "5571b910292cbe3d53cbf46d7e0f120dbef877e2"
provenance: "AUTHORED"
tags: ["storage", "r2", "cas", "s3", "tenant-isolation"]
timestamp: "2026-06-26T00:00:00Z"
---

# R2 CAS bucket topology

The CAS bucket is where every content-addressed blob actually lands. The native Firecracker container
cannot use the Worker's R2 binding (that is wasm-only), so it reaches R2 through the S3-compatible API
over egress (`aws-sdk-s3` pointed at `https://<account>.r2.cloudflarestorage.com`). Unlike the
[Action Cache](/storage/r2-ac-regional.md), in the native S3 adapter CAS is **one** bucket: the residency
region and the tenant are not separate buckets but segments baked into the object key, so *private*
cross-tenant co-residence is structurally impossible (one deliberate exception — the shared `_public`
dedup namespace — is carved out below) — while the region is carried only as a **LOGICAL key-prefix
tag, NOT physical residency**: changing it relabels keys *inside the same physical bucket* and does
not relocate any bytes, so a key-prefix "region migration" is a false-confidence trap and must NOT be
mistaken for a residency fix (`specs/03_architecture/adrs/ADR-S14-009-cas-residency-single-bucket-launch-posture.md:40-51`).
(Separately, `corelink-region::Region::r2_bucket_name()` does define per-region `corelink-cas-{region}`
bucket names — `crates/corelink-region/src/region.rs:66-70` — the regional-bucket topology the native
adapter here does not itself use.) This is the durable tier
behind the [native CAS surface](/surfaces/native-cas.md) and the [CAS write flow](/flows/cas-write.md).

# Role
- The native-side durable storage adapter for R2, the complement to the Worker-only wasm R2 bindings
  (`crates/corelink-container/src/storage.rs:1-34`).
- The S3 client + tenant-scoped key deriver that every CAS handler writes through
  (`crates/corelink-container/src/storage/r2_s3.rs:1-19`).

# How it works
1. R2 is reached via the S3-compatible API over egress; this module is the native complement to the
   Worker's wasm bindings (`crates/corelink-container/src/storage.rs:1-34`).
2. All S3/D1 config is sourced from env all-or-nothing — `from_env` returns `Some` only if every
   required variable is present and non-empty (`crates/corelink-container/src/storage.rs:98-113`).
3. The S3 config is built directly from explicit static R2 credentials, deliberately bypassing the AWS
   credential-provider chain (`crates/corelink-container/src/storage/r2_s3.rs:88-116`).
4. CAS is a single bucket; the tenant and region are encoded in the object KEY, not in the bucket name
   (`crates/corelink-container/src/storage/r2_s3.rs:1-19`). There are TWO key shapes, selected by digest
   algorithm in `blob_key`: BLAKE3 → `<region>/<tenant_prefix_16>/<digest>`; SHA-256 (the Bazel REAPI
   surface) → `<region>/<tenant_prefix_16>/bazel/sha256/<digest>`
   (`crates/corelink-container/src/storage/r2_s3.rs:453-458`). Both carry the same tenant-prefix segment,
   so the isolation guarantee is identical across the two.
4b. **The ONE deliberate cross-tenant carve-out: the `_public` shared-dedup namespace.** A write whose
   tenant is the exact reserved string `_public` (`adapter_cache::PUBLIC_NAMESPACE`, used by
   public-package dedup for npm/brew/pip) derives its prefix from a FIXED reserved sentinel UUID
   (`PUBLIC_NAMESPACE_UUID`) under the same secret TDK, so every caller storing the same public blob dedups
   to ONE shared `_public` R2 key that ALL tenants read/write — the network-effect moat
   (`crates/corelink-container/src/storage/r2_s3.rs:571`, `:579-581`, full carve-out `:559-601`). It is
   reserved so it can never collide with a real (random v4/v7) tenant's HMAC prefix, and digest-verified
   bytes are content-addressed, but it IS genuine cross-tenant co-residence by design — so "co-residence is
   structurally impossible" holds for PRIVATE content only (mirror of `tenancy/isolation`'s `_public` gotcha).
5. The client serializes measure-and-delete per object key so two racing deletes cannot both report the
   same bytes reclaimed (`crates/corelink-container/src/storage/r2_s3.rs:61-77`).
6. Bucket / region env reads route through `env_or` so an absent OR empty value falls back to the
   container default instead of producing a request-breaking empty bucket name
   (`crates/corelink-container/src/storage.rs:127-139`).

# Invariants
- S3 credentials come only from env and are redacted in both `Debug` and `Display` — a `{:?}` must
  never leak the R2 secret key or CF token (`crates/corelink-container/src/storage.rs:65-89`).
- `StorageEnv::from_env` is all-or-nothing: a partial credential set yields `None` and the caller falls
  back to in-memory fakes rather than a half-configured client
  (`crates/corelink-container/src/storage.rs:98-113`).
- The tenant prefix segment is derived via `derive_prefix`, so cross-tenant key co-residence is
  impossible for PRIVATE content — layer 5 of `INV-TENANT-ISOLATION`
  (`crates/corelink-container/src/storage/r2_s3.rs:1-19`). The lone exception is the reserved `_public`
  dedup namespace, which is intentionally shared cross-tenant via a fixed sentinel-UUID prefix
  (`crates/corelink-container/src/storage/r2_s3.rs:571`, `:579-581`).
- The S3 config MUST be built from explicit static credentials; calling `aws_config::defaults` would
  trigger IMDS probes that have no endpoint in CF Containers and burn 60-90s of cold-start
  (`crates/corelink-container/src/storage/r2_s3.rs:88-116`).

# Gotchas
- Empty-string env is the trap, not just absent env: the DO forwards container env as `this.env.X ?? ""`,
  so a bare `var().unwrap_or(default)` would accept `""` and yield an S3 client that fails every request
  (the 2026-06-05 AC-500 dogfood incident) — always route bucket/region reads through `env_or`.
- CAS keeps one bucket with region-in-key; AC keeps region-in-bucket-name. Do not assume the two
  surfaces share a topology.

# Citations
1. `crates/corelink-container/src/storage.rs:1-34` — module charter: native R2 access via the S3-compatible API over egress.
2. `crates/corelink-container/src/storage.rs:65-89` — credential-redacting `Debug`/`Display` impls.
3. `crates/corelink-container/src/storage.rs:98-113` — all-or-nothing `StorageEnv::from_env`.
4. `crates/corelink-container/src/storage.rs:127-139` — `env_or` (absent OR empty → default; AC-500 incident).
5. `crates/corelink-container/src/storage/r2_s3.rs:1-19` — CAS key scheme `<region>/<tenant_prefix_16>/<digest>` + tenant isolation.
5b. `crates/corelink-container/src/storage/r2_s3.rs:453-458` — `blob_key`: the two key shapes (BLAKE3 plain vs SHA-256 `…/bazel/sha256/<digest>`).
5c. `crates/corelink-container/src/storage/r2_s3.rs:559-601` — the `_public` shared-dedup carve-out (`PUBLIC_NAMESPACE_UUID` sentinel at `:571`; the `tenant == _public` shared-prefix arm at `:579-581`).
6. `crates/corelink-container/src/storage/r2_s3.rs:61-77` — `R2S3Client` bucket field + per-key delete-serialization locks.
7. `crates/corelink-container/src/storage/r2_s3.rs:88-116` — direct static-credential S3 config; IMDS-bypass cold-start fix.
8. `crates/corelink-region/src/region.rs:66-70` — `Region::r2_bucket_name()` → per-region `corelink-cas-{region}` (the regional-bucket topology this native adapter does not use).
