/** Per-tier quota enforcement stage. */
import type { ExecutionContext } from "@cloudflare/workers-types";
import type { Env } from "./index_common.js";
import { applyCors, DO_METER_LEASE_BLOCK } from "./index_auth.js";
import type { KvReader } from "./lib/pat_verify_cache.js";
import {
  runQuotaBatch, requestCapResultForCount, storageQuotaHeaderValue, storageCapIsFinite,
  FREE_REQUEST_CAP, STORAGE_QUOTA_HEADER, QUOTAS, currentYearMonthUtc, secondsUntilNextMonthStart,
} from "./lib/quota.js";
import { serveViaDO, serveGateActive } from "./lib/edge_do_meter.js";
import { checkStorageQuotaCachedRead } from "./lib/quota_storage_cache.js";
import { tryFastRequestCount, decideFastPath, populateRequestCountKv } from "./lib/quota_request_cache.js";
import { resolveTenantTierCached } from "./lib/tenant_tier_cache.js";
import { quotaExceededResponse, quotaPathFor } from "./index_observability.js";
import { constantTimeSecretEqual } from "./lib/internal_auth.js";

export interface QuotaStageResult {
  readonly storageQuotaHeader: string | null;
  readonly pendingCapVerdict: Promise<{ withinCap: boolean; refilled: boolean } | null> | null;
  readonly deferredCap: { cap: number; tier: string } | null;
  readonly deferredCapTierResult: Awaited<ReturnType<typeof resolveTenantTierCached>> | null;
  readonly stQTierMs: number;
  readonly stQDoMs: number;
  readonly stQBatchMs: number;
  readonly response?: never;
}

