---
type: "SecurityControl"
title: "Data-plane attack surface"
description: "What an attacker-grade red-team of CoreLink's cache data plane and every cache surface actually found — and why the data plane held."
source_files:
  - "docs/security/2026-06-23-brutal-dataplane.md"
  - "crates/corelink-container/src/routes.rs"
  - "crates/corelink-container/src/routes/cas.rs"
  - "crates/corelink-container/src/routes/cas_erase.rs"
  - "crates/corelink-container/src/storage/r2_s3.rs"
checkpoint_sha: "c100df62c1ce7d50185f5102ce1185da0a9fe9f9"
provenance: "AUTHORED"
tags: ["security", "data-plane", "cache-poisoning", "tenant-isolation", "red-team"]
timestamp: "2026-06-26T00:00:00Z"
---

# Data-plane attack surface

CoreLink's data plane is the bytes path — native CAS/AC plus every off-native cache surface
([native CAS](/surfaces/native-cas.md), [action cache](/surfaces/action-cache.md), Bazel REAPI v2,
Turbo v8, OCI, and the cargo/npm/pip/brew adapters). It is the surface a malicious free-tenant, an
anon, or a competitor actually reaches at scale, so its integrity and isolation are the product's
load-bearing security promise. This concept captures the 2026-06-23 brutal data-plane red-team: the
verdict was **DATA PLANE HELD** — zero new or regressed exploitable findings — and, more usefully,
*why* it held, so the structural defenses are not silently regressed later.

# Role

It documents the defensive posture of the bytes path as confirmed by an attacker-grade audit: the
seams where tenant isolation, erasure, and content-integrity are enforced, and the kill-chains that
were tried and failed. It is the data-plane companion to the broader
[security posture overview](/security/posture-overview.md) and the
[pentest learnings](/security/pentest-learnings.md) that the prior fix-wave closed.

# How it works

- The audit was a read-only attacker-grade code review against `main`, with no destructive,
  charging, or DoS action run against prod; the verdict was DATA PLANE HELD, 0 new / 0 regressed
  exploitable findings (`docs/security/2026-06-23-brutal-dataplane.md:1-13`).
- The GDPR-erasure (tombstone-410) gate was verified fixed **by construction**: it moved out of
  per-route inline checks into a single shared `TombstoneGatedCasHandler` (struct at
  `crates/corelink-container/src/routes/cas_erase.rs:938`) that is wrapped around the shared
  read/write/delete trait objects at one chokepoint and cloned into every surface
  (`crates/corelink-container/src/routes.rs:457`), so a re-PUT of a tombstoned `(tenant,hash)` is refused
  by the write-handler gate (`crates/corelink-container/src/routes/cas_erase.rs:1015`) and a read is
  short-circuited by the read-handler gate (`crates/corelink-container/src/routes/cas_erase.rs:983`); the
  native route additionally answers a precise 410 before the handler at
  `crates/corelink-container/src/routes/cas.rs:674` (`docs/security/2026-06-23-brutal-dataplane.md:19-31`).
- CAS poisoning is closed at one gate: the production `R2CasHandler`
  (`crates/corelink-container/src/storage/r2_s3.rs:466`) recomputes and verifies the content hash
  before any R2 put — `verify_content_hash` runs at `crates/corelink-container/src/storage/r2_s3.rs:973`
  and a mismatch returns `HashMismatch` with nothing written
  (`crates/corelink-container/src/storage/r2_s3.rs:986`) — and the keyspace is explicitly partitioned
  (Blake3 native/sccache vs Sha256 for the Bazel keyspace) so the two never collide
  (`docs/security/2026-06-23-brutal-dataplane.md:33-43`).
- Tenant isolation at the R2 key layer is a secret-keyed ~96-bit HMAC namespace prefix that fails
  CLOSED on a non-UUID or missing key: the R2 object key is built only via `r2_key`, which derives the
  prefix through `tenant_prefix(self.tdk, tenant)?` and propagates its error
  (`crates/corelink-container/src/storage/r2_s3.rs:519`), with `_public` a reserved sentinel UUID that
  cannot collide with a real tenant prefix (`docs/security/2026-06-23-brutal-dataplane.md:45-51`).
- The edge trust boundary is comprehensive strip-then-set: the Worker structurally strips the full
  client-trust header set on every forward and re-establishes them from the PAT-resolved tenant +
  D1 scope + tier, and the container re-verifies the bearer PAT and the path-vs-auth tenant before
  storage (`docs/security/2026-06-23-brutal-dataplane.md:52-64`).
- Bazel and Turbo are tenant-scoped: Bazel checks the instance namespace equals the authenticated
  tenant on every op; Turbo demotes a validated `teamId` to an intra-tenant sub-namespace so
  cross-tenant reach is impossible (`docs/security/2026-06-23-brutal-dataplane.md:66-75`).
