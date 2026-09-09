import { afterEach, describe, expect, it, vi } from "vitest";
import handler, { runDeferredDeliveries } from "../src/index.js";
import {
  buildPagerDutyEvent,
  parseSyntheticPageEnvelope,
  validateReceiverEnvironment,
  type ReceiverEnv,
} from "../src/contract.js";

const drillId = "SP-1785844800000";
const correlation = `PAT-CORRELATION-ID-001:${drillId}`;
const immediateEnvelope = {
  drill: "synthetic_page",
  cron: "0 14 * * 1",
  scheduled_at_ms: 1_785_844_800_000,
  synthetic_page: {
    service: "synthetic-drill",
    event_action: "trigger",
    severity: "info",
    synthetic_severity: "sev2_synthetic",
    region: "americas",
    rotation_week: 0,
    emit_at_ms: 1_785_844_800_000,
    delivery_mode: "immediate",
    dedup_key: drillId,
    correlation_id: correlation,
  },
} as const;
const deferredEnvelope = {
  ...immediateEnvelope,
  synthetic_page: {
    ...immediateEnvelope.synthetic_page,
    region: "boundary_handoff",
    rotation_week: 3,
    emit_at_ms: 1_785_905_940_000,
    delivery_mode: "deferred",
  },
} as const;

function fakeDb(options: { row?: unknown; rows?: unknown[]; failBatch?: boolean } = {}) {
  const calls: string[] = [];
  const run = vi.fn().mockResolvedValue({ success: true });
  const batch = vi.fn().mockImplementation(async () => {
    calls.push("batch");
    if (options.failBatch) throw new Error("storage unavailable");
    for (const row of options.rows ?? []) {
      if (typeof row === "object" && row !== null && "delivery_mode" in row && "delivered_at_ms" in row) {
        (row as { delivered_at_ms: number | null }).delivered_at_ms = 1_785_905_940_000;
      }
    }
    return [];
  });
  const prepare = vi.fn().mockImplementation((sql: string) => {
    calls.push(`prepare:${sql.slice(0, 24)}`);
    return {
      bind: vi.fn().mockImplementation(() => ({
        all: vi.fn().mockResolvedValue({
          results: (options.rows ?? []).filter((row) =>
            typeof row === "object" && row !== null && "delivery_mode" in row && "delivered_at_ms" in row
              ? (row as { delivery_mode: string; delivered_at_ms: number | null }).delivery_mode === "deferred" &&
                (row as { delivered_at_ms: number | null }).delivered_at_ms === null
              : true,
          ),
        }),
        first: vi.fn().mockResolvedValue(options.row ?? null),
        run,
      })),
    };
  });
  return { db: { prepare, batch } as unknown as D1Database, prepare, batch, run, calls };
}

function env(overrides: Partial<ReceiverEnv> = {}, dbOptions: Parameters<typeof fakeDb>[0] = {}) {
  const database = fakeDb(dbOptions);
  return {
    value: {
      ENVIRONMENT: "staging",
      SYNTHETIC_DRILL_ENABLED: "true",
      PAGERDUTY_EVENTS_URL: "https://events.pagerduty.com/v2/enqueue",
      PAGERDUTY_SERVICE: "synthetic-drill",
      PAGERDUTY_SYNTHETIC_ROUTING_KEY: "test-routing-key",
      PAGERDUTY_WEBHOOK_SECRET: "webhook-secret",
      CONFIG_DB: database.db,
      ...overrides,
    } satisfies ReceiverEnv,
    database,
  };
}

function request(body: unknown, path = "/v1/drills/synthetic_page"): Request {
  const dedupKey = (body as { synthetic_page: { dedup_key: string } }).synthetic_page.dedup_key;
  return new Request(`https://synthetic.example${path}`, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      "x-corelink-scheduled-drill-id": dedupKey,
    },
    body: JSON.stringify(body),
  });
}

const workerFetch = handler.fetch!;
async function invoke(requestValue: Request, environment: ReceiverEnv): Promise<Response> {
  return workerFetch(requestValue as Parameters<typeof workerFetch>[0], environment, {} as ExecutionContext);
}

async function signature(body: string, secret: string): Promise<string> {
  const key = await crypto.subtle.importKey("raw", new TextEncoder().encode(secret), { name: "HMAC", hash: "SHA-256" }, false, ["sign"]);
  const mac = new Uint8Array(await crypto.subtle.sign("HMAC", key, new TextEncoder().encode(body)));
  return `v1=${[...mac].map((byte) => byte.toString(16).padStart(2, "0")).join("")}`;
}

afterEach(() => vi.unstubAllGlobals());