export async function enforceQuota(
  request: Request,
  env: Env,
  ctx: ExecutionContext,
  requestId: string,
  resolvedTenantId: string,
): Promise<QuotaStageResult | Response> {
  let pendingCapVerdict: Promise<{ withinCap: boolean; refilled: boolean } | null> | null = null;
  let deferredCap: { cap: number; tier: string } | null = null;
  let deferredCapTierResult: Awaited<ReturnType<typeof resolveTenantTierCached>> | null = null;
  let stQTierMs = -1;
  let stQDoMs = -1;
  let stQBatchMs = -1;
    let storageQuotaHeader: string | null = null;
    if (resolvedTenantId !== "_anonymous" && resolvedTenantId !== "_system" && resolvedTenantId !== "_pending") {
      // Helper: emit the shared 429 quota-exceeded response shape.
      const quotaExceeded = (reason: string, retryAfterSec: number): Response =>
        applyCors(quotaExceededResponse(reason, retryAfterSec, requestId), request);

      // Request-count quota is ENFORCED BY DEFAULT (fail-CLOSED). The monthly
      // per-tenant request cap is a CONTRACTED ceiling, so an unset env var in
      // prod must NOT silently disable it (Cluster D). The gate is an explicit
      // opt-OUT kill-switch: enforcement is live UNLESS REQUEST_QUOTA_DISABLED
      // === "true" (set only in dev/test). Mirrors the fail-closed posture of
      // the other quota/security gates in this file.
      const requestQuotaEnabled = env.REQUEST_QUOTA_DISABLED !== "true";

      // ── #11: multi-region fan-out over-count fix ──────────────────────────
      // A regional Worker invocation that is itself an INTERNAL fan-out sub-
      // request of one logical client request must NOT re-meter that request:
      // the PRIMARY Worker already incremented the monthly counter once before
      // it fanned out. Counting again here double-charges multi-region tenants
      // (customer-unfavourable).
      //
      // FORGERY-SAFE (tech-lead review of b5ba30c1): the fan-out marker MUST be
      // a value a client cannot forge. The public edge does NOT ingress-strip
      // x-corelink-fanout-from, so a PRESENCE check on the header alone would let
      // ANY client send `x-corelink-fanout-from: anything` to SKIP metering → a
      // request-quota BYPASS (fail-OPEN — worse than the over-count it replaced).
      // So the marker carries the shared server-to-server secret
      // CORELINK_INTERNAL_AUTH_KEY (bound on [env.prod] AND every regional worker
      // env — prod-sam/lhr/nrt/syd — per ADR-MULTI-REGION-V1 §Consequences and the
      // wrangler.toml per-region secret block). The primary Worker sets the header
      // to that secret AFTER stripClientTrustHeaders on the fan-out forward, and it
      // travels ONLY over the service binding (never to a client). We treat the
      // request as a fan-out ONLY on a CONSTANT-TIME match against that secret:
      // a forged value ("prod", a random guess, "") does NOT match → metering
      // still happens. Fail-SAFE: if the secret is unbound, the match can never
      // succeed → every request meters (no bypass).
      //
      // SCOPE: this gates ONLY the metering (the increment + the request-cap
      // comparison). Tier resolution (getTierForTenant) and the server-trusted
      // STORAGE_QUOTA_HEADER forwarding stay UNCONDITIONAL below — a fan-out
      // sub-request still needs the resolved storage cap forwarded to its
      // regional container, and gating those would re-introduce the regional
      // storage-header regression.
      const fanoutHeader = request.headers.get("x-corelink-fanout-from");
      const isFanout =
        fanoutHeader !== null &&
        typeof env.CORELINK_INTERNAL_AUTH_KEY === "string" &&
        env.CORELINK_INTERNAL_AUTH_KEY.length > 0 &&
        constantTimeSecretEqual(env.CORELINK_INTERNAL_AUTH_KEY, fanoutHeader);

      // UNCONDITIONAL (also on fan-out): the served path — including a fan-out
      // sub-request forwarding to its regional container — needs the resolved
      // tier for the server-trusted STORAGE_QUOTA_HEADER. Gating these on
      // !isFanout would re-introduce the regional storage-header regression.
      // Latency WP slice 2: resolve the tier through the L1→KV-L2→D1 cache
      // (`ttier:`), mirroring the residency KV-L2 (slice 1) — collapses the tier
      // pair of the `wdb` quota trio for far-from-D1 (SAM/GRU) callers. Returns
      // the same TierResult shape; an unconfirmed (`d1Error`) result is never
      // cached, so the F21 fail-open posture is preserved.
      //
      // Resolved FIRST (it used to run between the counter UPSERT and the storage
      // SUM) because the batch below needs the tier to decide whether the storage
      // read is worth issuing at all. It is cache-served in steady state — the
      // 2026-08-04 prod measurement puts `qtier` at 0/0/3 ms — and it was already
      // UNCONDITIONAL, so nothing pays extra for the reorder: the same reads
      // happen, in a different order.
      const tierKv = (env as unknown as { METADATA_KV?: KvReader }).METADATA_KV;
      const tierStart = Date.now();
      const quotaTier = await resolveTenantTierCached(env.CONFIG_DB, resolvedTenantId, {
        ...(tierKv ? { kv: tierKv } : {}),
        waitUntil: ctx.waitUntil.bind(ctx),
      });
      stQTierMs = Date.now() - tierStart;
      storageQuotaHeader = storageQuotaHeaderValue(quotaTier);

      // A storage-increasing op is a write verb (PUT uploads / POST). DELETE
      // reduces storage and reads (GET/HEAD) cannot grow it, so both stay
      // available during a D1 outage.
      const isStorageMutating = request.method === "PUT" || request.method === "POST";
      const meter = !isFanout && requestQuotaEnabled;

      // ── P3 EDGE_DO_METER serve (ADR 2026-08-19): the edge-local DO is the
      //    AUTHORITATIVE monthly request-count cap ──────────────────────────
      // When EDGE_DO_METER === "serve" (per-region flag) AND this is a
      // genuinely-counted request on a CAPPED tier with both DOs + a region
      // bound, we meter this request through the shard/coordinator SYNCHRONOUSLY
      // (awaited — the verdict is a serve decision, not telemetry) with
      // reconcile→D1 ON, and treat the DO's `withinCap` as the request-cap
      // verdict. On success we DROP the per-request D1 counter UPSERT (pass
      // `meter:false` to runQuotaBatch, storage-only) and SKIP the async-`on`
      // fast path and the D1 `withinFreeCap` gate below — the DO owns the cap.
      //
      // FAIL-OPEN (money-path safety): if serveViaDO THROWS (any DO hop non-2xx
      // / outage), we swallow it to `serveVerdict = null` and fall straight back
      // to today's exact D1 count path (meter stays true, async/D1 metering +
      // withinFreeCap run). A DO outage must NEVER break a request nor fail it
      // closed. Storage quota is ALWAYS enforced regardless (serve concerns ONLY
      // the request-count cap). FAN-OUT: the gate requires meter===true, so a
      // fan-out sub-request (meter=false) never hits the DO — no double count.
      const doServeCap = QUOTAS[quotaTier.tier].requestsPerMonthMax;
      const doServeActive = serveGateActive({
        mode: (env as unknown as { EDGE_DO_METER?: string }).EDGE_DO_METER,
        meter,
        hasShardNs: env.REQUEST_METER_SHARD_DO !== undefined,
        hasCoordNs: env.REQUEST_METER_COORDINATOR_DO !== undefined,
        region: env.R2_CAS_REGION,
        cap: doServeCap,
      });
      let serveVerdict: { withinCap: boolean; refilled: boolean } | null = null;
      // The extra env truthiness checks are always-true given doServeActive
      // (serveGateActive already required them) — they are here solely so TS
      // narrows the optional DO namespaces + region, mirroring the shadow branch.
      if (
        doServeActive &&
        env.REQUEST_METER_SHARD_DO &&
        env.REQUEST_METER_COORDINATOR_DO &&
        env.R2_CAS_REGION
      ) {
        const shardNs = env.REQUEST_METER_SHARD_DO;
        const coordNs = env.REQUEST_METER_COORDINATOR_DO;
        // Defer the hop OFF the critical path when the request is a READ that
        // would take the `serve-fast` quota path anyway: the storage verdict then
        // comes from the ≤60 s-fresh B1 cache, so this hop is the ONLY thing left
        // between auth and the origin fetch, and the fetch does not depend on its
        // answer. See `pendingCapVerdict` for the trade and why GET/HEAD only.
        const deferCapVerdict = deferCapVerdictFor({
          method: request.method,
          isStorageMutating,
          asyncMeterMode: (env as unknown as { EDGE_ASYNC_METER?: string }).EDGE_ASYNC_METER,
          meter,
          tierD1Error: quotaTier.d1Error === true,
        });
        const qdoStart = Date.now();
        if (deferCapVerdict) {
          deferredCap = { cap: doServeCap, tier: quotaTier.tier };
          deferredCapTierResult = quotaTier;
          pendingCapVerdict = serveViaDO(
            { shard: shardNs, coordinator: coordNs },
            {
              tenantId: resolvedTenantId,
              region: env.R2_CAS_REGION,
              yearMonth: currentYearMonthUtc(),
              cap: doServeCap,
              block: Math.min(doServeCap, DO_METER_LEASE_BLOCK),
              lowWater: 0,
            },
          )
            .then((v) => {
              stQDoMs = Date.now() - qdoStart;
              return v;
            })
            .catch(() => {
              // Fail-OPEN, exactly as the awaited path does — but the D1 count
              // fallback moves OFF the response path (see the await site).
              stQDoMs = Date.now() - qdoStart;
              return null;
            });
        } else {
        try {
          serveVerdict = await serveViaDO(
            { shard: shardNs, coordinator: coordNs },
            {
              tenantId: resolvedTenantId,
              region: env.R2_CAS_REGION,
              yearMonth: currentYearMonthUtc(),
              cap: doServeCap,
              block: Math.min(doServeCap, DO_METER_LEASE_BLOCK),
              lowWater: 0,
            },
          );
        } catch {
          // Fail-OPEN: fall back to the normal D1 count path below.
          serveVerdict = null;
        } finally {
          // Timed even on the throw path above: the outage still cost wall
          // time inside `wdb`, and the fail-open `catch` must not make it
          // disappear from the header.
          stQDoMs = Date.now() - qdoStart;
        }
        // P3 serve telemetry: mirror `do_meter_shadow` for the LIVE serve path so
        // its verdict + hop-health are observable in prod. ONLY on the success
        // path (serveVerdict non-null); on a DO error the catch above already
        // fell open and the D1 fallback owns the request — nothing to log. Cheap
        // synchronous console.log (no ctx.waitUntil); no tenant id (region+tier
        // are not PII, matching shadow). Emits no control-flow effect.
        if (serveVerdict !== null) {
          console.log(
            JSON.stringify({
              evt: "do_meter_serve",
              tier: quotaTier.tier,
              region: env.R2_CAS_REGION,
              withinCap: serveVerdict.withinCap,
              refilled: serveVerdict.refilled,
            }),
          );
        }
        }
      }
      // True iff the DO authoritatively metered this request (verdict in hand).
      const serveHandled = serveVerdict !== null;

      // ── WP-B (async metering): take the quota trio off the WARM READ path ──
      // For a metered, tier-CONFIRMED, NON-mutating request whose tenant is
      // provably far under its request cap, serve with ZERO synchronous D1: the
      // monthly-counter increment runs via ctx.waitUntil (off the hot path) and
      // the storage verdict comes from the B1 KV cache (a READ cannot grow
      // storage, so a ≤60s-stale byte count is safe and customer-favorable).
      // Every OTHER case — a WRITE verb, a fan-out/kill-switched request, an
      // unconfirmed (d1Error) tier, a KV miss, or a tenant within burstMargin of
      // its cap — falls through to the EXACT `runQuotaBatch` path below,
      // byte-identical to before. Flag `EDGE_ASYNC_METER`: unset/off ⇒ exact path
      // (today's behaviour); `shadow` ⇒ exact path + a no-PII divergence log to
      // prove the arming rule on real traffic; `on` ⇒ serve the fast path.
      // ADR: docs/design/2026-08-17-adr-edge-async-metering.md.
      const asyncMeterMode = (env as unknown as { EDGE_ASYNC_METER?: string }).EDGE_ASYNC_METER;
      const asyncMeterEligible =
        meter && !isStorageMutating && !quotaTier.d1Error;
      let handledFast = false;

      // ── The serve path used to force a synchronous D1 storage SUM ─────────
      // Measured from inside the fabric (colo=IAD, corelink-runners run
      // 32792837540, 2026-08-25): `qbatch` 35-51 ms on EVERY warm authed read,
      // and 115-124 ms on the same request from São Paulo. It should not have
      // been there at all. `EDGE_ASYNC_METER="on"` (WP-B) exists precisely to
      // take that round trip off a warm READ, but its arming condition began
      // with `!serveHandled`, so flipping `EDGE_DO_METER="serve"` — which is on
      // in every prod region — silently disabled it for every capped tier. Two
      // optimizations that compose perfectly were mutually exclusive by an
      // accident of ordering.
      //
      // They compose because they own DIFFERENT caps. The DO is authoritative
      // for the request COUNT (it already metered this request, verdict in
      // hand), which leaves only the STORAGE verdict — and a READ cannot grow
      // storage, so the B1 KV byte count answers it with the same verdict the
      // live SUM would return, from a ≤60 s-fresh read. That is the identical
      // trade `asyncMeterEligible` already encodes and the async-metering ADR
      // already accepted; this branch changes WHO ELSE may take it, not WHAT is
      // traded.
      //
      // Enforcement order is preserved exactly: storage 429 first, then the DO's
      // request-cap 429 — the same precedence the `!handledFast` block below
      // applies on the serve path. Both are enforced HERE because setting
      // `handledFast` skips that block entirely; forgetting the DO verdict here
      // would be a money-path hole, not an optimization.
      // A DEFERRED verdict counts as `serveHandled` for path selection: the DO
      // hop is in flight and owns the request count, so the D1 counter must NOT
      // also run (that would double-count the same request). The verdict itself
      // is enforced at the await site below, not here — which is why the
      // `serve-fast` branch's `serveVerdict !== null` guard is correctly inert on
      // this path rather than silently skipped.
      const quotaPath = quotaPathFor({
        serveHandled: serveHandled || pendingCapVerdict !== null,
        asyncMeterMode,
        asyncMeterEligible,
      });
      if (quotaPath === "serve-fast") {
        const storageCheck = await checkStorageQuotaCachedRead(
          env.CONFIG_DB,
          resolvedTenantId,
          quotaTier.tier,
          false,
          storageCapIsFinite(quotaTier.tier),
          { ...(tierKv ? { kv: tierKv } : {}), waitUntil: ctx.waitUntil.bind(ctx) },
        );
        if (!storageCheck.ok) {
          return quotaExceeded(storageCheck.reason, storageCheck.retryAfterSec);
        }
        if (serveVerdict !== null && !serveVerdict.withinCap) {
          return quotaExceeded(
            `Monthly request quota exceeded: limit is ${doServeCap} (tier: ${quotaTier.tier})`,
            secondsUntilNextMonthStart(),
          );
        }
        handledFast = true;
      } else if (quotaPath === "fast") {
        const fast = await tryFastRequestCount(env.CONFIG_DB, resolvedTenantId, quotaTier.tier, {
          ...(tierKv ? { kv: tierKv } : {}),
          waitUntil: ctx.waitUntil.bind(ctx),
        });
        if (fast !== null) {
          // Request cap: armed ⇒ headroom > burstMargin ⇒ within cap by
          // construction. Storage: B1 cached read verdict (same verdict the live
          // SUM would return, from a ≤60s-fresh byte count).
          const storageCheck = await checkStorageQuotaCachedRead(
            env.CONFIG_DB,
            resolvedTenantId,
            quotaTier.tier,
            false,
            storageCapIsFinite(quotaTier.tier),
            { ...(tierKv ? { kv: tierKv } : {}), waitUntil: ctx.waitUntil.bind(ctx) },
          );
          if (!storageCheck.ok) {
            return quotaExceeded(storageCheck.reason, storageCheck.retryAfterSec);
          }
          handledFast = true;
        }
      } else if (quotaPath === "shadow") {
        // Canary: compute the arming DECISION (no side effect, no double-count)
        // and log it; the exact path below still serves. No tenant id (no PII).
        ctx.waitUntil(
          decideFastPath(resolvedTenantId, quotaTier.tier, {
            ...(tierKv ? { kv: tierKv } : {}),
          })
            .then((d) => {
              console.log(
                JSON.stringify({
                  evt: "async_meter_shadow",
                  tier: quotaTier.tier,
                  arm: d.arm,
                  reason: d.reason,
                  cachedCount: d.cachedCount,
                }),
              );
            })
            .catch(() => {
              /* telemetry must never break the request */
            }),
        );
      }

      // ── ONE round trip for both uncached D1 statements ────────────────────
      // The monthly-counter UPSERT and the storage `SUM(bytes_used)` read used to
      // run as two SERIAL awaits, and the 2026-08-04 prod measurement (warm, n=30)
      // showed they were the ENTIRE `wdb` phase: `qmeter` 152/158/163 ms + `qstor`
      // 120/126/130 ms = `wdb` 277/284/302 ms, with sum(4) − wdb = 0 on 30/30
      // requests. Neither reads the other's result, so the seriality bought
      // nothing and cost a full ENAM round trip. `db.batch` issues both in one.
      //
      // The counter is still incremented SYNCHRONOUSLY here, and deliberately: it
      // is a billing/quota counter whose post-increment value IS the 429 decision
      // below, and `ctx.waitUntil` carries no durability guarantee (an evicted
      // isolate drops the pending write → a silently under-counted tenant). What
      // is counted, when, and against which cap is byte-for-byte unchanged; only
      // the number of network round trips changed. (`runQuotaBatch` documents the
      // one real consequence: a D1 batch is a transaction, so a fault now rolls
      // the increment back too — an UNDER-count, the direction this counter's
      // fail-open design already tolerates, never an over-count.)
      //
      // #11: skip the metering UPSERT on an internal fan-out sub-request (the
      // primary Worker already counted this logical request). `counted:false`
      // yields `withinFreeCap === true`, so the request-cap comparison below is
      // also skipped — a fan-out sub-request is never re-metered nor 429'd on the
      // request cap. The storage read is likewise skipped for an unlimited-storage
      // tier or an unconfirmed (`d1Error`) one, exactly as before.
      //
      // rt-nuclear #24 (accepted, narrowed): the counter UPSERT used to run FIRST
      // so a tenant already over even the LOWEST tier cap was 429'd without paying
      // the storage SUM. Batched, an over-cap tenant now also pays that read — but
      // in the SAME round trip, so the cost is rows-read, not latency, and it is
      // bounded (a SUM over that tenant's handful of `tenant_storage_state` rows).
      // The alternative — remembering that this tenant was over-cap to skip the
      // read — is a cached authorization decision, which this path must not have.
      if (!handledFast) {
      const qbatchStart = Date.now();
      const quotaBatch = await runQuotaBatch(env.CONFIG_DB, resolvedTenantId, quotaTier, {
        // P3 serve: the DO authoritatively counted this request, so DROP the
        // per-request D1 counter UPSERT (storage-only batch). Otherwise unchanged.
        meter: serveHandled ? false : meter,
        isMutating: isStorageMutating,
      });
      // `-1` (phase omitted) iff no round trip was issued at all — both statements
      // skipped. Anything else is a real measurement of the one round trip, 0
      // included. Unlike the `qmeter`/`qstor` pair it replaces, `qbatch` does NOT
      // reveal WHICH statements were in it, which incidentally closes the
      // confirmation oracle the old `qmeter` omission gave a fan-out caller (an
      // absent `qmeter` beside a present `qtier`/`qstor`/`qresid` confirmed that
      // their `x-corelink-fanout-from` matched CORELINK_INTERNAL_AUTH_KEY). What
      // remains is strictly narrower: `qbatch` is absent only when the metering
      // AND the storage statement are both skipped, i.e. a fan-out (or
      // kill-switched) request from an unlimited-STORAGE tenant — enterprise only.
      if (quotaBatch.ranD1) stQBatchMs = Date.now() - qbatchStart;
      const inc = quotaBatch.increment;

      // P3 EDGE_DO_METER shadow (ADR 2026-08-19): run the edge-local DO metering
      // path in parallel to the authoritative D1 verdict and log agreement. D1
      // still enforces below; the DO path performs NO enforcement here. Runs OFF
      // the response path (`ctx.waitUntil`) so it never adds latency, and only for
      // a genuinely-counted request (`inc.counted` — a fan-out sub-request is not
      // re-metered) on a CAPPED tier (uncapped never denies). No tenant id is
      // logged (region is a colo code + tier are not PII). NOTE: an UNSEEDED
      // shadow diverges from D1 mid-month by construction (the DO count starts at
      // 0 while D1 already holds this month's count) — this shadow validates the
      // DO MECHANISM (lease math, hop health, error rate, zero user impact), not
      // absolute mid-month parity; parity requires the WP-4 DO→D1 reconcile/seed.
      const doMeterMode = (env as unknown as { EDGE_DO_METER?: string }).EDGE_DO_METER;
      if (
        doMeterMode === "shadow" &&
        inc.counted &&
        env.REQUEST_METER_SHARD_DO &&
        env.REQUEST_METER_COORDINATOR_DO &&
        env.R2_CAS_REGION
      ) {
        const doMeterCap = QUOTAS[quotaTier.tier].requestsPerMonthMax;
        if (doMeterCap !== Number.MAX_SAFE_INTEGER) {
          const d1WithinCap =
            inc.count <= FREE_REQUEST_CAP ||
            requestCapResultForCount(inc.count, quotaTier.tier).ok;
          const tier = quotaTier.tier;
          const region = env.R2_CAS_REGION;
          const shardNs = env.REQUEST_METER_SHARD_DO;
          const coordNs = env.REQUEST_METER_COORDINATOR_DO;
          ctx.waitUntil(
            meterViaDO(
              { shard: shardNs, coordinator: coordNs },
              {
                tenantId: resolvedTenantId,
                region,
                yearMonth: currentYearMonthUtc(),
                cap: doMeterCap,
                block: Math.min(doMeterCap, DO_METER_LEASE_BLOCK),
                lowWater: 0,
              },
            )
              .then((v) => {
                console.log(
                  JSON.stringify({
                    evt: "do_meter_shadow",
                    tier,
                    region,
                    d1WithinCap,
                    doWithinCap: v.withinCap,
                    agree: d1WithinCap === v.withinCap,
                    refilled: v.refilled,
                  }),
                );
              })
              .catch(() => {
                /* telemetry must never break the request */
              }),
          );
        }
      }

      // Monthly request-count quota (red-team #5): compare the already-counted
      // value against the tenant's RESOLVED tier cap. No re-increment. Within
      // the free cap → already within every cap, skip the comparison. Over the
      // free cap → a free tenant rejects HERE; a paid tenant under its (higher)
      // cap passes through. Checked BEFORE the storage verdict so the 429 a
      // client sees for a doubly-over-cap tenant is unchanged by the batching.
      const withinFreeCap = !inc.counted || inc.count <= FREE_REQUEST_CAP;
      if (!withinFreeCap) {
        const requestCheck = requestCapResultForCount(inc.count, quotaTier.tier);
        if (!requestCheck.ok) {
          return quotaExceeded(requestCheck.reason, requestCheck.retryAfterSec);
        }
      }

      // Storage quota verdict. (OCI never reaches this PAT-gate path — see the
      // dedicated pass-through branch above; the container enforces OCI quota.)
      // Storage is ALWAYS enforced — serve concerns ONLY the request-count cap.
      const storageCheck = quotaBatch.storage;
      if (!storageCheck.ok) {
        return quotaExceeded(storageCheck.reason, storageCheck.retryAfterSec);
      }

      // P3 serve: the edge-local DO is AUTHORITATIVE for the request-count cap.
      // Enforce its verdict here — AFTER storage — REPLACING the D1 `withinFreeCap`
      // gate above (which is inert on the serve path: meter=false ⇒ inc.counted=
      // false ⇒ withinFreeCap=true, so it never fires). `serveVerdict` is non-null
      // only when the DO metered this request (serve active + no DO error). The
      // 429 shape matches requestCapResultForCount: Retry-After = seconds to the
      // next UTC-month reset (the DO count is authoritative, so no D1 `count` is
      // available to echo).
      if (serveVerdict !== null && !serveVerdict.withinCap) {
        return quotaExceeded(
          `Monthly request quota exceeded: limit is ${doServeCap} (tier: ${quotaTier.tier})`,
          secondsUntilNextMonthStart(),
        );
      }

      // WP-B2: seed the request-count KV from this authoritative count so
      // subsequent reads for this tenant can arm the async fast path. Only when
      // we actually counted (not a fan-out / kill-switch / uncounted fault) and
      // the flag is engaged (shadow or on). Best-effort, off the response path.
      if (
        (asyncMeterMode === "on" || asyncMeterMode === "shadow") &&
        inc.counted &&
        tierKv !== undefined
      ) {
        populateRequestCountKv(resolvedTenantId, inc.count, {
          kv: tierKv,
          waitUntil: ctx.waitUntil.bind(ctx),
        });
      }
      }
    }

    // WI-MULTI-REGION-V1: per-tenant region routing. Look up tenant.primary_region
    // from D1, map the MACRO code to its serving colo via the FROZEN region map,
    // and fan-out to the regional Worker via Service Binding when the colo is
    // non-IAD. (backlog #29 — Schrems II residency leak.)
    //
    // The previous code routed by literal colo strings ("lhr"/"nrt"/"syd") that
    // the macro `primary_region` NEVER equals (D1 holds wnam/enam/weur/sam/...),
    // so EU `weur` tenants never matched "lhr" and stayed on IAD = US storage.
    // The colo map removes that vocabulary mismatch.
    //
    // FAIL-CLOSED for non-IAD residency regions: if the macro maps to a non-IAD
    // colo but the matching Service Binding is missing, OR if the D1 lookup
    // throws, we return 503 — we DO NOT fall through to the IAD path, because
    // that fall-through is precisely the cross-border leak. wnam/enam map to IAD
    // (the local path) so they legitimately fall through to the DO below.
    // F-014: recompute the forgery-safe fan-out marker in THIS scope (the metering
    // block's isFanout above is scoped to that sibling block). Same constant-time
    // secret check — a fanned-out request (already on a regional Worker) must not
    // re-route through the region fan-out below (the regional envs lack PROD_*
    // bindings → 503). Pure check, no side effects.

  return {
    storageQuotaHeader,
    pendingCapVerdict,
    deferredCap,
    deferredCapTierResult,
    stQTierMs,
    stQDoMs,
    stQBatchMs,
  };
}
