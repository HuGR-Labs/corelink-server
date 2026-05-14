// Thin wrapper around the consent backend (S-11 endpoints). The functions
// here are injectable so tests can stub them without touching `fetch`.

import type {
  ConsentRow,
  ConsentSubmitPayload,
  JwtReceiptClaims,
} from "@/lib/consent-types";

export interface ConsentGrantResponse {
  consent_id: string;
  audit_event_id: string;
  jwt_receipt: string;
}

export interface ConsentHistoryQuery {
  page: number;
  page_size: number;
  status?: "active" | "withdrawn";
  purpose?: string;
  date_from?: string;
  date_to?: string;
}

export interface ConsentHistoryResponse {
  rows: ConsentRow[];
  total: number;
  page: number;
}

export interface ConsentApi {
  listActive: () => Promise<ConsentRow[]>;
  grant: (payload: ConsentSubmitPayload) => Promise<ConsentGrantResponse>;
  withdraw: (id: string, reason: string) => Promise<{ withdrawn_at: string; jwt_receipt: string }>;
  history: (q: ConsentHistoryQuery) => Promise<ConsentHistoryResponse>;
  subprocessors: () => Promise<string[]>;
}

async function jsonOrThrow<T>(res: Response): Promise<T> {
  if (!res.ok) throw new Error(`consent_api_error_${res.status}`);
  return (await res.json()) as T;
}

export const defaultConsentApi: ConsentApi = {
  async listActive() {
    return jsonOrThrow<ConsentRow[]>(await fetch("/v1/consent/active"));
  },
  async grant(payload) {
    return jsonOrThrow<ConsentGrantResponse>(
      await fetch("/v1/consent/grant", {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify(payload),
      }),
    );
  },
  async withdraw(id, reason) {
    return jsonOrThrow<{ withdrawn_at: string; jwt_receipt: string }>(
      await fetch(`/v1/consent/${encodeURIComponent(id)}/withdraw`, {
        method: "POST",
        headers: { "content-type": "application/json" },
        body: JSON.stringify({ reason }),
      }),
    );
  },
  async history(q) {
    const params = new URLSearchParams({
      page: String(q.page),
      page_size: String(q.page_size),
    });
    if (q.status) params.set("status", q.status);
    if (q.purpose) params.set("purpose", q.purpose);
    if (q.date_from) params.set("date_from", q.date_from);
    if (q.date_to) params.set("date_to", q.date_to);
    return jsonOrThrow<ConsentHistoryResponse>(
      await fetch(`/v1/consent/history?${params.toString()}`),
    );
  },
  async subprocessors() {
    return jsonOrThrow<string[]>(await fetch("/v1/subprocessors"));
  },
};

export type { JwtReceiptClaims };