- The OCI plane held: digest-verified on push and content-bound on serve, tenant resolved only from
  the HMAC-signed bearer (never a header), the velocity gate wired and not fail-open on a missing
  tenant (`docs/security/2026-06-23-brutal-dataplane.md:77-85`).
- npm/pip/brew `_public` poisoning is closed: npm tarballs are stored per-tenant, `_public`
  metadata is SSRF-pinned and integrity-verified pre-store, and `MoatCache::get` re-hashes served
  bytes and self-heals a mismatch as a miss (`docs/security/2026-06-23-brutal-dataplane.md:87-99`).

# Invariants

- Erased CAS bytes do not resurrect on any surface: read 410/NotFound and re-PUT refused everywhere,
  because the gate lives in the shared `TombstoneGatedCasHandler` wrapped at the one chokepoint and
  cloned into all surfaces (`crates/corelink-container/src/routes.rs:457`,
  `crates/corelink-container/src/routes/cas_erase.rs:983`,
  `crates/corelink-container/src/routes/cas_erase.rs:1015`; `docs/security/2026-06-23-brutal-dataplane.md:103-107`).
- Served bytes always match the requested digest: cross-algo digest confusion is impossible (keyspace
  explicit + R2-key-partitioned) and the write path verifies the content hash before any PUT
  (`crates/corelink-container/src/storage/r2_s3.rs:973`; `docs/security/2026-06-23-brutal-dataplane.md:108-118`).
- Forged tenant/scope/quota headers cannot smuggle past the edge: they are stripped structurally and
  re-set from trusted sources, and the container re-verifies the PAT (`docs/security/2026-06-23-brutal-dataplane.md:115-118`).

# Gotchas

- Two documented, accepted residuals are NOT new findings: the shared `_token`/`_v2root` OCI
  rate-limit buckets mean a noisy source can 429 others' `docker login` for sub-second windows (the
  F-016 per-IP edge/WAF residual), and `_public` bytes survive a single tenant's DSR erase by design
  (public, non-personal content, out of GDPR Art.17 scope) (`docs/security/2026-06-23-brutal-dataplane.md:120-129`).
- A `grep tombstone` over the route files returning 0 is EXPECTED, not a regression — the gate moved
  one layer down into the shared `TombstoneGatedCasHandler`, so absence at the route level is the
  correct shape (`docs/security/2026-06-23-brutal-dataplane.md:19-31`).

# Citations

1. `docs/security/2026-06-23-brutal-dataplane.md:1-13` — scope and the DATA PLANE HELD verdict (0 new/0 regressed).
2. `docs/security/2026-06-23-brutal-dataplane.md:19-31` — F-004 erasure gate fixed by construction (shared handler cloned into all 7 surfaces).
3. `docs/security/2026-06-23-brutal-dataplane.md:33-43` — CAS poisoning closed at one gate; keyspace partition.
4. `docs/security/2026-06-23-brutal-dataplane.md:45-51` — secret HMAC R2 prefix, fail-closed; `_public` sentinel.
5. `docs/security/2026-06-23-brutal-dataplane.md:52-64` — edge strip-then-set + container re-verify.
6. `docs/security/2026-06-23-brutal-dataplane.md:66-75` — Bazel/Turbo tenant scoping.
7. `docs/security/2026-06-23-brutal-dataplane.md:77-85` — OCI plane held.
8. `docs/security/2026-06-23-brutal-dataplane.md:87-99` — npm/pip/brew `_public` poisoning closed + MoatCache re-verify.
9. `docs/security/2026-06-23-brutal-dataplane.md:103-118` — kill-chains attempted and refuted.
10. `docs/security/2026-06-23-brutal-dataplane.md:120-129` — the two accepted residuals (shared OCI buckets; `_public` survives DSR).
11. `crates/corelink-container/src/routes.rs:457` — the single chokepoint: shared CAS read/write/delete handlers wrapped in `TombstoneGatedCasHandler` and cloned into every surface (erasure gate by construction).
12. `crates/corelink-container/src/routes/cas_erase.rs:938` / `:983` / `:1015` — `TombstoneGatedCasHandler` struct; read gate (tombstoned read short-circuits); write gate (re-PUT of a tombstoned hash refused).
13. `crates/corelink-container/src/storage/r2_s3.rs:466` / `:973` / `:986` — `R2CasHandler`; `verify_content_hash` before any R2 PUT; `HashMismatch` returned with nothing written (poisoning gate).
14. `crates/corelink-container/src/storage/r2_s3.rs:519` — every R2 key is built via `r2_key`, which derives the HMAC tenant prefix through `tenant_prefix(self.tdk, tenant)?` and fails CLOSED on its error.
15. `crates/corelink-container/src/routes/cas.rs:674` — the native read route's precise 410-Gone tombstone gate (answers 410 before reaching the handler).
