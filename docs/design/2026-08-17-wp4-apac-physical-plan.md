# WP4 plan — physical APAC R2 + wire nrt + provision the `apac` macro

Status: PLAN (self-reviewed; ready to execute under standing prod authorization)
Date: 2026-08-17
Campaign: multi-region closure (`[[multi-region-reality-and-closure-campaign]]`). Follows WP-A/WP-C/WP-B (all
LIVE) + WP3 (edge-serve + async-meter live on all 5 regions).

## Goal

Make **Tokyo (nrt / `apac` macro) genuinely local**: physical APAC-located R2 CAS bucket + provision the `apac`
macro so tenants can pin there. Combined with the already-live WP-A (colo blob cache), WP-C (cached map read via
D1 replica), and WP-B (async metering — the cross-region metering WRITE is already off the hot path), an APAC
tenant's warm `_public` cache HIT then serves fully in-region: colo Cache API → APAC R2 origin, D1 APAC read
replica for the map, async metering. **No synchronous cross-Pacific hop on a warm HIT.**

SAM stays the documented platform limit (no CF South-America region — ADR `2026-08-16-adr-edge-public-cache-invariants.md`).

## Verified facts (CF API + code, 2026-08-17 — NOT assumed)

- `worker/src/region-map.ts`: `MACRO_TO_COLO` has `apac → nrt`; `coloForMacro`/`isMacroRegion` already recognise
  `apac` (routable). `PROVISIONED_MACROS = {wnam, enam, weur}` — `apac` is routable-but-NOT-provisioned (signup
  rejects it). Rust mirror `crates/corelink-container/src/storage/region_map.rs:47` = `["wnam","enam","weur"]`
  with a **mirror-assert test at `:125`** (`assert_eq!(PROVISIONED_MACROS, [...])`) — any change touches TS +
  Rust + that test.
- R2 buckets `corelink-cas-nrt` and `corelink-cas-syd` **exist, are `location=ENAM`, EMPTY (0 objects), and are
  NOT bound** (the nrt/syd workers bind `corelink-cas-prod`, also ENAM). `corelink-cas-prod` under the `nrt/` and
  `syd/` key prefixes is **also empty** → there is **NO data to migrate**; the APAC cache starts cold and self-
  fills on demand (a `_public` cache is regenerable by construction).
- `corelink-cas-eu` is physically EU (jurisdiction=eu), bound by lhr — EU is already local (done pre-campaign).
- R2 location hints available: `wnam enam weur eeur apac oc` — **Tokyo → `apac`**, Sydney → `oc` (distinct).
- D1 `read_replication = auto` replicates APAC; **reads** get the nearest replica via the Sessions API. **Writes**
  still go to the single ENAM primary — unavoidable, and WP-B already moved the metering write off the hot path;
  a fill's `tenant_storage_state` write stays cross-region, but that is the MISS/write path, not the warm HIT the
  campaign optimises. Documented, not a defect.

## Scope

- **In:** nrt (Tokyo / `apac`) physical CAS bucket + bind + provision the `apac` macro + prove.
- **Deferred (documented follow-up, not blocking):** syd (Sydney) needs a NEW `oc` macro (not in the
  `MacroRegion` type today) + an OC-located bucket — a clean parallel repeat of this plan once there is APAC
  demand. The AC (action-cache) buckets `corelink-ac-{nrt,syd}` are also ENAM; AC is NOT on the edge-serve
  `_public` hot path (it flows through the container), so AC locality is a smaller, separate win — bundle only if
  cheap.

## Work packages (disjoint)

### WP4.1 — Infra: APAC-located CAS bucket (no code)
Delete the empty, unbound, ENAM `corelink-cas-nrt` and recreate it **APAC-located** (same name, keeps the naming
convention; empty ⇒ zero data loss). CF API: `DELETE /accounts/<a>/r2/buckets/corelink-cas-nrt` then
`POST /accounts/<a>/r2/buckets` `{"name":"corelink-cas-nrt","locationHint":"apac"}`. Verify
`GET .../buckets/corelink-cas-nrt` returns `location=APAC` before proceeding. (Risk: name-reuse settle delay
after delete — if create 409s, retry with backoff, or fall back to a new name `corelink-cas-apac` and bind that.)

