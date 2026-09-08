import { afterEach, describe, expect, it, vi } from "vitest";
import handler from "../src/index.js";
import {
  buildPagerDutyEvent,
  parseSyntheticPageEnvelope,
  validateReceiverEnvironment,
  type ReceiverEnv,
} from "../src/contract.js";

const dedupKey = "synthetic_page:0 14 * * 1:1785844800000";
const envelope = {
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
    dedup_key: dedupKey,
    correlation_id: `PAT-CORRELATION-ID-001:${dedupKey}`,
  },
} as const;

function fakeDb() {
  const run = vi.fn().mockResolvedValue({ success: true });
  const bind = vi.fn().mockReturnValue({ run });
  const prepare = vi.fn().mockReturnValue({ bind });
  return { db: { prepare } as unknown as D1Database, prepare, bind, run };
}

function env(overrides: Partial<ReceiverEnv> = {}) {
  const database = fakeDb();
  return {
    value: {
      ENVIRONMENT: "staging",
      SYNTHETIC_DRILL_ENABLED: "true",
      PAGERDUTY_EVENTS_URL: "https://events.pagerduty.com/v2/enqueue",
      PAGERDUTY_SERVICE: "synthetic-drill",
      PAGERDUTY_SYNTHETIC_ROUTING_KEY: "test-routing-key",
      CONFIG_DB: database.db,
      ...overrides,
    } satisfies ReceiverEnv,
    database,
  };
}

function request(body: unknown): Request {
  return new Request("https://synthetic.example/v1/drills/synthetic_page", {
    method: "POST",
    headers: {
      "content-type": "application/json",
      "x-corelink-scheduled-drill-id": dedupKey,
    },
    body: JSON.stringify(body),
  });
}

afterEach(() => vi.unstubAllGlobals());

describe("synthetic receiver contract", () => {
  it("accepts the scheduler envelope and rejects a correlation mismatch", () => {
    expect(parseSyntheticPageEnvelope(envelope)?.synthetic_page.correlation_id).toBe(
      `PAT-CORRELATION-ID-001:${dedupKey}`,
    );
    expect(
      parseSyntheticPageEnvelope({
        ...envelope,
        synthetic_page: { ...envelope.synthetic_page, correlation_id: "wrong" },
      }),
    ).toBeNull();
  });

  it("builds the same PagerDuty dedup/correlation event every time", () => {
    const page = parseSyntheticPageEnvelope(envelope)!.synthetic_page;
    expect(buildPagerDutyEvent(page, "routing-key")).toEqual(buildPagerDutyEvent(page, "routing-key"));
    expect(buildPagerDutyEvent(page, "routing-key")).toMatchObject({
      event_action: "trigger",
      dedup_key: dedupKey,
      payload: { custom_details: { correlation_id: `PAT-CORRELATION-ID-001:${dedupKey}` } },
    });
  });

  it.each(["prod", "prod-sam", "prod-lhr", "prod-nrt", "prod-syd"])(
    "rejects %s before any external delivery",
    (environment) => {
      expect(validateReceiverEnvironment(env({ ENVIRONMENT: environment }).value)).toContain("production");
    },
  );

  it("retries a failed PagerDuty delivery with the same dedup key", async () => {
    const receiver = env();
    const pagerDutyFetch = vi
      .fn<typeof fetch>()
      .mockResolvedValueOnce(new Response(null, { status: 503 }))
      .mockResolvedValueOnce(new Response(null, { status: 202 }));
    vi.stubGlobal("fetch", pagerDutyFetch);

    const first = await handler.fetch(request(envelope), receiver.value, {} as ExecutionContext);
    const second = await handler.fetch(request(envelope), receiver.value, {} as ExecutionContext);
    expect(first.status).toBe(502);
    expect(second.status).toBe(202);
    expect(pagerDutyFetch).toHaveBeenCalledTimes(2);
    expect(JSON.parse(String(pagerDutyFetch.mock.calls[0]?.[1]?.body))).toMatchObject({ dedup_key: dedupKey });
    expect(pagerDutyFetch.mock.calls[0]?.[1]?.body).toBe(pagerDutyFetch.mock.calls[1]?.[1]?.body);
  });

  it("does not page when production is supplied by mistake", async () => {
    const receiver = env({ ENVIRONMENT: "prod", SYNTHETIC_DRILL_ENABLED: "true" });
    const pagerDutyFetch = vi.fn<typeof fetch>();
    vi.stubGlobal("fetch", pagerDutyFetch);
    const response = await handler.fetch(request(envelope), receiver.value, {} as ExecutionContext);
    expect(response.status).toBe(503);
    expect(pagerDutyFetch).not.toHaveBeenCalled();
    expect(receiver.database.prepare).not.toHaveBeenCalled();
  });
});
