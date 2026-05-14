import type { ConsentApi, ConsentGrantResponse } from "@/lib/consent-api";
import type { ConsentRow } from "@/lib/consent-types";

export function makeJwtReceipt(claims: {
  tenant_id?: string;
  consent_id?: string;
  granted_at?: string;
  locale?: string;
  jti?: string;
  exp?: number;
}): string {
  const header = { alg: "HS256", typ: "JWT" };
  const payload = {
    tenant_id: "tnt_test_123",
    consent_id: "csn_test_456",
    granted_at: "2026-05-14T00:00:00Z",
    locale: "pt-BR",
    jti: "jti-abc",
    exp: 9999999999,
    ...claims,
  };
  const b64 = (obj: unknown) =>
    Buffer.from(JSON.stringify(obj))
      .toString("base64")
      .replace(/=+$/, "")
      .replace(/\+/g, "-")
      .replace(/\//g, "_");
  return `${b64(header)}.${b64(payload)}.sig-not-verified-client-side`;
}

export interface MockApiState {
  active: ConsentRow[];
  historyRows: ConsentRow[];
  historyCalls: Array<Record<string, unknown>>;
  grantCalls: Array<unknown>;
  withdrawCalls: Array<{ id: string; reason: string }>;
  grantResponse?: ConsentGrantResponse;
}

export function makeMockApi(state: MockApiState): ConsentApi {
  return {
    async listActive() {
      return state.active;
    },
    async grant(payload) {
      state.grantCalls.push(payload);
      return (
        state.grantResponse ?? {
          consent_id: "csn_test_456",
          audit_event_id: "evt_abc_001",
          jwt_receipt: makeJwtReceipt({}),
        }
      );
    },
    async withdraw(id, reason) {
      state.withdrawCalls.push({ id, reason });
      return { withdrawn_at: "2026-05-14T01:02:03Z", jwt_receipt: makeJwtReceipt({}) };
    },
    async history(q) {
      state.historyCalls.push(q as unknown as Record<string, unknown>);
      // Hard-coded total ensures multi-page navigation is exercisable.
      return { rows: state.historyRows, total: 75, page: q.page };
    },
    async subprocessors() {
      return ["Cloudflare", "Stripe"];
    },
  };
}
