/** Scheduled synthetic drill delivery domain. */
import type { ScheduledController } from "@cloudflare/workers-types";
import type { Env } from "./index_common.js";
import { scheduledDrillForCron, scheduledWeekNumber, syntheticRegionForWeek, syntheticEmitAtMs, SYNTHETIC_PAGE_CONTRACT } from "./index_common.js";
export async function runScheduled(controller: ScheduledController, env: Env): Promise<void> {
    const drill = scheduledDrillForCron(controller.cron);
    if (drill === undefined) {
      // A cron is deployment configuration, not user input. Do not echo it in
      // telemetry anyway: keeping the event fixed makes this path incapable of
      // carrying a future secret-bearing configuration value into logs.
      console.error("[scheduled_drill] rejected reason=unknown_cron");
      controller.noRetry();
      throw new Error("unknown scheduled drill");
    }

    const delivery = env.SCHEDULED_DRILL_DELIVERY;
    if (delivery === undefined) {
      console.error(`[scheduled_drill] failed drill=${drill} reason=delivery_binding_unavailable`);
      throw new Error("scheduled drill delivery binding unavailable");
    }

    // `cron` plus Cloudflare's scheduled time is deterministic for this tick,
    // so the downstream receiver can make a retry idempotent. The body carries
    // only scheduler metadata — never a routing key, token, or customer data.
    // One canonical identifier is carried in the service-binding header, the
    // receiver D1 row, and PagerDuty's dedup_key. The scheduled timestamp is
    // stable for a Cloudflare retry and unique for each weekly tick.
    const deliveryId = `SP-${controller.scheduledTime}`;
    const week = scheduledWeekNumber(controller.scheduledTime);
    const providerMode = env.SYNTHETIC_DRILL_PROVIDER_MODE === "provider_deferred"
      ? "provider_deferred"
      : "pagerduty";
    const body = {
      drill,
      cron: controller.cron,
      scheduled_at_ms: controller.scheduledTime,
      synthetic_page: {
        ...SYNTHETIC_PAGE_CONTRACT,
        region: syntheticRegionForWeek(week),
        rotation_week: ((week % 4) + 4) % 4,
        emit_at_ms: syntheticEmitAtMs(controller.scheduledTime, week),
        delivery_mode: ((week % 4) + 4) % 4 === 3 ? "deferred" : "immediate",
        provider_mode: providerMode,
        worker_revision: env.CF_VERSION_METADATA?.id ?? "",
        serving_sha: env.SENTRY_RELEASE ?? "",
        dedup_key: deliveryId,
        correlation_id: `PAT-CORRELATION-ID-001:${deliveryId}`,
      },
    };
    let response: Response;
    try {
      response = await delivery.fetch(`https://scheduled-drill.internal/v1/drills/${drill}`, {
        method: "POST",
        headers: {
          "content-type": "application/json",
          "x-corelink-scheduled-drill-id": deliveryId,
        },
        body: JSON.stringify(body),
      });
    } catch {
      // Do not include an exception message: service-binding errors can contain
      // endpoint/configuration details that do not belong in worker logs.
      console.error(`[scheduled_drill] failed drill=${drill} reason=delivery_exception`);
      throw new Error("scheduled drill delivery failed");
    }

    if (!response.ok) {
      // A non-2xx is a failed delivery. Throwing leaves Cloudflare's normal
      // scheduled retry behaviour intact; the idempotency key above makes that
      // safe for a receiver that accepted work before its response was lost.
      console.error(`[scheduled_drill] failed drill=${drill} reason=delivery_status`);
      throw new Error("scheduled drill delivery failed");
    }

    if (providerMode === "provider_deferred") {
      let receipt: unknown;
      try { receipt = await response.json(); } catch { receipt = null; }
      const expectedWorkerRevision = env.CF_VERSION_METADATA?.id;
      const expectedServingSha = env.SENTRY_RELEASE;
      const terminalReceipt = typeof receipt === "object" && receipt !== null
        ? receipt as Record<string, unknown>
        : null;
      if (terminalReceipt === null ||
          terminalReceipt["terminal"] !== true ||
          terminalReceipt["outcome"] !== "provider_deferred" ||
          terminalReceipt["receiver_result"] !== "persisted_provider_deferred" ||
          terminalReceipt["drill_id"] !== deliveryId ||
          terminalReceipt["correlation_id"] !== `PAT-CORRELATION-ID-001:${deliveryId}` ||
          terminalReceipt["scheduled_at_ms"] !== controller.scheduledTime ||
          terminalReceipt["worker_revision"] !== expectedWorkerRevision ||
          terminalReceipt["serving_sha"] !== expectedServingSha ||
          typeof terminalReceipt["receiver_worker_revision"] !== "string") {
        console.error(`[scheduled_drill] failed drill=${drill} reason=terminal_receipt_invalid`);
        throw new Error("scheduled drill terminal receipt invalid");
      }
    }

    if (providerMode === "provider_deferred") {
      console.info(`[scheduled_drill] completed drill=${drill} result=provider_deferred_terminal`);
    } else {
      console.info(`[scheduled_drill] delivered drill=${drill} result=accepted`);
    }
}
