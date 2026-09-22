/**
 * Unit tests for the DSR erasure queue consumer (WP-G).
 * Mocks the container call via env.CORELINK_API_SVC.fetch (no global fetch).
 */
import { describe, it, expect, vi } from "vitest";
import {
  processErasureMessage,
  handleErasureQueueBatch,
  handleErasureDlqBatch,
  type DsrConsumerEnv,
  type QueueMessageBatch,
  type QueueMessage,
  type DsrDlqBody,
  type DsrDlqReceipt,
  type DsrDlqReceiptStore,
} from "../src/webhooks/dsr_consumer.js";
import type { DsrQueuedV1 } from "../src/webhooks/clerk.js";

function msg(dsrId = "dsr-1"): DsrQueuedV1 {
  return {
    schema: "dev.hugr.corelink.dsr.queued.v1",
    dsr_id: dsrId,
    tenant_id: "tenant-x",
    subject_id: "tenant-x",
    erasure_salt_hex: "00".repeat(32),
    queued_at_ms: 1,
    legal_hold: false,
    source: "clerk.user.deleted",
    clerk_user_id: "user-x",
  };
}

function svc(status: number): { fetch: typeof fetch } {
  return {
    fetch: (async () =>
      new Response(status >= 400 ? "err" : "ok", { status })) as unknown as typeof fetch,
  };
}

describe("processErasureMessage", () => {
  it("ok on a 2xx from the container", async () => {
    const env: DsrConsumerEnv = {
      CORELINK_API_BASE: "https://api",
      CORELINK_INTERNAL_AUTH_KEY: "k",
      CORELINK_API_SVC: svc(200),
    };
    expect(await processErasureMessage(msg(), env)).toEqual({ ok: true, status: 200 });
  });

  it("not ok (→ retry) on a 5xx", async () => {
    const env: DsrConsumerEnv = {
      CORELINK_API_BASE: "https://api",
      CORELINK_INTERNAL_AUTH_KEY: "k",
      CORELINK_API_SVC: svc(500),
    };
    expect(await processErasureMessage(msg(), env)).toEqual({ ok: false, status: 500 });
  });

  it("not ok (→ retry) when CORELINK_INTERNAL_AUTH_KEY is absent — never drops an erasure", async () => {
    const env: DsrConsumerEnv = { CORELINK_API_BASE: "https://api", CORELINK_API_SVC: svc(200) };
    expect((await processErasureMessage(msg(), env)).ok).toBe(false);
  });

  it("posts to /_internal/dsr/erase with the internal-auth header + the message body", async () => {
    let captured: Request | undefined;
    const env: DsrConsumerEnv = {
      CORELINK_API_BASE: "https://api",
      CORELINK_INTERNAL_AUTH_KEY: "secret-key",
      CORELINK_API_SVC: {
        fetch: (async (req: Request) => {
          captured = req;
          return new Response("ok", { status: 200 });
        }) as unknown as typeof fetch,
      },
    };
    await processErasureMessage(msg("dsr-99"), env);
    expect(captured?.url).toBe("https://api/_internal/dsr/erase");
    expect(captured?.headers.get("x-corelink-internal-auth")).toBe("secret-key");
    const body = (await captured!.json()) as DsrQueuedV1;
    expect(body.dsr_id).toBe("dsr-99");
    expect(body.schema).toBe("dev.hugr.corelink.dsr.queued.v1");
  });

  it("rt-nuclear #23: sends the dedicated CORELINK_ERASE_AUTH_KEY when set (full-split config)", async () => {
    let captured: Request | undefined;
    const env: DsrConsumerEnv = {
      CORELINK_API_BASE: "https://api",
      CORELINK_ERASE_AUTH_KEY: "dedicated-erase-key",
      CORELINK_INTERNAL_AUTH_KEY: "shared-key",
      CORELINK_API_SVC: {
        fetch: (async (req: Request) => {
          captured = req;
          return new Response("ok", { status: 200 });
        }) as unknown as typeof fetch,
      },
    };
    await processErasureMessage(msg(), env);
    expect(captured?.headers.get("x-corelink-internal-auth")).toBe("dedicated-erase-key");
  });

  it("not ok (→ retry) on a transport throw", async () => {
    const env: DsrConsumerEnv = {
      CORELINK_API_BASE: "https://api",
      CORELINK_INTERNAL_AUTH_KEY: "k",
      CORELINK_API_SVC: {
        fetch: (async () => {
          throw new Error("boom");
        }) as unknown as typeof fetch,
      },
    };
    expect((await processErasureMessage(msg(), env)).ok).toBe(false);
  });
});