describe("synthetic receiver contract", () => {
  it("uses one canonical drill id for PagerDuty dedup and correlation", () => {
    const page = parseSyntheticPageEnvelope(immediateEnvelope)!.synthetic_page;
    expect(page.dedup_key).toBe(drillId);
    expect(buildPagerDutyEvent(page, "routing-key")).toMatchObject({
      event_action: "trigger",
      dedup_key: drillId,
      payload: { custom_details: { correlation_id: correlation } },
    });
    expect(parseSyntheticPageEnvelope({
      ...immediateEnvelope,
      synthetic_page: { ...immediateEnvelope.synthetic_page, correlation_id: "wrong" },
    })).toBeNull();
  });

  it.each(["prod", "prod-sam", "prod-lhr", "prod-nrt", "prod-syd"])(
    "rejects %s before any external delivery",
    (environment) => {
      expect(validateReceiverEnvironment(env({ ENVIRONMENT: environment }).value)).toContain("production");
    },
  );

  it("commits D1 and audit before PagerDuty fetch", async () => {
    const receiver = env();
    const order: string[] = [];
    const pagerDutyFetch = vi.fn<typeof fetch>().mockImplementation(async () => {
      order.push("pagerduty");
      return new Response(null, { status: 202 });
    });
    receiver.database.batch.mockImplementationOnce(async () => {
      order.push("d1");
      return [];
    });
    vi.stubGlobal("fetch", pagerDutyFetch);
    expect((await invoke(request(immediateEnvelope), receiver.value)).status).toBe(202);
    expect(order).toEqual(["d1", "pagerduty"]);
  });

  it("suppresses PagerDuty when D1 storage fails", async () => {
    const receiver = env({}, { failBatch: true });
    const pagerDutyFetch = vi.fn<typeof fetch>();
    vi.stubGlobal("fetch", pagerDutyFetch);
    expect((await invoke(request(immediateEnvelope), receiver.value)).status).toBe(503);
    expect(pagerDutyFetch).not.toHaveBeenCalled();
  });

  it("persists the deferred handoff and returns 202 without dropping it", async () => {
    const receiver = env();
    const pagerDutyFetch = vi.fn<typeof fetch>();
    vi.stubGlobal("fetch", pagerDutyFetch);
    const response = await invoke(request(deferredEnvelope), receiver.value);
    expect(response.status).toBe(202);
    expect(JSON.parse(await response.text())).toMatchObject({ deferred: true, dedup_key: drillId });
    expect(receiver.database.batch).toHaveBeenCalledOnce();
    expect(pagerDutyFetch).not.toHaveBeenCalled();
  });

  it("executes deferred delivery and retries with identical dedup/correlation", async () => {
    const receiver = env({}, {
      rows: [{
        drill_id: drillId,
        region: "boundary_handoff",
        emit_ts_ms: 1_785_905_940_000,
        scheduled_at_ms: 1_785_844_800_000,
        correlation_id: correlation,
        delivery_mode: "deferred",
        delivered_at_ms: null,
      }],
    });
    const pagerDutyFetch = vi.fn<typeof fetch>()
      .mockResolvedValueOnce(new Response(null, { status: 503 }))
      .mockResolvedValueOnce(new Response(null, { status: 202 }));
    const controller = { cron: "59 23 * * 0", scheduledTime: 1_785_905_940_000, noRetry: vi.fn() } as ScheduledController;
    await expect(runDeferredDeliveries(controller, receiver.value, pagerDutyFetch)).rejects.toThrow("PagerDuty delivery failed");
    await expect(runDeferredDeliveries(controller, receiver.value, pagerDutyFetch)).resolves.toBeUndefined();
    await expect(runDeferredDeliveries(controller, receiver.value, pagerDutyFetch)).resolves.toBeUndefined();
    expect(pagerDutyFetch).toHaveBeenCalledTimes(2);
    expect(pagerDutyFetch.mock.calls[0]?.[1]?.body).toBe(pagerDutyFetch.mock.calls[1]?.[1]?.body);
    expect(receiver.database.batch).toHaveBeenCalledOnce();
  });

  it("requires an authenticated PagerDuty acknowledgment and records MTTA", async () => {
    const body = JSON.stringify({
      id: "pd-ack-001",
      event_type: "incident.acknowledged",
      occurred_at: "2026-09-08T14:01:00.000Z",
      data: { incident: { incident_key: drillId, assignments: [{ assignee: { summary: "eng-001" } }] } },
    });
    const receiver = env({}, {
      row: { drill_id: drillId, emit_ts_ms: 1_785_844_800_000, outcome: "unacked", correlation_id: correlation },
    });
    const bad = await invoke(new Request("https://synthetic.example/v1/webhooks/pagerduty", { method: "POST", body }), receiver.value);
    expect(bad.status).toBe(401);
    const good = await invoke(new Request("https://synthetic.example/v1/webhooks/pagerduty", {
      method: "POST",
      headers: { "x-pagerduty-signature": await signature(body, "webhook-secret") },
      body,
    }), receiver.value);
    expect(good.status).toBe(202);
    expect(receiver.database.batch).toHaveBeenCalledOnce();
    expect(receiver.database.prepare).toHaveBeenCalledTimes(3);
  });

  it("records an authenticated escalation as terminal escalation, not an ack", async () => {
    const body = JSON.stringify({
      id: "pd-escalation-001",
      event_type: "incident.escalated",
      occurred_at: "2026-09-08T14:02:00.000Z",
      data: { incident: { incident_key: drillId, service: { summary: "synthetic-drill" } } },
    });
    const receiver = env({}, {
      row: { drill_id: drillId, emit_ts_ms: 1_785_844_800_000, outcome: "unacked", correlation_id: correlation },
    });
    const response = await invoke(new Request("https://synthetic.example/v1/webhooks/pagerduty", {
      method: "POST",
      headers: { "x-pagerduty-signature": await signature(body, "webhook-secret") },
      body,
    }), receiver.value);
    expect(response.status).toBe(202);
    expect(JSON.parse(await response.text())).toMatchObject({ outcome: "escalated", drill_id: drillId });
    expect(receiver.database.batch).toHaveBeenCalledOnce();
  });
});
