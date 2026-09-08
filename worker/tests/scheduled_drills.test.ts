/**
 * Scheduled-drill routing contract. These tests exercise the exported Worker
 * handler so removing the entry point (or the internal delivery route) is red.
 */

import { readFileSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { ScheduledController } from "@cloudflare/workers-types";
import workerHandler from "../src/index.js";
import type { Env } from "../src/index.js";

const TEST_SCHEDULED_AT_MS = 1_785_844_800_000;
const TEST_DRILL_ID = `SP-${TEST_SCHEDULED_AT_MS}`;
const WORKTREE_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), "../..");

function controllerFor(cron: string): ScheduledController & { readonly noRetrySpy: ReturnType<typeof vi.fn> } {
  const noRetrySpy = vi.fn();
  return {
    cron,
    scheduledTime: TEST_SCHEDULED_AT_MS,
    noRetry: noRetrySpy,
    noRetrySpy,
  };
}

function envWithDelivery(fetch: typeof globalThis.fetch): Env {
  return { ENVIRONMENT: "dev", SCHEDULED_DRILL_DELIVERY: { fetch } } as Env;
}

function scheduledCtx(): ExecutionContext {
  return {
    waitUntil: (_promise: Promise<unknown>) => undefined,
    passThroughOnException: () => undefined,
  } as unknown as ExecutionContext;
}

afterEach(() => {
  vi.restoreAllMocks();
});

describe("scheduled drill configuration", () => {
  it("declares only the synthetic route in default/dev, never production", () => {
    const config = readFileSync(resolve(WORKTREE_ROOT, "wrangler.toml"), "utf8");
    const expectedRows = 'crons = [\n    "0 14 * * 1",\n]';

    expect(config.split(expectedRows)).toHaveLength(2);
    expect(config).not.toContain('"0 6 * * 1"');
    for (const environment of ["prod", "prod-sam", "prod-lhr", "prod-nrt", "prod-syd"]) {
      expect(config).toContain(`[env.${environment}.triggers]\ncrons = []`);
    }
  });
});

