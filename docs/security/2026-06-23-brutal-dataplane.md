# Brutal data-plane + cache-surface red-team — 2026-06-23

**Scope:** attacker-grade audit of CoreLink's DATA PLANE + every cache surface
(native CAS/AC, Bazel REAPI v2, Turbo v8, OCI, cargo/npm/pip/brew adapters) as a
malicious free-tenant / anon / competitor. Read-only code audit against `main`;
NO destructive/charging/DoS action run against prod.

**Verdict: DATA PLANE HELD.** Zero new or regressed exploitable findings. The
prior 52-round campaign (`2026-06-22-overnight-pentest-findings.md`, F-001…F-022)
plus the fix wave (container `36d6a891-r1`, worker `39a4c9d3`) closed the real
holes, and the fixes I re-verified are **correct by construction** (centralized at
a shared seam), not patched at a single call site. Below: what I verified
(regression checks) + the kill-chains I tried that failed.

---

## Regression checks on the prior fixes (all PRESENT + enforced)

### F-004 (erased-CAS resurrection across surfaces) — FIXED BY CONSTRUCTION
The tombstone 410 gate is no longer a per-route inline check. The shared
`Arc<dyn CasReadHandler/CasWriteHandler>` is wrapped in
`TombstoneGatedCasHandler` (`crates/corelink-container/src/routes/cas_erase.rs:983-1043`)
and that SAME wrapped handler is `clone()`d into every off-native surface —
cargo/brew/npm/pip/oci get `cas_read.clone()` and Bazel gets the gated read+write
(`crates/corelink-container/src/routes.rs:455-527`). Therefore:
- READ of a tombstoned `(tenant,hash)` → `NotFound` (`cas_erase.rs:988-992`), `exists` → false.
- WRITE (re-PUT) of a tombstoned `(tenant,hash)` → refused via `TOMBSTONE_GONE_SENTINEL` (`cas_erase.rs:1023-1026`).
So the grep `tombstone` over the route files returning 0 is EXPECTED — the gate
moved one layer down into the shared handler. Resurrection is closed on all 7
surfaces simultaneously. **Not bypassable** for any surface that funnels through
`MoatCache` / the shared CAS handler.

### Native + Bazel + sccache CAS poisoning (mismatched-digest) — CLOSED at one gate
Production path is `R2CasHandler`, not the in-memory fake. `R2CasHandler::write`
recomputes and verifies the content hash BEFORE any R2 put
(`crates/corelink-container/src/storage/r2_s3.rs:973-992`, `verify_content_hash`
at `:687-717`), returning `HashMismatch` and writing nothing on mismatch. The
algo is **keyspace-partitioned + explicit** (Blake3 for native/sccache, Sha256
for the `bazel/sha256/` keyspace, `:688-715`) and the R2 key embeds the algo
segment (`R2S3Client::blob_key`, `:520`) so the two keyspaces never collide and
the read re-applies the SAME function the blob was admitted under — no cross-algo
confusion. Bazel CAS write independently re-verifies sha256+size at the bridge
boundary (`routes/bazel_v2.rs:566`, `bazel-bridge/src/adapter.rs:142-145`).

### Tenant isolation at the R2 key layer — HMAC, fail-closed
Every R2 key is `derive_prefix(tdk, tenant_uuid)` — a secret-keyed ~96-bit HMAC
namespace (`storage/r2_s3.rs:518-601`). Non-UUID/missing-TDK on the production
path fails CLOSED (`derive_tenant_prefix_strict`, never a shared/empty prefix).
`_public` is a reserved sentinel UUID `0x5f5f7075626c6963…0001`
(`r2_s3.rs:571`) that can never collide with a real v4/v7 tenant prefix.

### Edge trust boundary — comprehensive strip-then-set
The Worker structurally strips the full client-trust header set
(`x-corelink-tenant-id`, `x-corelink-scope`, `x-corelink-storage-quota-bytes`,
`x-corelink-token-prefix`, `x-corelink-internal-auth`, `x-admin-*`, XFF,
`x-corelink-client-ip`, `x-corelink-primary-region`, fanout-from) on EVERY
forward (`worker/src/index.ts:427-477`) and re-establishes them from the
PAT-resolved tenant + D1 `pat.scope` + `getTierForTenant` D1 tier lookup
(`index.ts:2129-2272`). URL `:tenant` must equal the PAT-resolved tenant or 403
(`index.ts:2141-2149`). The container re-checks: `AuthTenant` rejects all
sentinels + empty + missing with 401 (`auth_tenant.rs:19-31`), every native
CAS/AC route enforces `path tenant != auth.0 → 403` BEFORE storage
(`cas.rs:636`, `cas.rs:728`, `ac.rs:496`, `ac.rs:555`) and re-verifies the bearer
PAT with Argon2id (`pat_gate_reject`). The trust anchor is unspoofable end-to-end.

### Bazel / Turbo scoping — isolated
- Bazel: `check_tenant(instance, caller_tenant)` on every CAS/AC op
  (`bazel-bridge/src/adapter.rs:111,142,193,219`); `instance` is the CAS
  namespace and MUST equal the authenticated tenant.
- Turbo: storage `tenant = AuthTenant`, key = `"<teamId>/<hash>"`; `teamId` is a
  validated label (`[A-Za-z0-9_-]`, no `/`, length-capped —
  `turbo-bridge/src/error.rs:35-61`), demoted to an intra-tenant sub-namespace.
  Cross-tenant impossible (401 fail-closed at the extractor on no auth). The
  opaque-hash "poison" is self-poison only (Turborepo protocol trusts its own
  cache), tenant-isolated.

