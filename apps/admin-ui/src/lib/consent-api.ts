// Thin wrapper around the consent backend (S-11 endpoints). The functions
// here are injectable so tests can stub them without touching `fetch`.
//
// ⚠️ NO BACKEND EXISTS FOR THIS SURFACE (verified 2026-08-03).
//   None of `/v1/consent/active`, `/v1/consent/grant`,
//   `/v1/consent/{id}/withdraw`, `/v1/consent/history` or `/v1/subprocessors`
//   is registered by any handler in the repo: the 55 `.route("…")` mounts
//   across `crates/` contain zero consent paths, `worker/src/index.ts` has no
//   `/v1/consent` branch, and `corelink-privacy` — the crate that owns the
//   consent ledger and whose doc-comments describe this API — is not a
//   dependency of `corelink-container` or of any Worker, so it is not compiled
//   into anything deployed. Its DESIGNED shape does not even match this client
//   (`POST /v1/consent/<purpose>` + `DELETE /v1/consent/<purpose>` +
//   `GET /v1/consent`, not `/grant` + `/{id}/withdraw` + `/history`).
//   `corelink-api` answers 401 to EVERY `/v1/*` path — including
//   `/v1/definitely-not-a-real-route-xyz123` and `/v1/healthz` — so the 401 you
//   get here is a blanket edge auth gate and is NOT evidence the route exists.
//   This module is therefore correct-but-unserved: it now addresses the right
//   origin with the right credential, and will start working the day the
//   backend lands. Until then every call fails honestly instead of silently
//   parsing marketing HTML.
//
// AUTH: the Clerk session bearer, matching every other tenant-scoped `/v1/*`
//   browser client in this app (`CustomerClient` via `useCustomerClient`,
//   `DsrClient`, the onboarding server actions). Because no handler exists
//   there is no server-side contract to read; the Clerk session is the
//   convention this whole surface uses and the only credential the browser
//   holds.

import type {
  ConsentRow,
  ConsentSubmitPayload,
  JwtReceiptClaims,
} from "@/lib/consent-types";
import { resolveApiBaseUrl } from "@/lib/api-base-url";

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

export interface ConsentApiOptions {
  /** Override the API origin. Defaults to {@link resolveApiBaseUrl}. */
  baseUrl?: string;
  fetchImpl?: typeof fetch;
  /**
   * Async supplier of the caller's Clerk session token (e.g.
   * `useAuth().getToken`). When provided and it resolves to a non-null string
   * every request carries `Authorization: Bearer <token>`. When omitted (or it
   * resolves to null) NO Authorization header is sent — which keeps E2E mock
   * mode byte-identical. Mirrors `CustomerClientOptions.getToken`.
   */
  getToken?: () => Promise<string | null>;
}

async function jsonOrThrow<T>(res: Response): Promise<T> {
  if (!res.ok) throw new Error(`consent_api_error_${res.status}`);
  return (await res.json()) as T;
}

/**
 * Build a `ConsentApi` bound to the resolved API origin.
 *
 * Prefer the `useConsentApi()` hook (`lib/use-consent-api.ts`) inside React —
 * it supplies the Clerk `getToken` and keeps the client identity stable.
 */
export function createConsentApi(opts: ConsentApiOptions = {}): ConsentApi {
  // Resolved per REQUEST, not at construction: `defaultConsentApi` is built at
  // module-evaluation time, which on the server happens before a test (or a
  // runtime config hop) can set NEXT_PUBLIC_CORELINK_API_URL. Freezing the
  // origin here would pin whatever `/api` fallback was visible at import.
  const baseUrl = () => opts.baseUrl ?? resolveApiBaseUrl();
  const doFetch = opts.fetchImpl ?? ((...a: Parameters<typeof fetch>) => fetch(...a));

  async function authHeaders(): Promise<Record<string, string>> {
    // Same E2E carve-out as `CustomerClient.request()`: in E2E mode the
    // endpoints are served by the deterministic mock with no auth and there is
    // no real Clerk session, so awaiting `getToken()` can stall forever and
    // leave the screen on its loading Skeleton. Inert in production —
    // NEXT_PUBLIC_E2E_TEST_MODE is never set there.
    const isE2E = process.env.NEXT_PUBLIC_E2E_TEST_MODE === "1";
    const token = isE2E ? null : opts.getToken ? await opts.getToken() : null;
    return token != null ? { authorization: `Bearer ${token}` } : {};
  }

  async function request<T>(path: string, init: RequestInit = {}): Promise<T> {
    const res = await doFetch(`${baseUrl()}${path}`, {
      ...init,
      headers: {
        "content-type": "application/json",
        ...(await authHeaders()),
        ...(init.headers ?? {}),
      },
    });
    return jsonOrThrow<T>(res);
  }

  return {
    async listActive() {
      return request<ConsentRow[]>("/v1/consent/active");
    },
    async grant(payload) {
      return request<ConsentGrantResponse>("/v1/consent/grant", {
        method: "POST",
        body: JSON.stringify(payload),
      });
    },
    async withdraw(id, reason) {
      return request<{ withdrawn_at: string; jwt_receipt: string }>(
        `/v1/consent/${encodeURIComponent(id)}/withdraw`,
        { method: "POST", body: JSON.stringify({ reason }) },
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
      return request<ConsentHistoryResponse>(`/v1/consent/history?${params.toString()}`);
    },
    async subprocessors() {
      return request<string[]>("/v1/subprocessors");
    },
  };
}

/**
 * Anonymous (no Clerk token) consent client. Retained as the non-React entry
 * point; React callers should use `useConsentApi()` so the session bearer is
 * actually attached.
 */
export const defaultConsentApi: ConsentApi = createConsentApi();

export type { JwtReceiptClaims };