describe("scheduled drill delivery", () => {
  it("delivers the synthetic cron", async () => {
    const cron = "0 14 * * 1";
    const drill = "synthetic_page";
    const fetch = vi.fn<typeof globalThis.fetch>().mockResolvedValue(new Response(null, { status: 202 }));
    const controller = controllerFor(cron);
    const info = vi.spyOn(console, "info").mockImplementation(() => undefined);

    await expect(workerHandler.scheduled!(controller, envWithDelivery(fetch), scheduledCtx())).resolves.toBeUndefined();

    expect(fetch).toHaveBeenCalledOnce();
    const [url, init] = fetch.mock.calls[0] ?? [];
    expect(url).toBe(`https://scheduled-drill.internal/v1/drills/${drill}`);
    expect(init).toMatchObject({
      method: "POST",
      headers: {
        "content-type": "application/json",
        "x-corelink-scheduled-drill-id": TEST_DRILL_ID,
      },
      body: expect.any(String),
    });
    const body = JSON.parse(String((init as RequestInit).body)) as Record<string, unknown>;
    expect(body).toMatchObject({ drill, cron, scheduled_at_ms: TEST_SCHEDULED_AT_MS });
    if (drill === "synthetic_page") {
      expect(body.synthetic_page).toMatchObject({
        service: "synthetic-drill",
        event_action: "trigger",
        severity: "info",
        synthetic_severity: "sev2_synthetic",
        region: "americas",
        rotation_week: 0,
        emit_at_ms: TEST_SCHEDULED_AT_MS,
        delivery_mode: "immediate",
        dedup_key: TEST_DRILL_ID,
        correlation_id: `PAT-CORRELATION-ID-001:${TEST_DRILL_ID}`,
      });
    }
    expect(controller.noRetrySpy).not.toHaveBeenCalled();
    expect(info).toHaveBeenCalledWith(`[scheduled_drill] delivered drill=${drill} result=accepted`);
  });

  it("fails and telemeters a rejected downstream delivery", async () => {
    const fetch = vi.fn<typeof globalThis.fetch>().mockResolvedValue(new Response(null, { status: 503 }));
    const controller = controllerFor("0 14 * * 1");
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);

    await expect(workerHandler.scheduled!(controller, envWithDelivery(fetch), scheduledCtx()))
      .rejects.toThrow("scheduled drill delivery failed");

    expect(controller.noRetrySpy).not.toHaveBeenCalled();
    expect(error).toHaveBeenCalledWith("[scheduled_drill] failed drill=synthetic_page reason=delivery_status");
  });

  it("fails closed when the delivery binding is absent", async () => {
    const controller = controllerFor("0 14 * * 1");
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);

    await expect(workerHandler.scheduled!(controller, {} as Env, scheduledCtx()))
      .rejects.toThrow("scheduled drill delivery binding unavailable");

    expect(error).toHaveBeenCalledWith(
      "[scheduled_drill] failed drill=synthetic_page reason=delivery_binding_unavailable",
    );
    expect(controller.noRetrySpy).not.toHaveBeenCalled();
  });

  it("fails and preserves retry semantics when the binding throws", async () => {
    const fetch = vi.fn<typeof globalThis.fetch>().mockRejectedValue(new Error("transport"));
    const controller = controllerFor("0 14 * * 1");
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);

    await expect(workerHandler.scheduled!(controller, envWithDelivery(fetch), scheduledCtx()))
      .rejects.toThrow("scheduled drill delivery failed");

    expect(error).toHaveBeenCalledWith(
      "[scheduled_drill] failed drill=synthetic_page reason=delivery_exception",
    );
    expect(controller.noRetrySpy).not.toHaveBeenCalled();
  });

  it("rotates the PagerDuty drill region over the four-week cycle", async () => {
    const regions = ["americas", "emea", "apac", "boundary_handoff"] as const;
    for (const [week, region] of regions.entries()) {
      const scheduledAt = TEST_SCHEDULED_AT_MS + week * 7 * 24 * 60 * 60 * 1_000;
      const fetch = vi.fn<typeof globalThis.fetch>().mockResolvedValue(new Response(null, { status: 202 }));
      const controller = { ...controllerFor("0 14 * * 1"), scheduledTime: scheduledAt };

      await workerHandler.scheduled!(controller, envWithDelivery(fetch), scheduledCtx());
      const [, init] = fetch.mock.calls[0] ?? [];
      const body = JSON.parse(String((init as RequestInit).body)) as {
        synthetic_page?: {
          region?: string;
          rotation_week?: number;
          emit_at_ms?: number;
          delivery_mode?: string;
        };
      };
      expect(body.synthetic_page).toMatchObject({ region, rotation_week: week });
      if (week < 3) {
        expect(body.synthetic_page).toMatchObject({
          emit_at_ms: scheduledAt,
          delivery_mode: "immediate",
        });
      } else {
        const scheduled = new Date(scheduledAt);
        const expectedBoundary = Date.UTC(
          scheduled.getUTCFullYear(),
          scheduled.getUTCMonth(),
          scheduled.getUTCDate() + (7 - scheduled.getUTCDay()),
          23,
          59,
          0,
          0,
        );
        expect(body.synthetic_page?.emit_at_ms).toBe(expectedBoundary);
        expect(body.synthetic_page?.delivery_mode).toBe("deferred");
        const emitted = new Date(body.synthetic_page?.emit_at_ms ?? 0);
        expect(emitted.getUTCDay()).toBe(0);
        expect(emitted.getUTCHours()).toBe(23);
        expect(emitted.getUTCMinutes()).toBe(59);
        expect(emitted.getUTCSeconds()).toBe(0);
        expect(emitted.getUTCMilliseconds()).toBe(0);
      }
    }
  });

  it("fails closed and telemeters an undeclared cron without retrying it", async () => {
    const controller = controllerFor("* * * * *");
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);

    await expect(workerHandler.scheduled!(controller, envWithDelivery(vi.fn<typeof globalThis.fetch>()), scheduledCtx()))
      .rejects.toThrow("unknown scheduled drill");

    expect(controller.noRetrySpy).toHaveBeenCalledOnce();
    expect(error).toHaveBeenCalledWith("[scheduled_drill] rejected reason=unknown_cron");
  });
});
