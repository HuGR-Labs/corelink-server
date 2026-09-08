# WP4 plan — physical APAC R2 + wire nrt + provision the `apac` macro

Status: EXECUTED (2026-08-17; current state reconciled 2026-09-05)
Date: 2026-08-17
Campaign: multi-region closure (`[[multi-region-reality-and-closure-campaign]]`). Follows WP-A/WP-C/WP-B (all
LIVE) + WP3 (edge-serve + async-meter live on all 5 regions).

The work packages below are retained as the historical execution plan. The
current provisioned contract is `{wnam, enam, weur, apac}` with `apac → nrt`;
`sam` and `afr` remain valid macro codes but are unprovisioned and rejected at
signup. The APAC bucket is provisioned and bound before that macro is accepted.

## Goal

Make **Tokyo (nrt / `apac` macro) genuinely local**: physical APAC-located R2 CAS bucket + provision the `apac`
macro so tenants can pin there. Combined with the already-live WP-A (colo blob cache), WP-C (cached map read via
D1 replica), and WP-B (async metering — the cross-region metering WRITE is already off the hot path), an APAC
tenant's warm `_public` cache HIT then serves fully in-region: colo Cache API → APAC R2 origin, D1 APAC read
replica for the map, async metering. **No synchronous cross-Pacific hop on a warm HIT.**

SAM stays the documented platform limit (no CF South-America region — ADR `2026-08-16-adr-edge-public-cache-invariants.md`).

## Verified facts (CF API + code, 2026-08-17 — NOT assumed)

- `worker/src/region-map.ts` maps `apac → nrt` and its executable
  `PROVISIONED_MACROS` set is `{wnam, enam, weur, apac}`. The Rust mirror and
  mirror-assert test carry the same four values. `sam` and `afr` remain
  unprovisioned and signup rejects them.
- `corelink-cas-apac` is APAC-located and bound by the `prod-nrt` worker;
  APAC objects are written under the `nrt/` prefix. The bucket was empty before
  enablement, so there was **NO data to migrate**; the APAC cache starts cold
  and self-fills on demand (a `_public` cache is regenerable by construction).
- `corelink-cas-eu` is physically EU (jurisdiction=eu), bound by lhr — EU is already local (done pre-campaign).
- R2 location hints available: `wnam enam weur eeur apac oc` — **Tokyo → `apac`**, Sydney → `oc` (distinct).
- D1 `read_replication = auto` replicates APAC; **reads** get the nearest replica via the Sessions API. **Writes**
  still go to the single ENAM primary — unavoidable, and WP-B already moved the metering write off the hot path;
  a fill's `tenant_storage_state` write stays cross-region, but that is the MISS/write path, not the warm HIT the
  campaign optimises. Documented, not a defect.

## Scope

- **Completed:** nrt (Tokyo / `apac`) physical CAS bucket + bind + provisioned
  the `apac` macro + proof gate. Sydney remains deferred.
- **Deferred (documented follow-up, not blocking):** syd (Sydney) needs a NEW `oc` macro (not in the
  `MacroRegion` type today) + an OC-located bucket — a clean parallel repeat of this plan once there is APAC
  demand. The AC (action-cache) buckets `corelink-ac-{nrt,syd}` are also ENAM; AC is NOT on the edge-serve
  `_public` hot path (it flows through the container), so AC locality is a smaller, separate win — bundle only if
  cheap.

## Work packages (disjoint)

### WP4.1 — Infra: APAC-located CAS bucket (completed)
Historical execution deleted the empty, unbound ENAM bucket and created
`corelink-cas-apac` with `locationHint=apac` (zero data loss). The retained
deployment evidence records `corelink-cas-apac` as APAC-located; this is the
bucket used by the current `prod-nrt` binding.

### WP4.2 — Bind + deploy (completed; worker config, nrt env only)
`wrangler.toml [env.prod-nrt]` points `CAS_BUCKET` from `corelink-cas-prod` → `corelink-cas-apac`. It keeps
`R2_CAS_REGION="nrt"` (the key prefix is unchanged; the bucket is what moves). Redeploy `corelink-prod-nrt`
(worker-only, no container). This is disjoint from every other region's config. `R2_TDK_HEX` etc. already present
(verified in WP3). Rollback = point `CAS_BUCKET` back to `corelink-cas-prod` + redeploy.

### WP4.3 — Provision the `apac` macro (completed; code PR and gate)
`apac` is present in `PROVISIONED_MACROS` in both region-map implementations,
the mirror-assert tests agree on `{wnam, enam, weur, apac}`, and signup pins
APAC tenants to `apac` (→ nrt). WP4.1 and WP4.2 landed before this macro was
opened, so the physical bucket was local before provisioning accepted APAC.

### WP4.4 — Prove by USE (the gate; nothing is "done" until this passes)
- Confirm `GET corelink-cas-apac` = APAC (WP4.1) and the nrt worker binds it (deployed config).
- Pin a test tenant to `apac`, fill a `_public` bottle so an `nrt/<prefix>/<hash>` object lands in the
  **APAC** bucket (verify via S3 list that the object is physically in `corelink-cas-apac`), then drive a warm
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
- **Verdict: EXECUTED.** The APAC macro is open only because the APAC bucket and
  nrt binding were in place first. Any future region still requires the same
  physical-placement proof and an explicit owner decision.
