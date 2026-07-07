// Customer self-serve API client.
//
// Hits the `/v1/customer/*` surface — tenant-scoped, Viewer-minimum. The
// scope is always the *caller's own* tenant; there is no `tenant_id` path
// parameter (the backend infers tenant from the session). This is the key
// boundary distinction vs. the operator `AdminClient` which can target any
// tenant.
//
// In E2E mode every endpoint is mocked by `src/lib/e2e-mock-fixtures.ts`
// behind the catch-all route at `src/app/api/v1/[...path]/route.ts`.

import type {
  CustomerAuditEvent,
  CustomerBilling,
  CustomerByokConfig,
  CustomerDollarCeiling,
  CustomerOverview,
  CustomerPat,
  CustomerRunnerEntitlement,
  CustomerRunnerRun,
  CustomerTeamMember,
  CustomerUsage,
  CustomerWorkspace,
} from "./customer-types";

/**
 * Thrown by client methods whose backend endpoint is not wired yet (DATA-TRUTH
 * `[not-wired]`). Screens catch this and render a teaching EmptyState — they
 * never fabricate a value. Each carries the backend WP that will close it.
 */
export class NotWiredError extends Error {
  constructor(public readonly backendWp: string) {
    super(`not wired yet (${backendWp})`);
    this.name = "NotWiredError";
  }
}

export interface CustomerClientOptions {
  baseUrl?: string;
  fetchImpl?: typeof fetch;
  /**
   * Async supplier of the caller's session token (e.g. Clerk's
   * `useAuth().getToken`). When provided and it resolves to a non-null
   * string, every request carries `Authorization: Bearer <token>`.
   * When omitted (or it resolves to null) NO Authorization header is sent —
   * this keeps the E2E mock mode (catch-all `/api` route) byte-identical.
   */
  getToken?: () => Promise<string | null>;
}

export class CustomerClientError extends Error {
  constructor(public readonly status: number, message: string) {
    super(message);
    this.name = "CustomerClientError";
  }
}

// Prefers NEXT_PUBLIC_CORELINK_API_URL when set. In production the admin-ui
// build sets NEXT_PUBLIC_CORELINK_API_URL=https://corelink-api.humangr.com at
// build time (NEXT_PUBLIC_* is inlined into the client bundle), so the browser
// talks to the real API origin; without it we fall back to the same-origin
// `/api` path (E2E mock catch-all / local dev).
function resolveBaseUrl(): string {
  const explicit =
    typeof process !== "undefined" ? process.env?.NEXT_PUBLIC_CORELINK_API_URL : undefined;
  if (typeof window !== "undefined") return explicit ?? "/api";
  if (explicit && !explicit.startsWith("/")) return explicit;
  const port = (typeof process !== "undefined" && process.env?.PORT) || "3000";
  const relPath = explicit ?? "/api";
  return `http://127.0.0.1:${port}${relPath}`;
}

export class CustomerClient {
  private readonly baseUrl: string;
  private readonly fetchImpl: typeof fetch;
  private readonly getToken?: () => Promise<string | null>;

  constructor(opts: CustomerClientOptions = {}) {
    this.baseUrl = opts.baseUrl ?? resolveBaseUrl();
    this.fetchImpl = opts.fetchImpl ?? globalThis.fetch.bind(globalThis);
    this.getToken = opts.getToken;
  }

  private async request<T>(path: string, init: RequestInit = {}): Promise<T> {
    // In E2E test mode the endpoints are served by the deterministic mock
    // (no auth required) and there is NO real Clerk session, so calling the
    // injected `useAuth().getToken()` can stall — leaving the fetch pending and
    // the screen stuck on its loading Skeleton forever (the admin client sidesteps
    // this by defaulting getToken to `async () => null`). Skip the token in E2E.
    // Inert in production: NEXT_PUBLIC_E2E_TEST_MODE is never set there.
    // Canonical Next form: bare `process.env.NEXT_PUBLIC_*` dot-access, which Next
    // statically REPLACES with the literal in the client bundle. Do NOT guard with
    // `typeof process` — Next does not define `process` as a runtime object in the
    // browser, so that guard would short-circuit to false client-side.
    const isE2E = process.env.NEXT_PUBLIC_E2E_TEST_MODE === "1";
    const token = isE2E ? null : this.getToken ? await this.getToken() : null;
    const authHeaders: Record<string, string> =
      token != null ? { authorization: `Bearer ${token}` } : {};
    const res = await this.fetchImpl(`${this.baseUrl}${path}`, {
      ...init,
      headers: { "content-type": "application/json", ...authHeaders, ...(init.headers ?? {}) },
    });
    if (!res.ok) {
      throw new CustomerClientError(res.status, `customer api error (${res.status})`);
    }
    return (await res.json()) as T;
  }

  async getOverview(): Promise<CustomerOverview> {
    return this.request<CustomerOverview>("/v1/customer/overview");
  }

  async getUsage(period?: string): Promise<CustomerUsage> {
    const q = period ? `?period=${encodeURIComponent(period)}` : "";
    return this.request<CustomerUsage>(`/v1/customer/usage${q}`);
  }

