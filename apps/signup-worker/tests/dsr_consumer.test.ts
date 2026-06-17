/**
 * Unit tests for the DSR erasure queue consumer (WP-G).
 * Mocks the container call via env.CORELINK_API_SVC.fetch (no global fetch).
 */
import { describe, it, expect, vi } from "vitest";
import {
  processErasureMessage,
  handleErasureQueueBatch,
  type DsrConsumerEnv,
  type QueueMessageBatch,
  type QueueMessage,
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
  function fakeMsg(body: DsrQueuedV1): QueueMessage<DsrQueuedV1> & { ack: ReturnType<typeof vi.fn>; retry: ReturnType<typeof vi.fn> } {
    return { body, ack: vi.fn(), retry: vi.fn() };
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