describe("handleErasureQueueBatch", () => {
  type MockQueueMessage<T> = QueueMessage<T> & {
    ack: ReturnType<typeof vi.fn<() => void>>;
    retry: ReturnType<typeof vi.fn<() => void>>;
  };

  function fakeMsg(body: DsrQueuedV1): MockQueueMessage<DsrQueuedV1> {
    return { body, ack: vi.fn<() => void>(), retry: vi.fn<() => void>() };
  }

  it("acks on 2xx, retries on failure, and isolates per-message", async () => {
    const m1 = fakeMsg(msg("ok-1"));
    const m2 = fakeMsg(msg("fail-2"));
    let call = 0;
    const env: DsrConsumerEnv = {
      CORELINK_API_BASE: "https://api",
      CORELINK_INTERNAL_AUTH_KEY: "k",
      CORELINK_API_SVC: {
        fetch: (async () => {
          call += 1;
          return new Response("x", { status: call === 1 ? 200 : 500 });
        }) as unknown as typeof fetch,
      },
    };
    const batch: QueueMessageBatch<DsrQueuedV1> = { messages: [m1, m2] };
    await handleErasureQueueBatch(batch, env);
    expect(m1.ack).toHaveBeenCalledOnce();
    expect(m1.retry).not.toHaveBeenCalled();
    expect(m2.retry).toHaveBeenCalledOnce();
    expect(m2.ack).not.toHaveBeenCalled();
  });
});

