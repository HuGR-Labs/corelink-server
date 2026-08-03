// WI-S16-005 — typed admin API client.
//
// Privacy + security:
//  - CTRL-CRED-001: session tokens never logged. The fetch helper does not
//    serialize headers anywhere — Authorization is set per-call and dropped.
//  - All mutating dual-approval calls REQUIRE an `X-Admin-Operation-Reason`
//    header; helpers refuse to send the request if reason is missing.
//
// The base URL is read from `NEXT_PUBLIC_CORELINK_API_URL`; tests inject a
// custom `fetch` via the optional `fetchImpl` parameter.

import type {
  AdminOp,
  AuditEventDetail,
  AuditFilter,
  AuditPage,
  ExportResponse,
  OpType,
  Tenant,
} from "./types";
import { withAppBasePath } from "./route-matcher";

export interface AdminClientOptions {
  baseUrl?: string;
  getToken?: () => Promise<string | null>;
  fetchImpl?: typeof fetch;
}

export class AdminClientError extends Error {
  constructor(public readonly status: number, message: string) {
    super(message);
    this.name = "AdminClientError";
  }
}

function resolveDefaultBaseUrl(): string {
  const explicit = typeof process !== "undefined"
    ? process.env?.NEXT_PUBLIC_CORELINK_API_URL
    : undefined;
  // On the server side a relative `/api` is not a valid fetch URL — Next 15
  // (Node runtime) requires absolute. When running in dev/test we synthesize
  // an absolute origin from PORT so SSR calls land on our own catch-all
  // mock. On the client side `window.fetch` happily accepts relative paths.
  //
  // Any SAME-ORIGIN fallback must carry the app's `/corelink` basePath: the
  // catch-all mock lives at `/corelink/api/v1/**`, and the bare `/api` form
  // resolves against the apex `humangr.com` — the hugr-site MARKETING app,
  // not this one. `withAppBasePath` is a no-op on an absolute
  // `https://corelink-api…` value (which is what prod injects), so this is
  // latent today and stays correct if the env var is ever dropped.
  if (typeof window !== "undefined") return withAppBasePath(explicit ?? "/api");
  if (explicit && !explicit.startsWith("/")) return explicit;
  const port = (typeof process !== "undefined" && process.env?.PORT) || "3000";
  const relPath = withAppBasePath(explicit ?? "/api");
  return `http://127.0.0.1:${port}${relPath}`;
}

const DEFAULT_BASE_URL = resolveDefaultBaseUrl();

function buildQuery(filter: AuditFilter): string {
  const params = new URLSearchParams();
  if (filter.tenant_id) params.set("tenant_id", filter.tenant_id);
  if (filter.event_types?.length) params.set("event_types", filter.event_types.join(","));
  if (filter.since) params.set("since", filter.since);
  if (filter.until) params.set("until", filter.until);
  if (filter.severity?.length) params.set("severity", filter.severity.join(","));
  if (filter.correlation_id) params.set("correlation_id", filter.correlation_id);
  if (filter.cursor) params.set("cursor", filter.cursor);
  const q = params.toString();
  return q ? `?${q}` : "";
}

export class AdminClient {
  private readonly baseUrl: string;
  private readonly getToken: () => Promise<string | null>;
  private readonly fetchImpl: typeof fetch;

  constructor(opts: AdminClientOptions = {}) {
    this.baseUrl = opts.baseUrl ?? DEFAULT_BASE_URL;
    this.getToken = opts.getToken ?? (async () => null);
    this.fetchImpl = opts.fetchImpl ?? globalThis.fetch.bind(globalThis);
  }

  private async request<T>(
    path: string,
    init: RequestInit & { reason?: string } = {},
  ): Promise<T> {
    const headers = new Headers(init.headers ?? {});
    headers.set("Accept", "application/json");
    if (init.body && !headers.has("Content-Type")) {
      headers.set("Content-Type", "application/json");
    }

    const token = await this.getToken();
    if (token) headers.set("Authorization", `Bearer ${token}`);

    const method = (init.method ?? "GET").toUpperCase();
    const isMutation = method !== "GET" && method !== "HEAD";
    if (isMutation && path.includes("/admin/ops")) {
      if (!init.reason || !init.reason.trim()) {
        throw new AdminClientError(0, "X-Admin-Operation-Reason is required for ops mutations");
      }
      headers.set("X-Admin-Operation-Reason", init.reason.trim());
    }

    const res = await this.fetchImpl(`${this.baseUrl}${path}`, { ...init, headers });
    if (!res.ok) {
      let detail = "";
      try {
        detail = await res.text();
      } catch {
        // ignore body parse — surface status only.
      }
      throw new AdminClientError(res.status, `${method} ${path} -> ${res.status} ${detail}`);
    }
    if (res.status === 204) return undefined as T;
    return (await res.json()) as T;
  }

  // ---- audit -----------------------------------------------------------
  async listAuditEvents(filter: AuditFilter): Promise<AuditPage> {
    return this.request<AuditPage>(`/v1/admin/audit${buildQuery(filter)}`);
  }

  async getAuditEvent(eventId: string): Promise<AuditEventDetail> {
    return this.request<AuditEventDetail>(`/v1/admin/audit/${encodeURIComponent(eventId)}`);
  }

  async exportAudit(filter: AuditFilter, format: "csv" | "json"): Promise<ExportResponse> {
    return this.request<ExportResponse>(`/v1/admin/audit/export`, {
      method: "POST",
      body: JSON.stringify({ filter, format }),
    });
  }

  // ---- dual-approval ops ----------------------------------------------
  async listOps(status?: string): Promise<AdminOp[]> {
    const q = status ? `?status=${encodeURIComponent(status)}` : "";
    const result = await this.request<{ ops: AdminOp[] }>(`/v1/admin/ops${q}`);
    return result.ops;
  }

  async getOp(opId: string): Promise<AdminOp> {
    return this.request<AdminOp>(`/v1/admin/ops/${encodeURIComponent(opId)}`);
  }

  async submitOp(opType: OpType, payload: Record<string, unknown>, reason: string): Promise<AdminOp> {
    return this.request<AdminOp>(`/v1/admin/ops`, {
      method: "POST",
      body: JSON.stringify({ op_type: opType, payload }),
      reason,
    });
  }

  async approveOp(opId: string, reason: string): Promise<AdminOp> {
    return this.request<AdminOp>(`/v1/admin/ops/${encodeURIComponent(opId)}/approve`, {
      method: "POST",
      reason,
    });
  }

  async rejectOp(opId: string, reason: string): Promise<AdminOp> {
    return this.request<AdminOp>(`/v1/admin/ops/${encodeURIComponent(opId)}/reject`, {
      method: "POST",
      reason,
    });
  }

  // ---- tenants ---------------------------------------------------------
  async searchTenants(query: string): Promise<Tenant[]> {
    const q = query ? `?q=${encodeURIComponent(query)}` : "";
    const result = await this.request<{ tenants: Tenant[] }>(`/v1/admin/tenants${q}`);
    return result.tenants;
  }

  async getTenant(tenantId: string): Promise<Tenant> {
    return this.request<Tenant>(`/v1/admin/tenants/${encodeURIComponent(tenantId)}`);
  }
}

export const adminClient = new AdminClient();