  /**
   * List tenant-scoped audit events. The wire params are the backend's
   * CANONICAL names: `from` (ISO-8601 lower bound), `to` (ISO-8601 upper bound),
   * and `kind` (comma-separated event-type codes) — see `AuditQuery` in
   * `crates/corelink-container/src/routes/customer.rs`. The pre-2026-07 client
   * sent `since` / `event_types`, which the backend silently DROPPED (the struct
   * only deserializes `from`/`to`/`kind`), so date + event-type filters never
   * reached D1. `to` is currently accepted-but-ignored server-side (parity with
   * the Rust handler); callers still apply the upper bound client-side.
   */
  async listAudit(
    filter: { from?: string; to?: string; kind?: string[] } = {},
  ): Promise<{ rows: CustomerAuditEvent[] }> {
    const params = new URLSearchParams();
    if (filter.from) params.set("from", filter.from);
    if (filter.to) params.set("to", filter.to);
    if (filter.kind?.length) params.set("kind", filter.kind.join(","));
    const q = params.toString();
    return this.request<{ rows: CustomerAuditEvent[] }>(
      `/v1/customer/audit${q ? `?${q}` : ""}`,
    );
  }

  async getBilling(): Promise<CustomerBilling> {
    return this.request<CustomerBilling>("/v1/customer/billing");
  }

  async startBillingPortal(): Promise<{ portal_url: string }> {
    return this.request<{ portal_url: string }>("/v1/customer/billing/portal", {
      method: "POST",
    });
  }

  async listKeys(): Promise<{ pats: CustomerPat[]; byok: CustomerOverview["byok"] }> {
    return this.request<{ pats: CustomerPat[]; byok: CustomerOverview["byok"] }>(
      "/v1/customer/keys",
    );
  }

  async createPat(input: { name: string; scopes: string[] }): Promise<CustomerPat & { token?: string }> {
    return this.request<CustomerPat & { token?: string }>("/v1/customer/keys", {
      method: "POST",
      body: JSON.stringify(input),
    });
  }

  async revokePat(patId: string): Promise<CustomerPat> {
    return this.request<CustomerPat>(`/v1/customer/keys/${patId}/revoke`, { method: "POST" });
  }

  async listTeam(): Promise<{ members: CustomerTeamMember[] }> {
    return this.request<{ members: CustomerTeamMember[] }>("/v1/customer/team");
  }

  async inviteTeam(input: {
    email: string;
    role: CustomerTeamMember["role"];
  }): Promise<CustomerTeamMember> {
    return this.request<CustomerTeamMember>("/v1/customer/team/invite", {
      method: "POST",
      body: JSON.stringify(input),
    });
  }

  /** [live] Remove a member (flips seat to removed AND revokes their PATs). */
  async removeTeamMember(userId: string): Promise<{ member: CustomerTeamMember; revoked_pats: number }> {
    return this.request<{ member: CustomerTeamMember; revoked_pats: number }>(
      `/v1/customer/team/${encodeURIComponent(userId)}`,
      { method: "DELETE" },
    );
  }

  /** [live] GDPR self-erasure of the whole tenant (Clerk-session only; MFA-gated upstream). */
  async deleteAccount(): Promise<{ request_id: string }> {
    return this.request<{ request_id: string }>("/v1/customer/account/delete", { method: "POST" });
  }

  // ── [not-wired] reads — throw NotWiredError so screens teach, never fake. ──

  /** [not-wired → BE-7] Spend controls. */
  async getDollarCeiling(): Promise<CustomerDollarCeiling> {
    throw new NotWiredError("BE-7 dollar-ceiling read/update");
  }

  /** [not-wired → BE-8] BYOK self-serve config (status is on overview/keys; config here). */
  async getByokConfig(): Promise<CustomerByokConfig> {
    throw new NotWiredError("BE-8 BYOK self-serve");
  }

  /** [live] Runner entitlement + consumption + repo allowlist (BE-10). */
  async getRunnerEntitlement(): Promise<CustomerRunnerEntitlement> {
    return this.request<CustomerRunnerEntitlement>("/v1/customer/runners/entitlement");
  }

  /** [live] Recent runner runs (BE-10). Empty until a runs table exists server-side. */
  async listRunnerRuns(): Promise<{ runs: CustomerRunnerRun[] }> {
    return this.request<{ runs: CustomerRunnerRun[] }>("/v1/customer/runners/runs");
  }

  /** [live] Workspace snapshots (BE-11). */
  async listWorkspaces(): Promise<{ workspaces: CustomerWorkspace[] }> {
    return this.request<{ workspaces: CustomerWorkspace[] }>("/v1/customer/workspaces");
  }

  /** [live] Create a workspace snapshot (BE-11). */
  async createWorkspace(input: { name: string }): Promise<CustomerWorkspace> {
    return this.request<CustomerWorkspace>("/v1/customer/workspaces", {
      method: "POST",
      body: JSON.stringify(input),
    });
  }

  /** [live] Delete a workspace snapshot (tenant-scoped, idempotent) (BE-11). */
  async deleteWorkspace(workspaceId: string): Promise<{ ok: boolean }> {
    return this.request<{ ok: boolean }>(
      `/v1/customer/workspaces/${encodeURIComponent(workspaceId)}`,
      { method: "DELETE" },
    );
  }

  /** [live] Toggle pin on a workspace snapshot (BE-11). */
  async pinWorkspace(workspaceId: string): Promise<CustomerWorkspace> {
    return this.request<CustomerWorkspace>(
      `/v1/customer/workspaces/${encodeURIComponent(workspaceId)}/pin`,
      { method: "POST" },
    );
  }
}
