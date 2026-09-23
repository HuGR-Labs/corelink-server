import { describe, expect, it, vi } from "vitest";
import {
  D1DsrDlqRecoveryStore,
  handleDsrDlqRedrive,
  runDsrDlqRedriveCleanup,
  type DsrDlqRecoveryEnvelopeInput,
  type DsrDlqRecoveryStore,
} from "../src/webhooks/dsr_dlq_redrive.js";

const EVENT_ID = `dsr-erasure-dlq:${"a".repeat(64)}`;
const DSR_ID = "00000000-0000-7000-8000-000000000001";
const TENANT_ID = "00000000-0000-7000-8000-000000000002";
const KEY = "r".repeat(32);

type State = "captured" | "ready" | "closed" | "claimed" | "submitted" | "ambiguous";
type Envelope = DsrDlqRecoveryEnvelopeInput & {
  state: State;
  expiresAtMs: number;
  claimExpiresAtMs: number;
  actor?: string;
  approval?: string;
};

class MemoryRecoveryStore implements DsrDlqRecoveryStore {
  readonly rows = new Map<string, Envelope>();
  readonly audit: Array<{ eventId: string; transition: "claimed" | "submitted" | "ambiguous" }> = [];

  async capture(input: DsrDlqRecoveryEnvelopeInput, nowMs: number): Promise<void> {
    if (!this.rows.has(input.eventId)) {
      this.rows.set(input.eventId, { ...input, state: "captured", expiresAtMs: nowMs + 7 * 24 * 60 * 60_000, claimExpiresAtMs: 0 });
    }
  }
  async makeReady(eventId: string): Promise<void> {
    const row = this.rows.get(eventId);
    if (row?.state === "captured" && row.requeueCount === 0) row.state = "ready";
  }
  async close(eventId: string): Promise<void> {
    const row = this.rows.get(eventId);
    if (row && (row.state === "captured" || row.state === "ready")) row.state = "closed";
  }
  async claim(eventId: string, actor: string, approval: string, nowMs: number): Promise<"claimed" | "expired" | "not_ready"> {
    const row = this.rows.get(eventId);
    if (!row) return "not_ready";
    if (row.expiresAtMs <= nowMs) return "expired";
    if (row.state !== "ready" || row.requeueCount !== 0) return "not_ready";
    row.state = "claimed";
    row.actor = actor;
    row.approval = approval;
    row.claimExpiresAtMs = nowMs + 5 * 60_000;
    this.audit.push({ eventId, transition: "claimed" });
    return "claimed";
  }
  async prepareDispatch(eventId: string, nowMs: number): Promise<boolean> {
    const row = this.rows.get(eventId);
    if (!row || row.state !== "claimed" || row.claimExpiresAtMs <= nowMs || row.expiresAtMs <= nowMs) return false;
    row.state = "ambiguous";
    this.audit.push({ eventId, transition: "ambiguous" });
    return true;
  }
  async submitted(eventId: string): Promise<void> {
    const row = this.rows.get(eventId);
    if (!row || row.state !== "ambiguous") throw new Error("not dispatching");
    row.state = "submitted";
    this.audit.push({ eventId, transition: "submitted" });
  }
  async ambiguous(eventId: string): Promise<void> {
    const row = this.rows.get(eventId);
    if (!row) throw new Error("missing");
    if (row.state === "claimed") {
      row.state = "ambiguous";
      this.audit.push({ eventId, transition: "ambiguous" });
    }
  }
  async cleanup(nowMs: number): Promise<void> {
    for (const [eventId, row] of this.rows) {
      if (row.state === "claimed" && row.claimExpiresAtMs <= nowMs) {
        row.state = "ambiguous";
        this.audit.push({ eventId, transition: "ambiguous" });
      }
      if (row.expiresAtMs <= nowMs && row.state !== "claimed") this.rows.delete(eventId);
    }
  }
  async claimedEnvelope(eventId: string) {
    const row = this.rows.get(eventId);
    if (!row || row.state !== "claimed") return null;
    return {
      dsr_id: row.dsrId,
      tenant_id: row.tenantId,
      queued_at_ms: row.queuedAtMs,
      legal_hold: row.legalHold ? 1 : 0,
      requeue_count: row.requeueCount,
    };
  }
}

async function readyStore(): Promise<MemoryRecoveryStore> {
  const store = new MemoryRecoveryStore();
  await store.capture({ eventId: EVENT_ID, dsrId: DSR_ID, tenantId: TENANT_ID, queuedAtMs: 123, legalHold: true, requeueCount: 0 }, Date.now());
  await store.makeReady(EVENT_ID, Date.now());
  return store;
}

function request(eventId = EVENT_ID, extra: Record<string, unknown> = {}, headers: HeadersInit = {}): Request {
  const requestHeaders = new Headers({
    authorization: `Bearer ${KEY}`,
    "x-corelink-operator-ref": "op_incident_2166",
    "x-corelink-approval-ref": "apr_change_2166",
    "content-type": "application/json",
  });
  for (const [key, value] of new Headers(headers)) requestHeaders.set(key, value);
  return new Request("https://worker.test/internal/dsr/dlq/redrive", {
    method: "POST",
    headers: requestHeaders,
    body: JSON.stringify({ event_id: eventId, ...extra }),
  });
}