### WP4.2 — Bind + deploy (worker config, nrt env only)
`wrangler.toml [env.prod-nrt]`: point `CAS_BUCKET` from `corelink-cas-prod` → `corelink-cas-nrt`. Keep
`R2_CAS_REGION="nrt"` (the key prefix is unchanged; the bucket is what moves). Redeploy `corelink-prod-nrt`
(worker-only, no container). This is disjoint from every other region's config. `R2_TDK_HEX` etc. already present
(verified in WP3). Rollback = point `CAS_BUCKET` back to `corelink-cas-prod` + redeploy.

### WP4.3 — Provision the `apac` macro (code PR, gated)
Add `apac` to `PROVISIONED_MACROS` in `worker/src/region-map.ts:67` AND
`crates/corelink-container/src/storage/region_map.rs:47`, and update the mirror-assert test at `region_map.rs:125`
(and any TS test asserting the set). This lets signup pin a tenant to `apac` (→ nrt). One concern, one PR; CI +
adversarial self-review; merge only when green. **Ordering: land 4.1 + 4.2 FIRST** — a provisioned `apac` macro
with an ENAM-bound nrt worker would serve APAC tenants from US storage (silent non-locality), so the physical
bucket must be live before the macro opens.

### WP4.4 — Prove by USE (the gate; nothing is "done" until this passes)
- Confirm `GET corelink-cas-nrt` = APAC (WP4.1) and the nrt worker binds it (deployed config).
- Pin a test tenant to `apac` (post-4.3), fill a `_public` bottle so an `nrt/<prefix>/<hash>` object lands in the
  **APAC** bucket (verify via S3 list that the object is physically in `corelink-cas-nrt`), then drive a warm
  cache HIT and confirm: `X-Cache: HIT`, served from nrt, and (from an APAC vantage — a runner box in an APAC
  colo, or Server-Timing) the `origin`/R2 phase is in-region, not cross-Pacific. Reuse the proven harness:
  mint via `scripts/admin/mint-dogfood-pat.sh`, drive `/v1/cas/<tenant>/<digest>` and the `/brew` `_public`
  surface, query prod D1 with `--env prod`. Async-meter convergence already proven on iad (identical code).
- If no APAC runner vantage is available, prove **physical placement** (object is in the APAC bucket) + functional
  HIT + Server-Timing locally, and record the same-region latency number as a follow-up once an APAC box exists —
  honestly, not hand-waved.

## Rollback
Per-layer, instant: WP4.2 rebind `CAS_BUCKET → corelink-cas-prod` + redeploy nrt; WP4.3 revert the macro PR
(signup stops pinning apac). WP4.1's recreated bucket is empty either way. No data at risk anywhere (all targets
empty).

## Risks & mitigations
- **Name-reuse settle after delete** (WP4.1): retry with backoff; fallback to `corelink-cas-apac` name.
- **Macro-before-bucket** ordering trap: enforced (4.1+4.2 before 4.3).
- **APAC proof vantage**: may need a runner box in an APAC colo; if unavailable, prove placement+functional now,
  latency-number as an honest follow-up (do NOT claim a same-region number without measuring it).
- **D1 write cross-region on fills**: acknowledged, out of scope (write path; WP-B already fixed the hot-path
  metering write). Not a locality regression — it is the pre-existing platform reality.
- **Mirror-assert test** (Rust `:125`): must be updated in the SAME PR or the container build fails.

## Self-review (techlead — I own this call)
- **Disjointness:** WP4.1 (R2 infra), WP4.2 (nrt-only wrangler), WP4.3 (region-map code) touch disjoint surfaces;
  no shared-file conflict. Sequential by dependency (bucket → bind → macro → prove), not parallel — correct, since
  the ordering is load-bearing (macro must not open before the bucket is local). No fan-out warranted.
- **No gambiarra / debt:** zero data migration (verified empty), the mirror test is updated (not bypassed), the
  deferred syd/OC + AC scope is explicitly documented (not silently dropped), and the D1-write-cross-region
  residual is disclosed, not hidden.
- **Prove-by-use is the gate**, not a formality — physical placement in the APAC bucket + a functional HIT are
  mandatory; the same-region latency number is honestly gated on an APAC vantage.
- **Reversible + safe:** every step rolls back instantly; nothing irreversible; no data at risk.
- **Verdict: APPROVED to execute.** Escalate to owner ONLY the go/no-go on *opening the `apac` macro to signup*
  (WP4.3) — that is a product-surface change (new sellable region), the one genuinely product-level decision here.
