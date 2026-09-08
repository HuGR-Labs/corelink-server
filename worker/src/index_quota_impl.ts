
import type { ExecutionContext } from "@cloudflare/workers-types";
import type { Env } from "./index_common.js";
import { applyCors, DO_METER_LEASE_BLOCK } from "./index_auth.js";
import type { KvReader } from "./lib/pat_verify_cache.js";
import {
  runQuotaBatch, requestCapResultForCount, storageQuotaHeaderValue, storageCapIsFinite,
  FREE_REQUEST_CAP, STORAGE_QUOTA_HEADER, QUOTAS, currentYearMonthUtc, secondsUntilNextMonthStart,
} from "./lib/quota.js";
import { meterViaDO, serveViaDO, serveGateActive } from "./lib/edge_do_meter.js";
import { checkStorageQuotaCachedRead } from "./lib/quota_storage_cache.js";
import { tryFastRequestCount, decideFastPath, populateRequestCountKv } from "./lib/quota_request_cache.js";
import { resolveTenantTierCached } from "./lib/tenant_tier_cache.js";
import { deferCapVerdictFor, quotaExceededResponse, quotaPathFor } from "./index_observability.js";
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

      const quotaExceeded = (reason: string, retryAfterSec: number): Response =>
        applyCors(quotaExceededResponse(reason, retryAfterSec, requestId), request);

      const requestQuotaEnabled = env.REQUEST_QUOTA_DISABLED !== "true";

      const fanoutHeader = request.headers.get("x-corelink-fanout-from");
      const isFanout =
        fanoutHeader !== null &&
        typeof env.CORELINK_INTERNAL_AUTH_KEY === "string" &&
        env.CORELINK_INTERNAL_AUTH_KEY.length > 0 &&
        constantTimeSecretEqual(env.CORELINK_INTERNAL_AUTH_KEY, fanoutHeader);

      const tierKv = (env as unknown as { METADATA_KV?: KvReader }).METADATA_KV;
      const tierStart = Date.now();
      const quotaTier = await resolveTenantTierCached(env.CONFIG_DB, resolvedTenantId, {
        ...(tierKv ? { kv: tierKv } : {}),
        waitUntil: ctx.waitUntil.bind(ctx),
      });
      stQTierMs = Date.now() - tierStart;
      storageQuotaHeader = storageQuotaHeaderValue(quotaTier);

      const isStorageMutating = request.method === "PUT" || request.method === "POST";
      const meter = !isFanout && requestQuotaEnabled;

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

      if (
        doServeActive &&
        env.REQUEST_METER_SHARD_DO &&
        env.REQUEST_METER_COORDINATOR_DO &&
        env.R2_CAS_REGION
      ) {
        const shardNs = env.REQUEST_METER_SHARD_DO;
        const coordNs = env.REQUEST_METER_COORDINATOR_DO;

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

          serveVerdict = null;
        } finally {

          stQDoMs = Date.now() - qdoStart;
        }

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

      const serveHandled = serveVerdict !== null;

      const asyncMeterMode = (env as unknown as { EDGE_ASYNC_METER?: string }).EDGE_ASYNC_METER;
      const asyncMeterEligible =
        meter && !isStorageMutating && !quotaTier.d1Error;
      let handledFast = false;

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

            }),
        );
      }

      if (!handledFast) {
      const qbatchStart = Date.now();
      const quotaBatch = await runQuotaBatch(env.CONFIG_DB, resolvedTenantId, quotaTier, {

        meter: serveHandled ? false : meter,
        isMutating: isStorageMutating,
      });

      if (quotaBatch.ranD1) stQBatchMs = Date.now() - qbatchStart;
      const inc = quotaBatch.increment;

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

              }),
          );
        }
      }

      const withinFreeCap = !inc.counted || inc.count <= FREE_REQUEST_CAP;
      if (!withinFreeCap) {
        const requestCheck = requestCapResultForCount(inc.count, quotaTier.tier);
        if (!requestCheck.ok) {
          return quotaExceeded(requestCheck.reason, requestCheck.retryAfterSec);
        }
      }

      const storageCheck = quotaBatch.storage;
      if (!storageCheck.ok) {
        return quotaExceeded(storageCheck.reason, storageCheck.retryAfterSec);
      }

      if (serveVerdict !== null && !serveVerdict.withinCap) {
        return quotaExceeded(
          `Monthly request quota exceeded: limit is ${doServeCap} (tier: ${quotaTier.tier})`,
          secondsUntilNextMonthStart(),
        );
      }

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
