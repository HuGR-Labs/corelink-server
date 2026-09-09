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

  function fakeDlq(body: DsrDlqBody): MockQueueMessage<DsrDlqBody> {
    return { body, ack: vi.fn<() => void>(), retry: vi.fn<() => void>() };
  }

  it("emits a critical alert and requeues exactly once", async () => {
    const send = vi.fn<(message: unknown) => Promise<void>>(async () => undefined);
    const m = fakeDlq(msg("dlq-1"));
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    let errorCalls: unknown[][] = [];
    try {
      await handleErasureDlqBatch({ messages: [m] }, { DSR_QUEUE: { send } });
    } finally {
      errorCalls = error.mock.calls;
      error.mockRestore();
    }
    expect(send).toHaveBeenCalledOnce();
    expect(send.mock.calls[0]?.[0]).toMatchObject({ dsr_id: "dlq-1", _dlq_requeue: 1 });
    expect(m.ack).toHaveBeenCalledOnce();
    expect(m.retry).not.toHaveBeenCalled();
    expect(errorCalls).toHaveLength(1);
    const alert = JSON.parse(String(errorCalls[0]?.[0])) as Record<string, unknown>;
    expect(alert).toMatchObject({
      event: "dsr.erasure.dead_letter",
      event_id: "dsr-erasure-dlq:dlq-1:0",
      exhausted: true,
      action: "requeue_once",
      requeue_count: 1,
      paging_status: "not_configured",
      paging_configured: false,
    });
  });

  it("leaves an already requeued message dead and alerts without looping", async () => {
    const send = vi.fn<(message: unknown) => Promise<void>>(async () => undefined);
    const m = fakeDlq({ ...msg("dlq-2"), _dlq_requeue: 1 });
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    let errorCalls: unknown[][] = [];
    try {
      await handleErasureDlqBatch({ messages: [m] }, { DSR_QUEUE: { send } });
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
      event_id: "dsr-erasure-dlq:dlq-2:1",
      exhausted: true,
      requeue_count: 1,
      action: "left_dead",
      paging_status: "not_configured",
      paging_configured: false,
    });
  });

  it("retains the DLQ copy when the bounded requeue fails", async () => {
    const send = vi.fn<(message: unknown) => Promise<void>>(async () => { throw new Error("queue down"); });
    const m = fakeDlq(msg("dlq-3"));
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    try {
      await handleErasureDlqBatch({ messages: [m] }, { DSR_QUEUE: { send } });
    } finally {
      error.mockRestore();
    }
    expect(send).toHaveBeenCalledOnce();
    expect(m.retry).toHaveBeenCalledOnce();
    expect(m.ack).not.toHaveBeenCalled();
  });

  it("keeps a stable correlation id across redelivery of the same DLQ generation", async () => {
    const send = vi.fn<(message: unknown) => Promise<void>>(async () => undefined);
    const first = fakeDlq(msg("stable-id"));
    const second = fakeDlq(msg("stable-id"));
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    try {
      await handleErasureDlqBatch({ messages: [first] }, { DSR_QUEUE: { send } });
      await handleErasureDlqBatch({ messages: [second] }, { DSR_QUEUE: { send } });
    } finally {
      const alerts = error.mock.calls.map((call) => JSON.parse(String(call[0])) as Record<string, unknown>);
      error.mockRestore();
      expect(alerts[0]?.event_id).toBe(alerts[1]?.event_id);
    }
    expect(send).toHaveBeenCalledTimes(2);
  });

  it("fails closed when PagerDuty rejects the page and retains the DLQ copy", async () => {
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
          PAGERDUTY_ROUTING_KEY: "routing-key",
          PAGERDUTY_FETCH: pagerFetch,
        },
      );
      const alert = JSON.parse(String(error.mock.calls[0]?.[0])) as Record<string, unknown>;
      expect(alert).toMatchObject({
        event_id: "dsr-erasure-dlq:pager-fail:0",
        exhausted: true,
        requeue_count: 0,
        action: "retry_paging",
        paging_status: "failed",
        paging_error: "transport_error",
      });
      expect(String(error.mock.calls[0]?.[0])).not.toContain("secret-provider-detail");
    } finally {
      error.mockRestore();
    }
    expect(pagerFetch).toHaveBeenCalledOnce();
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
      dedup_key: "dsr-erasure-dlq:pager-ok:0",
      payload: {
        severity: "critical",
        custom_details: {
          event_id: "dsr-erasure-dlq:pager-ok:0",
          exhausted: true,
          requeue_count: 0,
        },
      },
    });
    expect(m.ack).toHaveBeenCalledOnce();
    expect(m.retry).not.toHaveBeenCalled();
  });

  it("fails closed on malformed or oversized requeue markers instead of looping", async () => {
    const send = vi.fn<(message: unknown) => Promise<void>>(async () => undefined);
    const m = fakeDlq({ ...msg("poison-marker"), _dlq_requeue: Number.NaN });
    const error = vi.spyOn(console, "error").mockImplementation(() => undefined);
    try {
      await handleErasureDlqBatch({ messages: [m] }, { DSR_QUEUE: { send } });
      const alert = JSON.parse(String(error.mock.calls[0]?.[0])) as Record<string, unknown>;
      expect(alert).toMatchObject({
        event_id: "dsr-erasure-dlq:poison-marker:1",
        exhausted: true,
        action: "left_dead",
        requeue_count: 1,
      });
    } finally {
      error.mockRestore();
    }
    expect(send).not.toHaveBeenCalled();
    const alreadyCapped = fakeDlq({ ...msg("oversized-marker"), _dlq_requeue: 99 });
    await handleErasureDlqBatch({ messages: [alreadyCapped] }, { DSR_QUEUE: { send } });
    expect(alreadyCapped.ack).toHaveBeenCalledOnce();
  });
});
