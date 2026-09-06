---
type: "RequestFlow"
title: "CAS at-rest integrity scrub"
description: "The cron-driven sweep that re-hashes stored CAS objects, covering the cold set the traffic-driven read-path re-verify never reaches; per-tenant enumeration, whole-tenant BYOK skip, and three counters that keep an empty sweep distinguishable from a healthy one."
source_files:
  - "crates/corelink-container/src/routes/cas_scrub.rs"
  - "crates/corelink-container/src/storage/r2_s3_parts/cas_core.rs"
  - "crates/corelink-container/src/storage/byok_cas/part-01.rs"
source_blobs:
  - "crates/corelink-container/src/routes/cas_scrub.rs@4788ef118b3e60bab81bb348169e388779507198"
  - "crates/corelink-container/src/storage/r2_s3_parts/cas_core.rs@c731944a56c1db190287a93fb8888b09ca67902c"
  - "crates/corelink-container/src/storage/byok_cas/part-01.rs@0ff91d96f9e7dffe47faed4808c468ce1b196663"
checkpoint_sha: "a65c7d7caed03adf00acd3a227dc20c4e857f7f0"
provenance: "AUTHORED"
tags: ["flows", "cas", "integrity", "scrubber", "storage", "byok", "request-flow"]
timestamp: "2026-08-26T00:00:00Z"

---
# CAS at-rest integrity scrub

`POST /_internal/cas/scrub` re-hashes stored CAS objects on a cron. It exists because the only integrity check CoreLink had is the read-path re-verify in `R2CasHandler::read`, which is real but is a sampling function driven by traffic: an object is verified exactly when a client asks for it, so a cold object was never verified at all. The scrubber covers the cold set, and ADR-S34-001 makes it a hard prerequisite for streaming CAS reads — streaming does not weaken the read-path check, it removes it.

# Context

The sweep is internal-only and gated by the same dedicated erase key the archive and erase routes use, checked in constant time (`crates/corelink-container/src/routes/cas_scrub.rs:198`). Any missing collaborator — the key, `R2_TDK_HEX`, D1 or R2 — leaves the route UNMOUNTED rather than mounted-and-broken, the same fail-closed posture as `audit_archive`. No new env var is introduced, so the secrets matrix is unchanged.

Enumeration is bounded per call: one call verifies up to an object budget and returns a cursor carrying both the tenant offset and the in-tenant position, because a sweep that runs out of budget mid-tenant must resume INSIDE that tenant or it would re-examine the same head forever and never reach the tail (`crates/corelink-container/src/routes/cas_scrub.rs:150`).

# Decision

**Enumerate R2, never `blob_meta`.** `blob_meta` is empty in production and no code inserts into it, so a scrubber keyed on it enumerates zero objects, verifies nothing, and reports success. Enumeration goes through `R2S3Client::list_objects_page`.

**Walk tenants, and walk them from `tenant`.** An R2 CAS key is `<region>/<tenant_prefix>/<digest>` where `tenant_prefix` is a secret-keyed HMAC of the tenant UUID, so a key cannot be mapped back to a tenant and the bucket cannot be swept blind. That makes the tenant list load-bearing: `tenant` holds 262 rows against `tenant_storage_state`'s 74, because the latter only carries tenants that have accrued byte accounting (`crates/corelink-container/src/routes/cas_scrub.rs:384`). The prefix derivation mirrors the eraser's, including `_public` through the reserved sentinel prefix and a fail-CLOSED refusal for a non-UUID tenant, since a truncate-and-pad fallback would collapse tenants into one shared keyspace (`crates/corelink-container/src/routes/cas_scrub.rs:314`).

**Classify BYOK once per tenant and skip an encrypting tenant WHOLE.** For a BYOK-`active` tenant the stored object is ciphertext, and re-hashing raw bytes would emit false violations against intact data. The question is asked through the public `ByokConfigCache::get` + `engagement_for` pair — the single source of truth the read and write paths already share — and NOT through `resolve_byok`, which is private to `impl R2CasHandler`, absent from `R2S3Client`, and maps a logical digest to a physical one, the opposite of the direction a sweep travels. `FailClosed` or a config-read error counts `failed`; nothing is ever assumed plaintext (`crates/corelink-container/src/routes/cas_scrub.rs:343`).

**Reuse the enforcement point, and let the key choose the algorithm.** `verify_content_hash` (`crates/corelink-container/src/storage/r2_s3_parts/cas_core.rs:615`) is widened to `pub(crate)` and called rather than reimplemented, so it remains the single enforcement point for content-addressing on the durable path. The key's own sub-prefix decides the digest function, so a REAPI v2 SHA-256 blob is not re-hashed with BLAKE3 and reported as a violation against intact data (`crates/corelink-container/src/routes/cas_scrub.rs:415`).

# Consequences

`examined`, `skipped_encrypted` and `failed` travel as three distinct numbers (`crates/corelink-container/src/routes/cas_scrub.rs:167`). This is the load-bearing property, not a reporting nicety: folding skips into examined would report full coverage over a store that was never checked, which is the same silent-success failure the `blob_meta` rule exists to prevent, arriving through a different door. A sweep reporting any `failed` returns 5xx so the cron surfaces it rather than logging a 200 with a bad number inside.

Verification moves from "on every read" to "whenever the scrubber last reached this key" for anything streaming eventually covers, and a BYOK-encrypting tenant gets no at-rest coverage at all until decrypting inside the sweep is decided on its own threat model. The scrubber never writes: it deletes, rewrites and repairs nothing, and a test re-reads the module's own source to prove no mutating storage call appears in it.

# Citations

1. `crates/corelink-container/src/routes/cas_scrub.rs:198` — fail-CLOSED construction: dedicated erase key, TDK, D1 and R2, or the route is not mounted.
2. `crates/corelink-container/src/routes/cas_scrub.rs:150` — the resume cursor carries the in-tenant position, not just the tenant offset.
3. `crates/corelink-container/src/routes/cas_scrub.rs:384` — tenants come from `tenant`, not `tenant_storage_state` (262 rows vs 74 in prod).
4. `crates/corelink-container/src/routes/cas_scrub.rs:314` — prefix derivation mirrors the eraser: `_public` sentinel first, non-UUID tenant fails closed.
5. `crates/corelink-container/src/routes/cas_scrub.rs:343` — per-tenant BYOK classification through the public config API; unclassifiable counts `failed`, never plaintext.
6. `crates/corelink-container/src/routes/cas_scrub.rs:415` — the key's sub-prefix selects BLAKE3 vs SHA-256, so a REAPI blob is not mis-hashed.
7. `crates/corelink-container/src/routes/cas_scrub.rs:167` — the three independent counters.
8. `crates/corelink-container/src/storage/r2_s3_parts/cas_core.rs:572` — `verify_content_hash`, widened to `pub(crate)` and reused as the single enforcement point.
9. `crates/corelink-container/src/storage/byok_cas/part-01.rs:447` — `engagement_for`, the shared read/write engagement decision the scrubber classifies through.