describe("handleErasureDlqBatch (bounded alert + requeue)", () => {
  type MockQueueMessage<T> = QueueMessage<T> & {
    ack: ReturnType<typeof vi.fn<() => void>>;
    retry: ReturnType<typeof vi.fn<() => void>>;
  };

  function fakeDlq(body: DsrDlqBody, attempts?: number): MockQueueMessage<DsrDlqBody> {
    return { body, attempts, ack: vi.fn<() => void>(), retry: vi.fn<() => void>() };
  }

  class MemoryReceipts implements DsrDlqReceiptStore {
    readonly rows = new Map<string, DsrDlqReceipt>();
    readonly writes: Array<{ eventId: string; status: string }> = [];
    failWrites = false;

    async find(eventId: string): Promise<DsrDlqReceipt | null> {
      return this.rows.get(eventId) ?? null;
    }

    async record(eventId: string, status: DsrDlqReceipt["status"]): Promise<void> {
      if (this.failWrites) throw new Error("d1 unavailable");
      this.writes.push({ eventId, status });
      const prior = this.rows.get(eventId);
      this.rows.set(eventId, {
        status,
        paging_claimed: status === "paging_retry" ? 0 : prior?.paging_claimed ?? 0,
        requeue_claimed: prior?.requeue_claimed ?? 0,
      });
    }

    async claimPaging(eventId: string): Promise<boolean> {
      const prior = this.rows.get(eventId);
      if (!prior || prior.paging_claimed) return false;
      this.rows.set(eventId, { ...prior, status: "paging_claimed", paging_claimed: 1 });
      return true;
    }

    async claimRequeue(eventId: string): Promise<boolean> {
      const prior = this.rows.get(eventId);
      if (!prior || prior.requeue_claimed) return false;
      this.rows.set(eventId, { ...prior, status: "requeue_claimed", requeue_claimed: 1 });
      return true;
    }
  }

  function pagedEnv(send: (message: unknown) => Promise<void>, receipts = new MemoryReceipts()) {
    return {
      DSR_QUEUE: { send },
      DSR_DLQ_RECEIPTS: receipts,
      PAGERDUTY_ROUTING_KEY: "routing-key",
      PAGERDUTY_FETCH: vi.fn<typeof fetch>(async () => new Response("accepted", { status: 202 })),
    };
  }

  it("emits a critical alert and requeues exactly once", async () => {
    const send = vi.fn<(message: unknown) => Promise<void>>(async () => undefined);
    const body: DsrDlqBody = {
      ...msg("dlq-1"),
      tenant_id: "tenant-private-canary",
      subject_id: "subject-private-canary",
      clerk_user_id: "user_private_canary",
      erasure_salt_hex: "ab".repeat(32),
    };
    const m = fakeDlq(body);
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    let errorCalls: unknown[][] = [];
    const env = pagedEnv(send);
    try {
      await handleErasureDlqBatch({ messages: [m] }, env);
    } finally {
      errorCalls = error.mock.calls;
      error.mockRestore();
    }
    expect(send).toHaveBeenCalledOnce();
    expect(send.mock.calls[0]?.[0]).toMatchObject({
      dsr_id: "dlq-1",
      tenant_id: "tenant-private-canary",
      subject_id: "subject-private-canary",
      clerk_user_id: "user_private_canary",
      erasure_salt_hex: "ab".repeat(32),
      _dlq_requeue: 1,
    });
    expect(m.ack).toHaveBeenCalledOnce();
    expect(m.retry).not.toHaveBeenCalled();
    expect(errorCalls).toHaveLength(1);
    const alert = JSON.parse(String(errorCalls[0]?.[0])) as Record<string, unknown>;
    expect(alert).toMatchObject({
      event: "dsr.erasure.dead_letter",
      exhausted: true,
      action: "requeue_once",
      requeue_count: 1,
      paging_status: "delivered",
      paging_configured: true,
    });
    expect(String(alert.event_id)).toMatch(/^dsr-erasure-dlq:[0-9a-f]{64}$/);
    const privateDsrFields = [
      "dlq-1",
      "tenant-private-canary",
      "subject-private-canary",
      "user_private_canary",
      "ab".repeat(32),
    ];
    for (const privateValue of privateDsrFields) {
      expect(JSON.stringify(alert)).not.toContain(privateValue);
      expect(JSON.stringify(env.PAGERDUTY_FETCH.mock.calls)).not.toContain(privateValue);
      expect(JSON.stringify(errorCalls)).not.toContain(privateValue);
    }
  });

  it("leaves an already requeued message dead and alerts without looping", async () => {
    const send = vi.fn<(message: unknown) => Promise<void>>(async () => undefined);
    const m = fakeDlq({ ...msg("dlq-2"), _dlq_requeue: 1 });
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    let errorCalls: unknown[][] = [];
    try {
      await handleErasureDlqBatch({ messages: [m] }, pagedEnv(send));
    } finally {
      errorCalls = error.mock.calls;
      error.mockRestore();
    }
    expect(send).not.toHaveBeenCalled();
    expect(m.ack).toHaveBeenCalledOnce();
    expect(m.retry).not.toHaveBeenCalled();
    expect(errorCalls).toHaveLength(1);
    const alert = JSON.parse(String(errorCalls[0]?.[0])) as Record<string, unknown>;
    expect(alert).toMatchObject({
      event: "dsr.erasure.dead_letter",
      severity: "critical",
      exhausted: true,
      requeue_count: 1,
      action: "left_dead",
      paging_status: "delivered",
      paging_configured: true,
    });
    expect(String(alert.event_id)).toMatch(/^dsr-erasure-dlq:[0-9a-f]{64}$/);
  });

  it("records an ambiguous requeue instead of issuing a duplicate send", async () => {
    const send = vi.fn<(message: unknown) => Promise<void>>(async () => { throw new Error("routing_key=secret-provider-detail"); });
    const m = fakeDlq(msg("dlq-3"));
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    try {
      await handleErasureDlqBatch({ messages: [m] }, pagedEnv(send));
      expect(String(error.mock.calls[0]?.[0])).not.toContain("secret-provider-detail");
      expect(JSON.parse(String(error.mock.calls[0]?.[0]))).toMatchObject({
        action: "requeue_ambiguous",
        requeue_error: "transport_error",
      });
    } finally {
      error.mockRestore();
    }
    expect(send).toHaveBeenCalledOnce();
    expect(m.ack).toHaveBeenCalledOnce();
    expect(m.retry).not.toHaveBeenCalled();
  });

  it("uses a durable receipt to prevent duplicate page and requeue side effects", async () => {
    const send = vi.fn<(message: unknown) => Promise<void>>(async () => undefined);
    const first = fakeDlq(msg("stable-id"));
    const second = fakeDlq(msg("stable-id"));
    const receipts = new MemoryReceipts();
    const env = pagedEnv(send, receipts);
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    try {
      await handleErasureDlqBatch({ messages: [first] }, env);
      await handleErasureDlqBatch({ messages: [second] }, env);
    } finally {
      const alerts = error.mock.calls.map((call) => JSON.parse(String(call[0])) as Record<string, unknown>);
      error.mockRestore();
      expect(alerts).toHaveLength(1);
    }
    expect(send).toHaveBeenCalledOnce();
    expect(env.PAGERDUTY_FETCH).toHaveBeenCalledOnce();
    expect(second.ack).toHaveBeenCalledOnce();
  });

  it("does not page twice after a crash between the durable paging claim and provider receipt", async () => {
    const send = vi.fn<(message: unknown) => Promise<void>>(async () => undefined);
    const receipts = new MemoryReceipts();
    const first = fakeDlq(msg("paging-claim-crash"));
    const second = fakeDlq(msg("paging-claim-crash"));
    let calls = 0;
    const pagerFetch = vi.fn<typeof fetch>(async () => {
      calls += 1;
      if (calls === 1) throw new Error("provider timeout after send");
      return new Response("accepted", { status: 202 });
    });
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    try {
      await handleErasureDlqBatch(
        { messages: [first] },
        { DSR_QUEUE: { send }, DSR_DLQ_RECEIPTS: receipts, PAGERDUTY_ROUTING_KEY: "routing-key", PAGERDUTY_FETCH: pagerFetch },
      );
      await handleErasureDlqBatch(
        { messages: [second] },
        { DSR_QUEUE: { send }, DSR_DLQ_RECEIPTS: receipts, PAGERDUTY_ROUTING_KEY: "routing-key", PAGERDUTY_FETCH: pagerFetch },
      );
    } finally {
      error.mockRestore();
    }
    expect(pagerFetch).toHaveBeenCalledOnce();
    expect(send).not.toHaveBeenCalled();
    expect(second.ack).toHaveBeenCalledOnce();
    expect([...receipts.rows.values()][0]?.status).toBe("paging_ambiguous");
  });

  it("records an ambiguous operator disposition when PagerDuty transport outcome is unknown", async () => {
    const send = vi.fn<(message: unknown) => Promise<void>>(async () => undefined);
    const m = fakeDlq(msg("pager-fail"));
    const pagerFetch = vi.fn<typeof fetch>(async () => {
      throw new Error("routing_key=secret-provider-detail");
    });
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    try {
      await handleErasureDlqBatch(
        { messages: [m] },
        {
          DSR_QUEUE: { send },
          DSR_DLQ_RECEIPTS: new MemoryReceipts(),
          PAGERDUTY_ROUTING_KEY: "routing-key",
          PAGERDUTY_FETCH: pagerFetch,
        },
      );
      const alert = JSON.parse(String(error.mock.calls[0]?.[0])) as Record<string, unknown>;
      expect(alert).toMatchObject({
        exhausted: true,
        requeue_count: 0,
        action: "paging_ambiguous",
        paging_status: "failed",
        paging_error: "transport_error",
      });
      expect(String(alert.event_id)).toMatch(/^dsr-erasure-dlq:[0-9a-f]{64}$/);
      expect(String(error.mock.calls[0]?.[0])).not.toContain("secret-provider-detail");
    } finally {
      error.mockRestore();
    }
    expect(pagerFetch).toHaveBeenCalledOnce();
    expect(send).not.toHaveBeenCalled();
    expect(m.ack).toHaveBeenCalledOnce();
    expect(m.retry).not.toHaveBeenCalled();
  });

  it("fails closed when the PagerDuty route is absent", async () => {
    const send = vi.fn<(message: unknown) => Promise<void>>(async () => undefined);
    const m = fakeDlq(msg("pager-unconfigured"));
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    try {
      await handleErasureDlqBatch({ messages: [m] }, { DSR_QUEUE: { send }, DSR_DLQ_RECEIPTS: new MemoryReceipts() });
      expect(JSON.parse(String(error.mock.calls[0]?.[0]))).toMatchObject({
        action: "retry_paging",
        paging_status: "not_configured",
        paging_error: "route_not_configured",
      });
      expect(String((JSON.parse(String(error.mock.calls[0]?.[0])) as Record<string, unknown>).event_id)).toMatch(/^dsr-erasure-dlq:[0-9a-f]{64}$/);
    } finally {
      error.mockRestore();
    }
    expect(send).not.toHaveBeenCalled();
    expect(m.retry).toHaveBeenCalledOnce();
    expect(m.ack).not.toHaveBeenCalled();
  });

  it("does not treat a generic HTTP 200 as PagerDuty delivery evidence", async () => {
    const send = vi.fn<(message: unknown) => Promise<void>>(async () => undefined);
    const m = fakeDlq(msg("pager-false-2xx"));
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    try {
      await handleErasureDlqBatch(
        { messages: [m] },
        {
          DSR_QUEUE: { send },
          DSR_DLQ_RECEIPTS: new MemoryReceipts(),
          PAGERDUTY_ROUTING_KEY: "routing-key",
          PAGERDUTY_FETCH: vi.fn<typeof fetch>(async () => new Response("proxy page", { status: 200 })),
        },
      );
      expect(JSON.parse(String(error.mock.calls[0]?.[0]))).toMatchObject({
        action: "retry_paging",
        paging_status: "failed",
        paging_error: "http_rejected",
      });
    } finally {
      error.mockRestore();
    }
    expect(send).not.toHaveBeenCalled();
    expect(m.retry).toHaveBeenCalledOnce();
    expect(m.ack).not.toHaveBeenCalled();
  });

  it("uses the canonical PagerDuty Events API envelope and dedup key", async () => {
    const send = vi.fn<(message: unknown) => Promise<void>>(async () => undefined);
    const m = fakeDlq(msg("pager-ok"));
    let request: RequestInfo | URL | undefined;
    let init: RequestInit | undefined;
    const pagerFetch = vi.fn<typeof fetch>(async (input, options) => {
      request = input;
      init = options;
      return new Response("accepted", { status: 202 });
    });
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    try {
      await handleErasureDlqBatch(
        { messages: [m] },
        {
          DSR_QUEUE: { send },
          DSR_DLQ_RECEIPTS: new MemoryReceipts(),
          PAGERDUTY_ROUTING_KEY: "routing-key",
          PAGERDUTY_FETCH: pagerFetch,
        },
      );
    } finally {
      error.mockRestore();
    }
    expect(request).toBe("https://events.pagerduty.com/v2/enqueue");
    const payload = JSON.parse(String(init?.body)) as Record<string, any>;
    expect(payload).toMatchObject({
      routing_key: "routing-key",
      event_action: "trigger",
      payload: {
        severity: "critical",
        custom_details: {
          exhausted: true,
          requeue_count: 0,
        },
      },
    });
    expect(String(payload.dedup_key)).toMatch(/^dsr-erasure-dlq:[0-9a-f]{64}$/);
    expect(String(payload.payload.custom_details.event_id)).toBe(payload.dedup_key);
    expect(JSON.stringify(payload)).not.toContain("pager-ok");
    expect(m.ack).toHaveBeenCalledOnce();
    expect(m.retry).not.toHaveBeenCalled();
  });

  it("does not page or acknowledge when durable receipt storage is unavailable", async () => {
    const send = vi.fn<(message: unknown) => Promise<void>>(async () => undefined);
    const m = fakeDlq(msg("receipt-unavailable"));
    const receipts = new MemoryReceipts();
    receipts.failWrites = true;
    const env = pagedEnv(send, receipts);
    await handleErasureDlqBatch({ messages: [m] }, env);
    expect(env.PAGERDUTY_FETCH).not.toHaveBeenCalled();
    expect(send).not.toHaveBeenCalled();
    expect(m.retry).toHaveBeenCalledOnce();
    expect(m.ack).not.toHaveBeenCalled();
  });

  for (const [name, makeReceipts, receiptBoundary] of [
    ["is absent", () => undefined, "not_configured"],
    ["cannot write", () => {
      const receipts = new MemoryReceipts();
      receipts.failWrites = true;
      return receipts;
    }, "storage_error"],
    ["is malformed", () => ({
      find: async () => ({ status: "invalid" as DsrDlqReceipt["status"], paging_claimed: 0, requeue_claimed: 0 }),
      record: async () => undefined,
      claimPaging: async () => true,
      claimRequeue: async () => true,
    } satisfies DsrDlqReceiptStore), "malformed"],
  ] as const) {
    it(`emits a privacy-safe terminal alert when the receipt boundary ${name}`, async () => {
      const send = vi.fn<(message: unknown) => Promise<void>>(async () => undefined);
      const m = fakeDlq(msg("receipt-terminal"), 3);
      const pagerFetch = vi.fn<typeof fetch>(async () => new Response("accepted", { status: 202 }));
      const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
      try {
        await handleErasureDlqBatch(
          { messages: [m] },
          {
            DSR_QUEUE: { send }, DSR_DLQ_RECEIPTS: makeReceipts(),
            PAGERDUTY_ROUTING_KEY: "routing-key", PAGERDUTY_FETCH: pagerFetch,
          },
        );
        const alert = JSON.parse(String(error.mock.calls[0]?.[0])) as Record<string, unknown>;
        expect(alert).toMatchObject({
          action: "receipt_boundary_exhausted", receipt_boundary: receiptBoundary,
          exhausted: true, requeue_count: 0,
        });
        expect(String(alert.event_id)).toMatch(/^dsr-erasure-dlq:[0-9a-f]{64}$/);
        expect(JSON.stringify(alert)).not.toContain("receipt-terminal");
      } finally {
        error.mockRestore();
      }
      expect(pagerFetch).not.toHaveBeenCalled();
      expect(send).not.toHaveBeenCalled();
      expect(m.ack).toHaveBeenCalledOnce();
      expect(m.retry).not.toHaveBeenCalled();
    });
  }

  it("fails closed when a durable receipt row is malformed", async () => {
    const send = vi.fn<(message: unknown) => Promise<void>>(async () => undefined);
    const m = fakeDlq(msg("receipt-malformed"));
    const receipts: DsrDlqReceiptStore = {
      find: async () => ({ status: "invalid" as DsrDlqReceipt["status"], paging_claimed: 0, requeue_claimed: 0 }),
      record: async () => undefined,
      claimPaging: async () => true,
      claimRequeue: async () => true,
    };
    const env = pagedEnv(send, receipts);
    await handleErasureDlqBatch({ messages: [m] }, env);
    expect(env.PAGERDUTY_FETCH).not.toHaveBeenCalled();
    expect(send).not.toHaveBeenCalled();
    expect(m.retry).toHaveBeenCalledOnce();
    expect(m.ack).not.toHaveBeenCalled();
  });

  it("writes a durable exhaustion receipt before consuming the final paging attempt", async () => {
    const send = vi.fn<(message: unknown) => Promise<void>>(async () => undefined);
    const m = fakeDlq(msg("paging-exhausted"), 3);
    const receipts = new MemoryReceipts();
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    try {
      await handleErasureDlqBatch(
        { messages: [m] },
        {
          DSR_QUEUE: { send },
          DSR_DLQ_RECEIPTS: receipts,
          PAGERDUTY_ROUTING_KEY: "routing-key",
          PAGERDUTY_FETCH: vi.fn<typeof fetch>(async () => new Response("unavailable", { status: 503 })),
        },
      );
    } finally {
      error.mockRestore();
    }
    expect(receipts.writes.at(-1)?.status).toBe("delivery_exhausted");
    expect(m.ack).toHaveBeenCalledOnce();
    expect(m.retry).not.toHaveBeenCalled();
  });

  it("records a paging recovery after a retained delivery later receives HTTP 202", async () => {
    const send = vi.fn<(message: unknown) => Promise<void>>(async () => undefined);
    const receipts = new MemoryReceipts();
    const failed = fakeDlq(msg("paging-recovery"), 1);
    const recovered = fakeDlq(msg("paging-recovery"), 2);
    const reject = vi.fn<typeof fetch>(async () => new Response("unavailable", { status: 503 }));
    const accepted = vi.fn<typeof fetch>(async () => new Response("accepted", { status: 202 }));
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    try {
      await handleErasureDlqBatch(
        { messages: [failed] },
        { DSR_QUEUE: { send }, DSR_DLQ_RECEIPTS: receipts, PAGERDUTY_ROUTING_KEY: "routing-key", PAGERDUTY_FETCH: reject },
      );
      await handleErasureDlqBatch(
        { messages: [recovered] },
        { DSR_QUEUE: { send }, DSR_DLQ_RECEIPTS: receipts, PAGERDUTY_ROUTING_KEY: "routing-key", PAGERDUTY_FETCH: accepted },
      );
    } finally {
      error.mockRestore();
    }
    expect(failed.retry).toHaveBeenCalledOnce();
    expect(recovered.ack).toHaveBeenCalledOnce();
    expect(receipts.writes.some((entry) => entry.status === "paging_recovered")).toBe(true);
  });

  it("fails closed on malformed or oversized requeue markers instead of looping", async () => {
    const send = vi.fn<(message: unknown) => Promise<void>>(async () => undefined);
    const m = fakeDlq({ ...msg("poison-marker"), _dlq_requeue: Number.NaN });
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    try {
      await handleErasureDlqBatch({ messages: [m] }, pagedEnv(send));
      const alert = JSON.parse(String(error.mock.calls[0]?.[0])) as Record<string, unknown>;
      expect(alert).toMatchObject({
        exhausted: true,
        action: "left_dead",
        requeue_count: 1,
      });
      expect(String(alert.event_id)).toMatch(/^dsr-erasure-dlq:[0-9a-f]{64}$/);
    } finally {
      error.mockRestore();
    }
    expect(send).not.toHaveBeenCalled();
    const alreadyCapped = fakeDlq({ ...msg("oversized-marker"), _dlq_requeue: 99 });
    await handleErasureDlqBatch({ messages: [alreadyCapped] }, pagedEnv(send));
    expect(alreadyCapped.ack).toHaveBeenCalledOnce();
  });
});
