# ADR (proposal) — Worker-native `_public` cache-HIT read path

- **Status:** PROPOSED — awaiting owner sign-off before Phase 0.
- **Date:** 2026-08-16
- **Campaign:** F3.3 (measure/deliver the jaw-drop). Follows the merged F3.2 correctness
  work (B1b revocation, B4 unowned accounting, inc3 allowlist, inc4a SSRF guard).
- **Supersedes-in-scope:** the parked note *worker-native hot-path cost re-architecture*
  (this ADR is its narrowest, highest-value first slice — `_public` **reads only**).

## Context — why

F3.2 proved cross-tenant dedup is **correct** live in prod (2026-08-16): tenant A fills a
`_public` bottle, tenant B (distinct PAT/tenant) is served the identical bytes with
`X-Cache: HIT`, one shared `adapter_cache_map` row. **But it is not the product yet — the
latency is bad.** Measured live (`scratchpad/hit_floor.sh`, 8 warm sequential HITs, colo GIG):

| condition | wall | server total | breakdown |
|---|---|---|---|
| cold first HIT | 3.4 s | 3227 ms | auth 666 / wdb 850 / origin 1711 |
| warm HIT (steady) | ~0.75–1.0 s | ~750 ms | wdb ~159 / origin ~585 (of which R2 `ostore` ~353 + `ohop` ~161) |

Timings are nested: `total = wdb + origin`; `origin = ohop + ostore + misc`. A cache HIT
whose whole value proposition is "faster + cheaper" that lands at **~750 ms warm / 3.4 s
cold** does not beat a customer pulling from their own upstream. **Not done.**

The floor is **architectural, not a cold-region fluke** (the cross-region `ohop` warms away
after request 1). Every HIT still pays:
- **`wdb ~159 ms`** — the `adapter_cache_map` read over D1-over-HTTP.
- **`origin ~585 ms`** — the round-trip **through the Rust container** (edge→DO→container
  Fetcher), of which **R2 fetch (`ostore`) ~353 ms** + hop ~161 ms.

The auth cost is already ~0 warm because the Worker's 3-tier PAT-verify cache
(L1 isolate → L2 KV `METADATA_KV` → L3 D1 replica) solved exactly this SAM-latency problem
for auth (`auth/pat-moat` OKF concept). This ADR applies the **same edge-native pattern to
the cache read itself.**

## Decision

**Serve `_public` cache HITs for the demand-fill adapter surfaces (brew, then npm/pip)
directly from the Cloudflare Worker edge — Worker→R2 native + one Worker→D1 map read —
bypassing the Durable Object and the Rust container entirely on a HIT.** A MISS (or any
edge-path uncertainty) **falls through to today's container path**, which owns the upstream
fetch + fill. The edge path is a **HIT-only fast lane**, flag-gated, dual-run verified before
it is trusted to serve.

This removes the container round-trip (`origin ~585`) from the HIT path and replaces the
D1-over-HTTP map read with a Worker-native `CONFIG_DB` read (edge-local, replica + KV-cacheable
like the PAT row). The R2 fetch stays, but as a **native `env.CAS_BUCKET.get()`** instead of
S3-over-HTTP from inside the container — fewer hops.

### Verified preconditions (all GREEN on `origin/main` @ `8ddcc026`, 2026-08-16)

Every dependency the edge read needs is **already deployed** — no new secret, no new binding:

1. **R2 CAS bucket bound to the Worker:** `CAS_BUCKET` → `corelink-cas-prod`
   (`wrangler.toml:73` base / `:391` prod). Confirmed present on the deployed `corelink-prod`
   AND `corelink-prod-sam` scripts (CF API `/settings` bindings).
2. **Map D1 bound to the Worker:** `CONFIG_DB` → `d64742ea-…` — the SAME D1 that holds
   `adapter_cache_map` (migration `migrations/d1/0058_adapter_cache_map.sql`) and
   `public_blocklist` (`0097_public_blocklist.sql`). `wrangler.toml:189`/`:494`.
3. **TDK available to the Worker request path:** `R2_TDK_HEX` **is set** as a deployed secret
   on both `corelink-prod` and `corelink-prod-sam` (verified via CF API). Today it is only
   *forwarded* into the container (`worker/src/durable_object.ts` pass-through); the edge read
   would `env.R2_TDK_HEX` it directly. No provisioning needed.
4. **Region var available:** `R2_CAS_REGION` (`"iad"` on `corelink-prod`, `"sam"` on
   `corelink-prod-sam`, etc.) — `wrangler.toml:282` and per-region `env.prod-*.vars`.
5. **BLAKE3 in the Worker already exists:** `worker/src/lib/blake3.ts` (`blake3Hex`,
   `blake3HexBytes`), spec-validated. Pure-JS, non-streaming, currently dormant.

### The read path to replicate (container truth, `origin/main`)

For `GET /brew/<tenant>/v2/homebrew/core/<f>/blobs/sha256:<digest>`:

1. **Edge (unchanged, keep):** PAT 3-tier verify + URL-tenant==PAT-tenant guard
   (`worker/src/index.ts` brew route match ~`745`, `extractAuth` ~`2617`, spoof guard ~`2686`).