describe("DSR DLQ operator redrive", () => {
  it("rejects a shared-secret substitute before it reads or claims a receipt", async () => {
    const store = await readyStore();
    const send = vi.fn<(message: unknown) => Promise<void>>(async () => undefined);
    const response = await handleDsrDlqRedrive(request(EVENT_ID, {}, { authorization: "Bearer shared-key" }), {
      DSR_DLQ_REDRIVE_AUTH_KEY: KEY, ERASURE_SALT_KEY: KEY, ENVIRONMENT: "prod", DSR_QUEUE: { send }, DSR_DLQ_RECOVERY: store,
    });
    expect(response.status).toBe(401);
    expect(send).not.toHaveBeenCalled();
    expect(store.rows.get(EVENT_ID)?.state).toBe("ready");
  });

  it("rejects a request body that tries to substitute a tenant", async () => {
    const store = await readyStore();
    const send = vi.fn<(message: unknown) => Promise<void>>(async () => undefined);
    const response = await handleDsrDlqRedrive(request(EVENT_ID, { tenant_id: "attacker-selected" }), {
      DSR_DLQ_REDRIVE_AUTH_KEY: KEY, ERASURE_SALT_KEY: KEY, ENVIRONMENT: "prod", DSR_QUEUE: { send }, DSR_DLQ_RECOVERY: store,
    });
    expect(response.status).toBe(400);
    expect(send).not.toHaveBeenCalled();
    expect(store.rows.get(EVENT_ID)?.state).toBe("ready");
  });

  it("uses the stored tenant and legal hold, derives the salt in-worker, and submits exactly once", async () => {
    const store = await readyStore();
    const send = vi.fn<(message: unknown) => Promise<void>>(async () => undefined);
    const env = { DSR_DLQ_REDRIVE_AUTH_KEY: KEY, ERASURE_SALT_KEY: KEY, ENVIRONMENT: "prod", DSR_QUEUE: { send }, DSR_DLQ_RECOVERY: store };
    expect((await handleDsrDlqRedrive(request(), env)).status).toBe(202);
    expect((await handleDsrDlqRedrive(request(), env)).status).toBe(409);
    expect(send).toHaveBeenCalledOnce();
    expect(send.mock.calls[0]?.[0]).toMatchObject({
      dsr_id: DSR_ID,
      tenant_id: TENANT_ID,
      subject_id: TENANT_ID,
      queued_at_ms: 123,
      legal_hold: true,
      _dlq_requeue: 1,
    });
    const sent = JSON.stringify(send.mock.calls[0]?.[0]);
    expect(sent).not.toContain("clerk_user_id");
    expect(store.audit.map((entry) => entry.transition)).toEqual(["claimed", "ambiguous", "submitted"]);
  });

  it("rejects an already claimed receipt without a second queue attempt", async () => {
    const store = await readyStore();
    await store.claim(EVENT_ID, "op_first", "apr_first", Date.now());
    const send = vi.fn<(message: unknown) => Promise<void>>(async () => undefined);
    const response = await handleDsrDlqRedrive(request(), {
      DSR_DLQ_REDRIVE_AUTH_KEY: KEY, ERASURE_SALT_KEY: KEY, ENVIRONMENT: "prod", DSR_QUEUE: { send }, DSR_DLQ_RECOVERY: store,
    });
    expect(response.status).toBe(409);
    expect(send).not.toHaveBeenCalled();
    expect(store.rows.get(EVENT_ID)?.state).toBe("claimed");
  });

  it("returns expired without a queue side effect", async () => {
    const store = await readyStore();
    store.rows.get(EVENT_ID)!.expiresAtMs = Date.now() - 1;
    const send = vi.fn<(message: unknown) => Promise<void>>(async () => undefined);
    const response = await handleDsrDlqRedrive(request(), {
      DSR_DLQ_REDRIVE_AUTH_KEY: KEY, ERASURE_SALT_KEY: KEY, ENVIRONMENT: "prod", DSR_QUEUE: { send }, DSR_DLQ_RECOVERY: store,
    });
    expect(response.status).toBe(410);
    expect(send).not.toHaveBeenCalled();
  });

  it("records an ambiguous outcome and never retries automatically after a queue error", async () => {
    const store = await readyStore();
    const send = vi.fn<(message: unknown) => Promise<void>>(async () => { throw new Error("provider data must not persist"); });
    const env = { DSR_DLQ_REDRIVE_AUTH_KEY: KEY, ERASURE_SALT_KEY: KEY, ENVIRONMENT: "prod", DSR_QUEUE: { send }, DSR_DLQ_RECOVERY: store };
    expect((await handleDsrDlqRedrive(request(), env)).status).toBe(502);
    expect((await handleDsrDlqRedrive(request(), env)).status).toBe(409);
    expect(send).toHaveBeenCalledOnce();
    expect(store.rows.get(EVENT_ID)?.state).toBe("ambiguous");
    expect(store.audit.map((entry) => entry.transition)).toEqual(["claimed", "ambiguous"]);
  });

  it("turns a stale claim ambiguous and removes expired bounded metadata", async () => {
    const store = await readyStore();
    await store.claim(EVENT_ID, "op_cleanup", "apr_cleanup", 1);
    await runDsrDlqRedriveCleanup({ DSR_DLQ_RECOVERY: store }, 5 * 60_000 + 1);
    expect(store.rows.get(EVENT_ID)?.state).toBe("ambiguous");
    store.rows.get(EVENT_ID)!.expiresAtMs = 2;
    await runDsrDlqRedriveCleanup({ DSR_DLQ_RECOVERY: store }, 3);
    expect(store.rows.has(EVENT_ID)).toBe(false);
  });

  it("does not dispatch after the claim lease expires", async () => {
    const store = await readyStore();
    await store.claim(EVENT_ID, "op_lease", "apr_lease", 1);
    expect(await store.prepareDispatch(EVENT_ID, 5 * 60_000 + 1)).toBe(false);
    await runDsrDlqRedriveCleanup({ DSR_DLQ_RECOVERY: store }, 5 * 60_000 + 1);
    expect(store.rows.get(EVENT_ID)?.state).toBe("ambiguous");
  });

  it("requires the envelope expiry to be strictly after the pre-send fence time", async () => {
    const beforeExpiry = await readyStore();
    const expiry = beforeExpiry.rows.get(EVENT_ID)!.expiresAtMs;
    await beforeExpiry.claim(EVENT_ID, "op_boundary", "apr_boundary", expiry - 2);
    expect(await beforeExpiry.prepareDispatch(EVENT_ID, expiry - 1)).toBe(true);

    const atExpiry = await readyStore();
    const sameExpiry = atExpiry.rows.get(EVENT_ID)!.expiresAtMs;
    await atExpiry.claim(EVENT_ID, "op_boundary", "apr_boundary", sameExpiry - 2);
    expect(await atExpiry.prepareDispatch(EVENT_ID, sameExpiry)).toBe(false);
    expect(atExpiry.rows.get(EVENT_ID)?.state).toBe("claimed");
  });

  it("does not send when the durable pre-send fence rejects an expired claim", async () => {
    const store = await readyStore();
    const send = vi.fn<(message: unknown) => Promise<void>>(async () => undefined);
    vi.spyOn(store, "prepareDispatch").mockResolvedValue(false);
    const response = await handleDsrDlqRedrive(request(), {
      DSR_DLQ_REDRIVE_AUTH_KEY: KEY, ERASURE_SALT_KEY: KEY, ENVIRONMENT: "prod", DSR_QUEUE: { send }, DSR_DLQ_RECOVERY: store,
    });
    expect(response.status).toBe(409);
    expect(send).not.toHaveBeenCalled();
    expect(store.rows.get(EVENT_ID)?.state).toBe("claimed");
  });

  it("binds the expiry instant into the strict D1 pre-send fence", async () => {
    const calls: Array<{ sql: string; values: unknown[] }> = [];
    const db = {
      prepare(sql: string) {
        const values: unknown[] = [];
        return {
          bind(...bound: unknown[]) { values.push(...bound); return this; },
          async first<T = unknown>(): Promise<T | null> { return null; },
          async run() { calls.push({ sql, values }); return { success: true, meta: { changes: 1 } }; },
        };
      },
    };
    const store = new D1DsrDlqRecoveryStore(db);
    expect(await store.prepareDispatch(EVENT_ID, 7_000)).toBe(true);
    expect(calls[0]?.sql).toContain("claim_expires_at_ms > ?2 AND expires_at_ms > ?2");
    expect(calls[0]?.values).toEqual([EVENT_ID, 7_000]);
  });

  it("persists only the bounded envelope fields", async () => {
    const calls: Array<{ sql: string; values: unknown[] }> = [];
    const db = {
      prepare(sql: string) {
        const values: unknown[] = [];
        return {
          bind(...bound: unknown[]) { values.push(...bound); return this; },
          async first<T = unknown>(): Promise<T | null> { return null; },
          async run() { calls.push({ sql, values }); return { success: true, meta: { changes: 1 } }; },
        };
      },
    };
    const store = new D1DsrDlqRecoveryStore(db);
    await store.capture({ eventId: EVENT_ID, dsrId: DSR_ID, tenantId: TENANT_ID, queuedAtMs: 123, legalHold: true, requeueCount: 0 }, 1);
    const serialized = JSON.stringify(calls);
    for (const forbidden of ["clerk_user_id", "erasure_salt_hex", "provider-response", "secret-value", "recovery_payload_json"]) {
      expect(serialized).not.toContain(forbidden);
    }
    expect(calls[0]?.values).toEqual([EVENT_ID, DSR_ID, TENANT_ID, 123, 1, 0, 1, 7 * 24 * 60 * 60_000 + 1]);
  });
});
