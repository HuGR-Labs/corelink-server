import { deriveErasureSalt } from "./clerk_erasure.js";
import { constantTimeEqual } from "./github_provision.js";

const TTL = 7 * 24 * 60 * 60_000;
const LEASE = 5 * 60_000;
const EVENT = /^dsr-erasure-dlq:[0-9a-f]{64}$/;
const UUID = /^[0-9a-f]{8}-[0-9a-f]{4}-[1-8][0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;

export interface D1RedriveDb { prepare(sql: string): { bind(...v: unknown[]): { first<T>(): Promise<T | null>; run(): Promise<{ success: boolean; meta?: { changes?: number } }> } } }
export interface RecoveryInput { eventId: string; dsrId: string; tenantId: string; queuedAtMs: number; legalHold: boolean; requeueCount: number }
export interface RecoveryStore {
  capture(input: RecoveryInput, now: number): Promise<void>;
  ready(id: string, now: number): Promise<void>;
  close(id: string, now: number): Promise<void>;
  claim(id: string, actor: string, approval: string, now: number): Promise<"claimed" | "expired" | "denied">;
  envelope(id: string): Promise<{ dsr_id: string; tenant_id: string; queued_at_ms: number; legal_hold: number; requeue_count: number } | null>;
  fence(id: string, now: number): Promise<boolean>;
  submitted(id: string, now: number): Promise<void>;
  ambiguous(id: string, now: number): Promise<void>;
  cleanup(now: number): Promise<void>;
}

export class D1RecoveryStore implements RecoveryStore {
  constructor(private readonly db: D1RedriveDb) {}
  async capture(i: RecoveryInput, now: number) {
    if (!EVENT.test(i.eventId) || !UUID.test(i.dsrId) || !UUID.test(i.tenantId) || !Number.isSafeInteger(i.queuedAtMs) || i.queuedAtMs < 0 || (i.requeueCount !== 0 && i.requeueCount !== 1)) throw new Error("invalid_recovery_envelope");
    const r = await this.db.prepare("INSERT OR IGNORE INTO dsr_dlq_redrive_envelopes (event_id,dsr_id,tenant_id,queued_at_ms,legal_hold,requeue_count,state,expires_at_ms,claim_expires_at_ms,updated_at_ms) VALUES (?1,?2,?3,?4,?5,?6,'captured',?7,0,?8)").bind(i.eventId,i.dsrId,i.tenantId,i.queuedAtMs,i.legalHold ? 1 : 0,i.requeueCount,now + TTL,now).run(); if (!r.success) throw new Error("capture_failed");
  }
  async ready(id: string, now: number) { const r = await this.db.prepare("UPDATE dsr_dlq_redrive_envelopes SET state='ready',updated_at_ms=?2 WHERE event_id=?1 AND state='captured' AND requeue_count=0").bind(id,now).run(); if (!r.success) throw new Error("ready_failed"); }
  async close(id: string, now: number) { const r = await this.db.prepare("UPDATE dsr_dlq_redrive_envelopes SET state='closed',updated_at_ms=?2 WHERE event_id=?1 AND state IN ('captured','ready')").bind(id,now).run(); if (!r.success) throw new Error("close_failed"); }
  async claim(id: string, actor: string, approval: string, now: number) { const r = await this.db.prepare("UPDATE dsr_dlq_redrive_envelopes SET state='claimed',actor_ref=?2,approval_ref=?3,claim_expires_at_ms=?4,updated_at_ms=?5 WHERE event_id=?1 AND state='ready' AND requeue_count=0 AND expires_at_ms>?5").bind(id,actor,approval,now + LEASE,now).run(); if (!r.success) throw new Error("claim_failed"); if (r.meta?.changes === 1) return "claimed" as const; const x = await this.db.prepare("SELECT expires_at_ms FROM dsr_dlq_redrive_envelopes WHERE event_id=?1").bind(id).first<{expires_at_ms:number}>(); return x && x.expires_at_ms <= now ? "expired" : "denied"; }
  async envelope(id: string) { return this.db.prepare("SELECT dsr_id,tenant_id,queued_at_ms,legal_hold,requeue_count FROM dsr_dlq_redrive_envelopes WHERE event_id=?1 AND state='claimed'").bind(id).first<{dsr_id:string;tenant_id:string;queued_at_ms:number;legal_hold:number;requeue_count:number}>(); }
  async fence(id: string, now: number) { const r = await this.db.prepare("UPDATE dsr_dlq_redrive_envelopes SET state='ambiguous',updated_at_ms=?2 WHERE event_id=?1 AND state='claimed' AND claim_expires_at_ms>?2 AND expires_at_ms>?2").bind(id,now).run(); if (!r.success) throw new Error("fence_failed"); return r.meta?.changes === 1; }
  async submitted(id: string, now: number) { const r = await this.db.prepare("UPDATE dsr_dlq_redrive_envelopes SET state='submitted',updated_at_ms=?2 WHERE event_id=?1 AND state='ambiguous'").bind(id,now).run(); if (!r.success || r.meta?.changes !== 1) throw new Error("submitted_failed"); }
  async ambiguous(id: string, now: number) { const r = await this.db.prepare("UPDATE dsr_dlq_redrive_envelopes SET state='ambiguous',updated_at_ms=?2 WHERE event_id=?1 AND state='claimed'").bind(id,now).run(); if (!r.success) throw new Error("ambiguous_failed"); }
  async cleanup(now: number) { await this.db.prepare("UPDATE dsr_dlq_redrive_envelopes SET state='ambiguous',updated_at_ms=?1 WHERE state='claimed' AND claim_expires_at_ms<=?1").bind(now).run(); await this.db.prepare("DELETE FROM dsr_dlq_redrive_audit WHERE occurred_at_ms<=?1").bind(now - 30 * 24 * 60 * 60_000).run(); await this.db.prepare("DELETE FROM dsr_dlq_redrive_envelopes WHERE expires_at_ms<=?1 AND state<>'claimed'").bind(now).run(); }
}

export interface DsrDlqRedriveEnv { DSR_DLQ_REDRIVE_AUTH_KEY?: string; ERASURE_SALT_KEY?: string; ENVIRONMENT?: string; DSR_QUEUE?: { send(message: unknown): Promise<void> }; CONFIG_DB?: D1RedriveDb; DSR_DLQ_RECOVERY?: RecoveryStore }
const response = (status: number, error: string) => Response.json({ error }, { status });
const ref = (v: string, p: string) => v.length <= 96 && new RegExp(`^${p}[A-Za-z0-9._:-]+$`).test(v);
export async function handleDsrDlqRedrive(request: Request, env: DsrDlqRedriveEnv): Promise<Response> {
  const key = env.DSR_DLQ_REDRIVE_AUTH_KEY?.trim(); if (request.method !== "POST") return response(405,"method_not_allowed"); if (!key || key.length < 32) return response(503,"unavailable");
  const token = (request.headers.get("authorization") ?? "").replace(/^Bearer /, ""); if (!constantTimeEqual(token,key)) return response(401,"unauthorized");
  const actor = request.headers.get("x-corelink-operator-ref") ?? "", approval = request.headers.get("x-corelink-approval-ref") ?? ""; if (!ref(actor,"op_") || !ref(approval,"apr_")) return response(400,"invalid_operator_reference");
  let body: unknown; try { body = await request.json(); } catch { return response(400,"invalid_receipt"); } if (!body || typeof body !== "object" || Array.isArray(body) || Object.keys(body).length !== 1 || !EVENT.test((body as {event_id?:unknown}).event_id as string)) return response(400,"invalid_receipt");
  const id = (body as {event_id:string}).event_id, store = env.DSR_DLQ_RECOVERY ?? (env.CONFIG_DB && new D1RecoveryStore(env.CONFIG_DB)); if (!store || !env.DSR_QUEUE) return response(503,"unavailable"); const now = Date.now();
  let claim: "claimed"|"expired"|"denied"; try { claim = await store.claim(id,actor,approval,now); } catch { return response(503,"unavailable"); } if (claim === "expired") return response(410,"receipt_expired"); if (claim !== "claimed") return response(409,"receipt_not_redriveable");
  const e = await store.envelope(id); if (!e || !UUID.test(e.dsr_id) || !UUID.test(e.tenant_id) || e.requeue_count !== 0) { await store.ambiguous(id,Date.now()); return response(409,"receipt_not_redriveable"); }
  let salt: string; try { salt = await deriveErasureSalt(e.dsr_id,env.ERASURE_SALT_KEY,env.ENVIRONMENT); } catch { return response(503,"unavailable"); } if (!await store.fence(id,Date.now())) return response(409,"receipt_not_redriveable");
  try { await env.DSR_QUEUE.send({schema:"dev.hugr.corelink.dsr.queued.v1",dsr_id:e.dsr_id,tenant_id:e.tenant_id,subject_id:e.tenant_id,erasure_salt_hex:salt,queued_at_ms:e.queued_at_ms,legal_hold:e.legal_hold===1,source:"clerk.user.deleted",_dlq_requeue:1}); await store.submitted(id,Date.now()); return Response.json({status:"submitted"},{status:202}); } catch { await store.ambiguous(id,Date.now()); return response(502,"redrive_ambiguous"); }
}
