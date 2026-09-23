import { describe, expect, it, vi } from "vitest";
import { handleDsrDlqRedrive, type RecoveryStore } from "../src/webhooks/dsr_dlq_redrive.js";

const id = `dsr-erasure-dlq:${"a".repeat(64)}`, key = "r".repeat(32);
const envelope = { dsr_id:"00000000-0000-7000-8000-000000000001",tenant_id:"00000000-0000-7000-8000-000000000002",queued_at_ms:1,legal_hold:1,requeue_count:0 };
class Store implements RecoveryStore {
  state = "ready"; audit: string[] = [];
  async capture() {} async ready() { this.state="ready"; } async close() { this.state="closed"; }
  async claim() { if (this.state === "expired") return "expired" as const; if (this.state !== "ready") return "denied" as const; this.state="claimed"; this.audit.push("claimed"); return "claimed" as const; }
  async envelope() { return this.state === "claimed" ? envelope : null; }
  async fence() { if (this.state !== "claimed") return false; this.state="ambiguous"; this.audit.push("ambiguous"); return true; }
  async submitted() { if (this.state !== "ambiguous") throw Error(); this.state="submitted"; this.audit.push("submitted"); }
  async ambiguous() { this.state="ambiguous"; } async cleanup() {}
}
function request(body: unknown = {event_id:id}, auth = key) { return new Request("https://x/internal/dsr/dlq/redrive",{method:"POST",headers:{authorization:`Bearer ${auth}`,"x-corelink-operator-ref":"op_2166","x-corelink-approval-ref":"apr_2166"},body:JSON.stringify(body)}); }
function env(store: Store, send = vi.fn(async () => undefined)) { return { DSR_DLQ_REDRIVE_AUTH_KEY:key,ERASURE_SALT_KEY:key,ENVIRONMENT:"prod",DSR_QUEUE:{send},DSR_DLQ_RECOVERY:store }; }
describe("DSR DLQ redrive", () => {
  it("rejects unauthorized, tenant substitution, expired, claimed receipts", async () => { const s=new Store(), e=env(s); expect((await handleDsrDlqRedrive(request({event_id:id,tenant_id:"x"}),e)).status).toBe(400); expect((await handleDsrDlqRedrive(request({event_id:id},"shared"),e)).status).toBe(401); s.state="expired"; expect((await handleDsrDlqRedrive(request(),e)).status).toBe(410); s.state="claimed"; expect((await handleDsrDlqRedrive(request(),e)).status).toBe(409); });
  it("derives worker salt, preserves stored tenant/legal hold, sends once", async () => { const s=new Store(), send=vi.fn(async () => undefined), e=env(s,send); expect((await handleDsrDlqRedrive(request(),e)).status).toBe(202); expect((await handleDsrDlqRedrive(request(),e)).status).toBe(409); expect(send).toHaveBeenCalledOnce(); expect(send.mock.calls[0]?.[0]).toMatchObject({tenant_id:envelope.tenant_id,subject_id:envelope.tenant_id,legal_hold:true,_dlq_requeue:1}); expect(JSON.stringify(s)).not.toContain("erasure_salt_hex"); });
  it("keeps ambiguous send visible, never auto-retries", async () => { const s=new Store(), send=vi.fn(async () => { throw Error("provider payload"); }), e=env(s,send); expect((await handleDsrDlqRedrive(request(),e)).status).toBe(502); expect((await handleDsrDlqRedrive(request(),e)).status).toBe(409); expect(send).toHaveBeenCalledOnce(); expect(s.state).toBe("ambiguous"); });
});