2. **Map read WITH the revocation guard** (`adapter_cache.rs:85-95`): one D1 statement,
   linearized, no TOCTOU —
   ```sql
   SELECT c.content_hash FROM adapter_cache_map c
    WHERE c.namespace = ?1 AND c.url_hash = ?2
      AND NOT EXISTS (SELECT 1 FROM public_blocklist pb WHERE pb.content_hash = c.content_hash)
    LIMIT 1
   ```
   `?1 = "_public"`, `?2 = url_hash = blake3(canonical bottle path)`.
3. **R2 read:** key = `<R2_CAS_REGION>/<derive_prefix(tdk, PUBLIC_NAMESPACE_UUID)>/<content_hash>`
   (`r2_s3.rs` `blob_key` ~`472`, `tenant_prefix` `_public` branch ~`877`).
   `derive_prefix` = **HMAC-SHA256(key=tdk 32B, msg=UUID 16 big-endian bytes) → base64url-no-pad
   → first 16 chars** (`crates/tenant-path/src/prefix.rs:148-166`).
   `PUBLIC_NAMESPACE_UUID = 0x5f5f_7075_626c_6963_0000_0000_0000_0001` (`r2_s3.rs:869`).
4. **Re-hash-on-read (integrity/self-heal):** `blake3(bytes) == content_hash` or treat as a
   MISS (`adapter_cache.rs:254-265`).
5. Serve with `X-Cache: HIT`.

### Invariants the edge path MUST preserve (else it is a security regression)

- **AUTH BEFORE ACT:** namespace is the literal constant `"_public"`, NEVER the Worker's
  tenant header; the PAT verify already ran. (Cross-tenant read is *intended* here — `_public`
  is shared public registry data — but the read is still only reached post-auth.)
- **Revocation:** the `NOT EXISTS public_blocklist` filter is **not optional** — it is B1b's
  kill-switch. The edge SQL must carry it verbatim, in the SAME statement (no second query).
- **Re-hash-on-read:** a poisoned/corrupt byte set must never be served — `blake3` verify
  before returning, exactly as the container does.
- **Region correctness:** read under the worker's own `R2_CAS_REGION`. A blob filled by the
  iad container lives at `iad/…`; the sam worker reads `sam/…`. This MATCHES current behavior
  (cross-region public dedup is not automatic today either) → no regression.

## Phased plan (flag + dual-run + prove-by-use)

**Phase 0 — port + parity-test the two pure functions (no deploy).**
- `derivePublicPrefix(tdkHex, uuid)` in TS (crypto.subtle HMAC-SHA256 → base64url-no-pad →
  `[..16]`), and `brewUrlHash(path)` (canonical bottle path → `blake3Hex`).
- Hermetic parity tests against Rust-generated vectors (assert byte-identical prefix + url_hash
  for a fixed set). This is the drift firewall: if the port diverges, tests fail here, never in
  prod.

**Phase 1 — shadow (deploy, flag ON, still serve container).**
- Behind a flag, on a brew `_public` GET, compute the edge result (map+blocklist read, R2 get,
  re-hash) **in parallel** with the normal container path, **serve the container's response**,
  and log divergence (edge HIT vs container HIT? bytes/hash match?). `ctx.waitUntil` the shadow
  so it never adds latency. Goal: prove 100% parity on real traffic with zero user impact.

**Phase 2 — edge-authoritative HIT (flip the flag).**
- On a brew `_public` GET: edge read first; on HIT serve from edge (no DO/container). On MISS or
  ANY error/uncertainty → fall through to container (fill). Re-measure live with `hit_floor.sh`:
  **definition of done = warm HIT p50 < 100 ms in the caller's region, revocation still enforced,
  dual-run parity 100%** — proven by USE, not tests.

**Phase 3 — extend surfaces + big blobs.**
- npm/pip (same MoatStore shape). Then the large-OCI-layer read needs a **streaming wasm
  BLAKE3** (pure-JS `blake3.ts` is O(n) and fine for KB–few-MB brew/npm/pip bottles but not
  100 MB layers) — that pulls in the parked de-risk (import wasm as a module; workerd forbids
  runtime `WebAssembly.compile`). Big-blob edge read is **out of scope for Phase 0–2.**

**Rollback:** flag off → 100% container path = today's exact behavior. Instant, total.

## Risks

- **R1 — url_hash canonicalization drift.** The edge must compute the same `url_hash` the
  container does. *Mitigation:* Phase 0 parity vectors + Phase 1 shadow divergence logging; a
  drift shows as a shadow MISS, never a wrong serve.
- **R2 — pure-JS blake3 CPU.** Fine for brew/npm/pip (small); a hard ceiling for big OCI layers.
  *Mitigation:* size-gate the edge path; big blobs fall through to container until the wasm phase.
- **R3 — serving stale-after-revoke.** *Mitigation:* the blocklist `NOT EXISTS` is in the map
  SQL; a revoke deletes the map row + hard-deletes R2 bytes, so the edge read misses on both
  independently.
- **R4 — this is the money/hot path.** *Mitigation:* flag + shadow + gradual + live re-measure;
  rollback is one flag.

## Out of scope (explicitly)

CAS/AC/Bazel/Turbo/sccache writes; the full container retirement; big-OCI-layer edge reads;
cross-region public dedup; runners/compute (a separate axis, untouched).