### OCI plane (delegated deep audit) — HELD
Manifest+blob digest verified on push (`oci/push/manifest.rs:161`,
`routes/oci.rs:524-526`) and content-bound on serve via `MoatCache`'s blake3
re-verify; tenant resolved ONLY from the HMAC-signed bearer (never a header),
per-tenant storage keys (OCI never uses `_public`), upload-session cross-tenant
uuid rejected, cap rides inside the HMAC preimage (tamper → verify fail), `/token`
runs full Option-B PAT re-verify + downscopes a `cas:r` PAT to pull-only, F-016
velocity gate wired in the live path (`ratelimit_layer.rs:456-560`) and does NOT
fail-open on a missing tenant. F-002 finite `team` cap present (`oci_cap.rs`).

### npm/pip/brew `_public` poisoning — CLOSED
- npm **tarballs** are stored PER-TENANT (`routes/npm.rs:138-177`, explicit
  fail-closed isolation because scope isn't visible at the CAS port) — no
  cross-tenant tarball reach. npm metadata `_public` is SSRF-pinned to
  `registry.npmjs.org` (same-origin enforce + internal-IP block + redirect
  policy, `adapter-host/src/npm/upstream.rs`), integrity-verified fail-closed
  pre-store (SHA512 SRI preferred, `tarball.rs:265-408`).
- pip wheels: digest-addressed sha256, verified by the adapter pre-put; `MoatCache`
  read re-verifies blake3 (`adapter_cache.rs:225-236`).
- brew: F-005 repo-path allowlist + digest-verify present (`routes/brew.rs:67-75`).
- `MoatCache::get` re-hashes served bytes against the mapped `content_hash` and
  refuses to serve on mismatch (`adapter_cache.rs:225-236`) — so even a poisoned
  map row or CAS corruption can't serve mismatched bytes; it self-heals as a miss.

---

## Kill-chains attempted and refuted

- **Off-native erased-byte resurrection (re-attempt of CK-2):** the gated handler
  is shared into every adapter — re-PUT refused, read 410/NotFound everywhere. Dead.
- **npm `_public` tarball cross-tenant poison:** tarballs are per-tenant, not
  `_public`. Dead.
- **npm cache-hit-skips-integrity (`tarball.rs:326-342` returns cached bytes with
  no adapter-layer re-check):** the binding is content-addressed one layer down
  (`MoatCache` blake3 re-verify), and tarballs are per-tenant — so a hit can only
  return that tenant's own previously-verified bytes. Integrity-sound; not a poison.
- **Cross-algo digest confusion (push Sha256-keyspace, read as Blake3):** keyspace
  is explicit + R2-key-partitioned; impossible to mix within one key. Dead.
- **Forged tenant/scope/quota header smuggle:** stripped structurally on every
  forward, re-set from trusted sources, container re-verifies PAT. Dead.
- **Turbo teamId key-aliasing escape:** `/` and traversal rejected by
  `validate_team_id`; namespace dimension is the auth tenant, not the key. Dead.

## Residuals (pre-existing, documented, NOT new findings)

- Shared `_token`/`_v2root` OCI rate-limit buckets: all tenants share one global
  50-rps/200-burst bucket for `/token` + `/v2/` root; a noisy source can 429
  others' `docker login` for sub-second windows. The explicitly-accepted F-016
  residual (true per-IP limiting is edge/WAF infra, off-repo). Bounded,
  self-recovering.
- `_public` namespace bytes survive a single tenant's DSR erase (erase deletes by
  that tenant's HMAC prefix, not `_public`). By design: `_public` holds public,
  non-personal content (Homebrew/public-npm/PyPI), out of GDPR Art.17 scope.

## Summary (10 lines)

1. Data plane HELD — 0 NEW, 0 REGRESSED confirmed findings.
2. CRITICAL: 0 · HIGH: 0 · MEDIUM: 0 · LOW: 0 (new/regressed).
3. F-004 erasure-gate fix verified correct BY CONSTRUCTION (shared
   `TombstoneGatedCasHandler` cloned into all 7 surfaces — `cas_erase.rs:983`,
   `routes.rs:455-527`).
4. Native/Bazel/sccache CAS poisoning closed at one gate (`r2_s3.rs:687-717,973`).
5. Tenant isolation = secret HMAC R2 prefix, fail-closed (`r2_s3.rs:518-601`).
6. Edge strips+re-sets all trust headers; container re-verifies PAT+tenant
   (`index.ts:427-477,2141-2272`; `auth_tenant.rs:19-31`; `cas.rs:636`).
7. Bazel `check_tenant`, Turbo `teamId` validated+intra-tenant — isolated.
8. OCI plane held (digest verify push+serve, signed-bearer tenant, F-016 gate wired).
9. npm tarballs per-tenant (not `_public`); npm/pip/brew `_public` SSRF-pinned +
   integrity-verified pre-store; `MoatCache` blake3 re-verify on read.
10. Scariest thing I tried (off-native resurrection of GDPR-erased bytes, the old
    CK-2) is dead: the erasure gate is now in the shared handler, so every surface
    410s/refuses uniformly. The hardening from the prior campaign is real.
