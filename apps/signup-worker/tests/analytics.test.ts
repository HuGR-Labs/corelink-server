import { describe, it, expect } from "vitest";
import {
  recordCasEvent,
  recordFirstCliAuthed,
  type KvLike,
  type AnalyticsSink,
} from "../../cas-worker/src/middleware/analytics.js";

function fakeKv(): KvLike & { dump: () => Record<string, string> } {
  const store: Record<string, string> = {};
  return {
    async get(k) {
      return store[k] ?? null;
    },
    async put(k, v) {
      store[k] = v;
    },
    dump: () => ({ ...store }),
  };
}

function fakeSink(): AnalyticsSink & {
  events: Array<{ name: string; tenant: string; props: Record<string, unknown> }>;
} {
  const events: Array<{
    name: string;
    tenant: string;
    props: Record<string, unknown>;
  }> = [];
  return {
    events,
    async emit(name, tenant, props) {
      events.push({ name, tenant, props });
    },
  };
}

describe("recordCasEvent — first_cache_hit", () => {
  const baseCtx = {
    tenantId: "t_1",
    patId: "pat_1",
    contentHash: "blake3:deadbeef",
    occurredAt: new Date("2026-05-27T12:00:00Z"),
  } as const;

  it("does not fire on read_miss or write", async () => {
    const kv = fakeKv();
    const sink = fakeSink();
    expect(
      await recordCasEvent({
        ctx: { ...baseCtx, kind: "read_miss" },
        kv,
        analytics: sink,
      }),
    ).toBe(false);
    expect(
      await recordCasEvent({
        ctx: { ...baseCtx, kind: "write" },
        kv,
        analytics: sink,
      }),
    ).toBe(false);
    expect(sink.events).toHaveLength(0);
  });

  it("does not fire on the FIRST read_hit, fires on the SECOND", async () => {
    const kv = fakeKv();
    const sink = fakeSink();
    const fired1 = await recordCasEvent({
      ctx: { ...baseCtx, kind: "read_hit" },
      kv,
      analytics: sink,
    });
    expect(fired1).toBe(false);
    expect(sink.events).toHaveLength(0);

    const fired2 = await recordCasEvent({
      ctx: { ...baseCtx, kind: "read_hit" },
      kv,
      analytics: sink,
    });
    expect(fired2).toBe(true);
    expect(sink.events).toHaveLength(1);
    expect(sink.events[0]!.name).toBe("first_cache_hit");
    expect(sink.events[0]!.tenant).toBe("t_1");
    expect(sink.events[0]!.props["content_hash"]).toBe("blake3:deadbeef");
  });

  it("is idempotent per tenant — never fires twice", async () => {
    const kv = fakeKv();
    const sink = fakeSink();
    await recordCasEvent({ ctx: { ...baseCtx, kind: "read_hit" }, kv, analytics: sink });
    await recordCasEvent({ ctx: { ...baseCtx, kind: "read_hit" }, kv, analytics: sink });
    // Different content_hash, also a 2nd hit — should NOT re-fire activation.
    await recordCasEvent({
      ctx: { ...baseCtx, contentHash: "blake3:cafef00d", kind: "read_hit" },
      kv,
      analytics: sink,
    });
    await recordCasEvent({
      ctx: { ...baseCtx, contentHash: "blake3:cafef00d", kind: "read_hit" },
      kv,
      analytics: sink,
    });
    expect(sink.events.filter((e) => e.name === "first_cache_hit")).toHaveLength(1);
  });
});

describe("recordFirstCliAuthed", () => {
  it("fires once and only once per tenant", async () => {
    const kv = fakeKv();
    const sink = fakeSink();
    const fired1 = await recordFirstCliAuthed({
      tenantId: "t_1",
      patId: "pat_1",
      cliVersion: "0.1.0",
      kv,
      analytics: sink,
    });
    const fired2 = await recordFirstCliAuthed({
      tenantId: "t_1",
      patId: "pat_1",
      cliVersion: "0.1.0",
      kv,
      analytics: sink,
    });
    expect(fired1).toBe(true);
    expect(fired2).toBe(false);
    expect(sink.events).toHaveLength(1);
    expect(sink.events[0]!.name).toBe("first_cli_authed");
  });
});
