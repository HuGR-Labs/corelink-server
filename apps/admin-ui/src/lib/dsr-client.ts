/**
 * Typed DSR API client (WI-S16-004).
 *
 * Talks to the backend surface shipped by WI-S11-001:
 *   POST /v1/privacy/dsr/{action}                  → submitDsr
 *   GET  /v1/privacy/dsr/{request_id}/status       → getDsrStatus
 *   GET  /v1/privacy/dsr                           → listMyDsrs
 *   GET  /v1/users/me                              → getMyProfile
 *   GET  /v1/data-categories                       → listDataCategories
 *
 * Security:
 *   - CTRL-CRED-001: bearer token from Clerk is sent only via Authorization
 *     header. NEVER logged, NEVER persisted, NEVER returned.
 *   - CTRL-PRIV-001: response errors are passed through `redactString` before
 *     surfacing to the caller, so token-shaped data never reaches Sentry.
 */

import {
  DSR_ACTIONS,
  type DataCategory,
  type DsrAction,
  type DsrRequestDetail,
  type DsrRequestSummary,
  type DsrSubmitRequest,
  type DsrSubmitResponse,
  type MeProfile,
} from "./dsr-types";
import { redactString } from "./safe-log";

export interface DsrClientOptions {
  /** Bearer token from Clerk session.getToken(). */
  token: string;
  baseUrl?: string;
  fetchImpl?: typeof fetch;
  signal?: AbortSignal;
}

export class DsrClientError extends Error {
  readonly status: number;
  readonly translationKey: string;

  constructor(status: number, translationKey: string, message: string) {
    super(message);
    this.status = status;
    this.translationKey = translationKey;
  }
}

function defaultBaseUrl(): string {
  const fromEnv =
    typeof process !== "undefined"
      ? process.env?.NEXT_PUBLIC_CORELINK_API_URL
      : undefined;
  return fromEnv ?? "https://api.corelink.dev";
}

function headers(opts: DsrClientOptions): Headers {
  const h = new Headers();
  h.set("Accept", "application/json");
  h.set("Content-Type", "application/json");
  // Authorization is set last and never read back from the Headers object.
  h.set("Authorization", `Bearer ${opts.token}`);
  return h;
}

async function callJson<T>(
  url: string,
  init: RequestInit,
  opts: DsrClientOptions,
): Promise<T> {
  const f = opts.fetchImpl ?? fetch;
  const res = await f(url, { ...init, signal: opts.signal });
  const text = await res.text();
  if (!res.ok) {
    throw mapError(res.status, text);
  }
  if (text.length === 0) return undefined as T;
  return JSON.parse(text) as T;
}

function mapError(status: number, body: string): DsrClientError {
  const safe = redactString(body).slice(0, 200);
  if (status === 401 || status === 403) {
    return new DsrClientError(status, "dsr.form.submit_error_mfa", safe);
  }
  if (status === 429) {
    return new DsrClientError(
      status,
      "dsr.form.submit_error_rate_limit",
      safe,
    );
  }
  return new DsrClientError(status, "dsr.form.submit_error_generic", safe);
}

export async function submitDsr(
  req: DsrSubmitRequest,
  opts: DsrClientOptions,
): Promise<DsrSubmitResponse> {
  if (!DSR_ACTIONS.includes(req.action)) {
    throw new DsrClientError(
      400,
      "dsr.form.submit_error_generic",
      `Unsupported DSR action: ${req.action}`,
    );
  }
  const url = `${opts.baseUrl ?? defaultBaseUrl()}/v1/privacy/dsr/${req.action}`;
  const { action: _unused, ...payload } = req;
  return callJson<DsrSubmitResponse>(
    url,
    {
      method: "POST",
      headers: headers(opts),
      body: JSON.stringify(payload),
    },
    opts,
  );
}

export async function getDsrStatus(
  requestId: string,
  opts: DsrClientOptions,
): Promise<DsrRequestDetail> {
  const url = `${opts.baseUrl ?? defaultBaseUrl()}/v1/privacy/dsr/${encodeURIComponent(requestId)}/status`;
  return callJson<DsrRequestDetail>(
    url,
    { method: "GET", headers: headers(opts) },
    opts,
  );
}

export interface ListMyDsrsResult {
  items: DsrRequestSummary[];
  next_cursor?: string;
}

export async function listMyDsrs(
  opts: DsrClientOptions & { cursor?: string; limit?: number },
): Promise<ListMyDsrsResult> {
  const params = new URLSearchParams();
  if (opts.cursor) params.set("cursor", opts.cursor);
  if (opts.limit) params.set("limit", String(opts.limit));
  const qs = params.toString();
  const url = `${opts.baseUrl ?? defaultBaseUrl()}/v1/privacy/dsr${qs ? `?${qs}` : ""}`;
  return callJson<ListMyDsrsResult>(
    url,
    { method: "GET", headers: headers(opts) },
    opts,
  );
}

export async function getMyProfile(
  opts: DsrClientOptions,
): Promise<MeProfile> {
  const url = `${opts.baseUrl ?? defaultBaseUrl()}/v1/users/me`;
  return callJson<MeProfile>(
    url,
    { method: "GET", headers: headers(opts) },
    opts,
  );
}

export async function listDataCategories(
  opts: DsrClientOptions,
): Promise<DataCategory[]> {
  const url = `${opts.baseUrl ?? defaultBaseUrl()}/v1/data-categories`;
  return callJson<DataCategory[]>(
    url,
    { method: "GET", headers: headers(opts) },
    opts,
  );
}

/**
 * Indirection used by ReAuthGate so server-side renders never see the token
 * directly. Returns true if the supplied fresh-auth marker is younger than
 * `MFA_FRESH_MAX_AGE_MS` ago.
 *
 * CTRL-AUTH-010: max age is 30 minutes for DSR ops.
 */
export const MFA_FRESH_MAX_AGE_MS = 30 * 60 * 1000;

export function isMfaFresh(
  verifiedAt: number | null,
  now: number = Date.now(),
): boolean {
  if (verifiedAt === null) return false;
  if (verifiedAt > now) return false; // clock skew protection
  return now - verifiedAt < MFA_FRESH_MAX_AGE_MS;
}
