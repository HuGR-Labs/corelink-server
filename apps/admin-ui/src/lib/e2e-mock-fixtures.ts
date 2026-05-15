/**
 * Shared E2E mock fixtures (wt-r3-7).
 *
 * Single source of truth for both the server-side catch-all
 * (`/app/api/v1/[...path]/route.ts`) AND the browser-side playwright
 * `page.route()` interceptor in `tests/e2e/fixtures/api-mocks.ts`.
 *
 * Determinism rules:
 *   - Fixed timestamps for list endpoints so date filters can be asserted.
 *   - Mutations DO update the in-memory state so a request like
 *     "approve op" causes a subsequent "list audit" to reflect the audit row.
 *   - State is module-level — a single Node process per playwright run; each
 *     spec file resets via `__resetMockState()` in beforeEach.
 *
 * Production safety: this module is imported only by the catch-all route
 * (which itself is gated on NEXT_PUBLIC_E2E_TEST_MODE=1) and by the
 * playwright fixture (which never ships to production). The functions are
 * pure and have no side effects on real backends.
 */

import type { AdminOp, AuditEventDetail, AuditPage, Tenant } from "./types";
import type {
  CustomerAuditEvent,
  CustomerBilling,
  CustomerOverview,
  CustomerPat,
  CustomerTeamMember,
  CustomerUsage,
} from "./customer-types";

export interface MockRequest {
  method: string;
  path: string;
  query: Record<string, string>;
  body: unknown;
  headers: Record<string, string>;
}

export interface MockResponse {
  status: number;
  body: unknown;
  contentType?: string;
}

interface MockState {
  tenants: Tenant[];
  auditEvents: AuditPage["rows"];
  auditDetails: Map<string, AuditEventDetail>;
  ops: AdminOp[];
  dsrRequests: Array<{
    request_id: string;
    action: string;
    submitted_at: string;
    status: "pending" | "review" | "approved" | "rejected";
    actor: string;
  }>;
  customer: {
    tenant_id: string;
    tenant_name: string;
    plan: CustomerOverview["plan"];
    audit: CustomerAuditEvent[];
    pats: CustomerPat[];
    team: CustomerTeamMember[];
    byok: CustomerOverview["byok"];
    billing: CustomerBilling;
  };
}

function rfc7807(status: number, title: string, detail: string): MockResponse {
  return {
    status,
    contentType: "application/problem+json",
    body: { type: "about:blank", title, status, detail },
  };
}

function makeTenants(): Tenant[] {
  const regions: Array<Tenant["region"]> = ["us-east", "us-west", "eu-west", "ap-south"];
  const plans: Array<Tenant["plan"]> = ["free", "starter", "team", "enterprise"];
  return Array.from({ length: 25 }, (_, i) => ({
    tenant_id: `tenant_${String(i + 1).padStart(3, "0")}`,
    name: `Tenant ${i + 1}`,
    plan: plans[i % plans.length]!,
    region: regions[i % regions.length]!,
    byok_status: i % 5 === 0 ? "active" : "none",
    created_at: `2026-0${(i % 5) + 1}-01T00:00:00Z`,
  }));
}

function makeAuditEvents(): MockState["auditEvents"] {
  return [
    {
      event_id: "evt_001",
      ts: "2026-05-10T10:00:00Z",
      tenant_id: "tenant_001",
      event_type: "auth.login",
      severity: "info",
      actor: "user_e2e_admin",
      summary: "operator login from 10.0.0.1",
      correlation_id: "corr-001",
    },
    {
      event_id: "evt_002",
      ts: "2026-05-12T11:30:00Z",
      tenant_id: "tenant_002",
      event_type: "byok.cmk_rotated",
      severity: "warn",
      actor: "user_e2e_approver",
      summary: "CMK rotated for tenant_002",
      correlation_id: "corr-002",
    },
    {
      event_id: "evt_003",
      ts: "2026-05-13T09:00:00Z",
      tenant_id: "tenant_001",
      event_type: "dsr.request",
      severity: "info",
      actor: "user_e2e_admin",
      summary: "DSR access submitted",
      correlation_id: "corr-003",
    },
  ];
}

function makeAuditDetail(id: string): AuditEventDetail {
  const summary = state.auditEvents.find((e) => e.event_id === id) ?? state.auditEvents[0]!;
  return {
    ...summary,
    cloudevent: {
      specversion: "1.0",
      id: summary.event_id,
      source: "corelink/admin-ui-e2e",
      type: String(summary.event_type),
      time: summary.ts,
      datacontenttype: "application/json",
      subject: summary.tenant_id,
    },
    payload: { tenant_id: summary.tenant_id, redacted: true },
    merkle_proof: {
      leaf_hash: "00".repeat(32),
      siblings: [
        { hash: "11".repeat(32), position: "left" },
        { hash: "22".repeat(32), position: "right" },
      ],
      expected_root: "ff".repeat(32),
      algorithm: "sha256",
    },
    r2_url: "https://example.invalid/audit-archive/" + summary.event_id,
  };
}

function makeOps(): AdminOp[] {
  return [
    {
      op_id: "op_byok_001",
      op_type: "byok_cmk_rotation",
      requestor: "user_e2e_admin",
      requested_at: "2026-05-13T08:00:00Z",
      status: "awaiting_approval",
      payload: { tenant_id: "tenant_001", new_cmk_id: "cmk_v2" },
      impact_summary: "Rotate CMK for tenant_001 (BYOK).",
      tenant_scope: ["tenant_001"],
      approvals: [],
    },
    {
      op_id: "op_dsr_001",
      op_type: "tenant_data_export",
      requestor: "user_e2e_admin",
      requested_at: "2026-05-13T09:00:00Z",
      status: "awaiting_approval",
      payload: { request_id: "dsr_001", tenant_id: "tenant_001" },
      impact_summary: "Export tenant_001 personal data for DSR access request.",
      tenant_scope: ["tenant_001"],
      approvals: [],
    },
  ];
}

function makeCustomerAudit(): CustomerAuditEvent[] {
  return [
    {
      event_id: "cevt_001",
      ts: "2026-05-10T10:00:00Z",
      event_type: "auth.login",
      severity: "info",
      actor: "user_e2e_admin",
      summary: "signin from 10.0.0.1",
    },
    {
      event_id: "cevt_002",
      ts: "2026-05-12T11:30:00Z",
      event_type: "pat.created",
      severity: "info",
      actor: "user_e2e_admin",
      summary: "created PAT 'ci-runner'",
    },
    {
      event_id: "cevt_003",
      ts: "2026-05-13T09:00:00Z",
      event_type: "byok.cmk_rotated",
      severity: "warn",
      actor: "user_e2e_admin",
      summary: "CMK rotated to cmk_v2",
    },
  ];
}

function makeCustomerPats(): CustomerPat[] {
  return [
    {
      pat_id: "pat_001",
      name: "ci-runner",
      scopes: ["cache:r", "cache:w", "cache:find-missing"],
      created_at: "2026-04-15T09:00:00Z",
      last_used_at: "2026-05-14T22:01:00Z",
    },
    {
      pat_id: "pat_002",
      name: "dashboard-readonly",
      scopes: ["cache:r", "admin:audit"],
      created_at: "2026-03-20T12:00:00Z",
      last_used_at: "2026-05-13T18:42:00Z",
    },
  ];
}

function makeCustomerTeam(): CustomerTeamMember[] {
  return [
    {
      user_id: "user_e2e_admin",
      email: "admin@acme.example",
      role: "Owner",
      joined_at: "2026-01-15T09:00:00Z",
      status: "active",
    },
    {
      user_id: "user_e2e_approver",
      email: "approver@acme.example",
      role: "Admin",
      joined_at: "2026-02-01T09:00:00Z",
      status: "active",
    },
    {
      user_id: "user_e2e_member",
      email: "member@acme.example",
      role: "Developer",
      joined_at: "2026-03-10T09:00:00Z",
      status: "active",
    },
  ];
}

function makeCustomerBilling(): CustomerBilling {
  return {
    status: "active",
    plan: "team",
    current_period_start: "2026-05-01T00:00:00Z",
    current_period_end: "2026-05-31T23:59:59Z",
    amount_due_cents: 19900,
    currency: "usd",
    payment_method: { brand: "visa", last4: "4242", exp_month: 12, exp_year: 2028 },
    invoices: [
      {
        invoice_id: "inv_2026_04",
        issued_at: "2026-04-30T23:00:00Z",
        amount_cents: 19900,
        status: "paid",
        hosted_url: "https://billing.example.invalid/inv_2026_04",
      },
      {
        invoice_id: "inv_2026_03",
        issued_at: "2026-03-30T23:00:00Z",
        amount_cents: 19900,
        status: "paid",
        hosted_url: "https://billing.example.invalid/inv_2026_03",
      },
    ],
  };
}

function freshState(): MockState {
  const events = makeAuditEvents();
  return {
    tenants: makeTenants(),
    auditEvents: events,
    auditDetails: new Map(),
    ops: makeOps(),
    dsrRequests: [
      {
        request_id: "dsr_001",
        action: "access",
        submitted_at: "2026-05-13T09:00:00Z",
        status: "pending",
        actor: "data_subject_001@example.invalid",
      },
    ],
    customer: {
      tenant_id: "tenant_acme",
      tenant_name: "Acme Inc.",
      plan: "team",
      audit: makeCustomerAudit(),
      pats: makeCustomerPats(),
      team: makeCustomerTeam(),
      byok: {
        status: "active",
        cmk_id: "cmk_v2",
        last_rotated_at: "2026-05-13T09:00:00Z",
      },
      billing: makeCustomerBilling(),
    },
  };
}

let state: MockState = freshState();

export function __resetMockState(): void {
  state = freshState();
}

export function getFixtureResponse(req: MockRequest): MockResponse {
  const { method, path, query, body } = req;

  // ----- tenants -----
  if (path === "/v1/admin/tenants" && method === "GET") {
    const q = (query["q"] ?? "").toLowerCase();
    const cursor = query["cursor"] ?? "0";
    const offset = Number.parseInt(cursor, 10) || 0;
    const pageSize = 10;
    const filtered = q
      ? state.tenants.filter(
          (t) => t.tenant_id.toLowerCase().includes(q) || t.name.toLowerCase().includes(q),
        )
      : state.tenants;
    const page = filtered.slice(offset, offset + pageSize);
    return {
      status: 200,
      body: {
        tenants: page,
        next_cursor: offset + pageSize < filtered.length ? String(offset + pageSize) : null,
        total: filtered.length,
      },
    };
  }
  if (path.startsWith("/v1/admin/tenants/") && method === "GET") {
    const id = path.split("/").pop()!;
    const t = state.tenants.find((x) => x.tenant_id === id);
    if (!t) return rfc7807(404, "Not Found", `tenant ${id} not found`);
    return { status: 200, body: t };
  }

  // ----- audit -----
  if (path === "/v1/admin/audit" && method === "GET") {
    const sinceParam = query["since"];
    let rows = state.auditEvents;
    if (sinceParam) {
      const since = Date.parse(sinceParam);
      if (!Number.isNaN(since)) {
        rows = rows.filter((r) => Date.parse(r.ts) >= since);
      }
    }
    const eventTypes = query["event_types"];
    if (eventTypes) {
      const types = new Set(eventTypes.split(","));
      rows = rows.filter((r) => types.has(String(r.event_type)));
    }
    return { status: 200, body: { rows, next_cursor: null } };
  }
  if (path.startsWith("/v1/admin/audit/") && method === "GET") {
    const id = path.split("/").pop()!;
    if (!state.auditDetails.has(id)) state.auditDetails.set(id, makeAuditDetail(id));
    return { status: 200, body: state.auditDetails.get(id) };
  }

  // ----- ops dual-approval -----
  if (path === "/v1/admin/ops" && method === "GET") {
    const status = query["status"];
    const ops = status ? state.ops.filter((o) => o.status === status) : state.ops;
    return { status: 200, body: { ops } };
  }
  if (path.startsWith("/v1/admin/ops/") && path.endsWith("/approve") && method === "POST") {
    const id = path.split("/")[4];
    const op = state.ops.find((o) => o.op_id === id);
    if (!op) return rfc7807(404, "Not Found", "op not found");
    const reason =
      (typeof body === "object" && body !== null && "reason" in (body as Record<string, unknown>)
        ? String((body as Record<string, unknown>)["reason"])
        : "") ||
      req.headers["x-admin-operation-reason"] ||
      "approved via e2e";
    op.approvals.push({
      approver: "user_e2e_approver",
      approved_at: new Date().toISOString(),
      reason,
    });
    if (op.approvals.length >= 2) op.status = "executed";
    // Drop an audit row so the audit-trail spec can assert it.
    state.auditEvents = [
      {
        event_id: `evt_op_${op.op_id}`,
        ts: new Date().toISOString(),
        tenant_id: op.tenant_scope[0] ?? "tenant_001",
        event_type: "admin.op_approved",
        severity: "warn",
        actor: "user_e2e_approver",
        summary: `approved ${op.op_type} ${op.op_id}`,
        correlation_id: `corr-${op.op_id}`,
      },
      ...state.auditEvents,
    ];
    return { status: 200, body: op };
  }
  if (path.startsWith("/v1/admin/ops/") && method === "GET") {
    const id = path.split("/").pop()!;
    const op = state.ops.find((o) => o.op_id === id);
    if (!op) return rfc7807(404, "Not Found", "op not found");
    return { status: 200, body: op };
  }

  // ----- DSR (admin-facing list / approve) -----
  if (path === "/v1/admin/dsr/requests" && method === "GET") {
    return { status: 200, body: { items: state.dsrRequests } };
  }
  if (path.startsWith("/v1/admin/dsr/requests/") && path.endsWith("/approve") && method === "POST") {
    const id = path.split("/")[4];
    const r = state.dsrRequests.find((x) => x.request_id === id);
    if (!r) return rfc7807(404, "Not Found", "DSR not found");
    r.status = "approved";
    state.auditEvents = [
      {
        event_id: `evt_dsr_${r.request_id}`,
        ts: new Date().toISOString(),
        tenant_id: "tenant_001",
        event_type: "dsr.request",
        severity: "info",
        actor: "user_e2e_admin",
        summary: `approved DSR ${r.request_id}`,
        correlation_id: `corr-${r.request_id}`,
      },
      ...state.auditEvents,
    ];
    return { status: 200, body: r };
  }
  if (path.startsWith("/v1/admin/dsr/requests/") && method === "GET") {
    const id = path.split("/").pop()!;
    const r = state.dsrRequests.find((x) => x.request_id === id);
    if (!r) return rfc7807(404, "Not Found", "DSR not found");
    return { status: 200, body: r };
  }

  // ----- customer self-serve (Viewer-minimum, tenant-scoped) -----
  if (path === "/v1/customer/overview" && method === "GET") {
    const c = state.customer;
    const overview: CustomerOverview = {
      tenant_id: c.tenant_id,
      tenant_name: c.tenant_name,
      plan: c.plan,
      usage: {
        period: "2026-05",
        cas_bytes: 4_812_344_321,
        reads: 124_502,
        writes: 8_712,
        quota_bytes: 10_737_418_240,
      },
      billing: {
        status: c.billing.status,
        next_invoice_at: c.billing.current_period_end,
        amount_due_cents: c.billing.amount_due_cents,
        currency: c.billing.currency,
      },
      byok: c.byok,
      recent_activity: c.audit.slice(0, 5),
    };
    return { status: 200, body: overview };
  }

  if (path === "/v1/customer/usage" && method === "GET") {
    const usage: CustomerUsage = {
      period: query["period"] ?? "2026-05",
      cas_bytes: 4_812_344_321,
      reads: 124_502,
      writes: 8_712,
      quota_bytes: 10_737_418_240,
      daily: Array.from({ length: 7 }, (_, i) => ({
        day: `2026-05-${String(8 + i).padStart(2, "0")}`,
        reads: 14_000 + i * 1_200,
        writes: 800 + i * 90,
        cas_bytes: 600_000_000 + i * 30_000_000,
      })),
    };
    return { status: 200, body: usage };
  }

  if (path === "/v1/customer/audit" && method === "GET") {
    let rows = state.customer.audit;
    const sinceParam = query["since"];
    if (sinceParam) {
      const since = Date.parse(sinceParam);
      if (!Number.isNaN(since)) {
        rows = rows.filter((r) => Date.parse(r.ts) >= since);
      }
    }
    const eventTypes = query["event_types"];
    if (eventTypes) {
      const types = new Set(eventTypes.split(","));
      rows = rows.filter((r) => types.has(r.event_type));
    }
    return { status: 200, body: { rows } };
  }

  if (path === "/v1/customer/billing" && method === "GET") {
    return { status: 200, body: state.customer.billing };
  }
  if (path === "/v1/customer/billing/portal" && method === "POST") {
    return {
      status: 200,
      body: { portal_url: "https://billing.example.invalid/portal/session_test" },
    };
  }

  if (path === "/v1/customer/keys" && method === "GET") {
    return {
      status: 200,
      body: { pats: state.customer.pats, byok: state.customer.byok },
    };
  }
  if (path === "/v1/customer/keys" && method === "POST") {
    const b = (body ?? {}) as { name?: string; scopes?: string[] };
    const pat: CustomerPat = {
      pat_id: `pat_${String(state.customer.pats.length + 1).padStart(3, "0")}`,
      name: b.name ?? "unnamed",
      scopes: b.scopes ?? ["cache:r"],
      created_at: new Date().toISOString(),
    };
    state.customer.pats.unshift(pat);
    state.customer.audit.unshift({
      event_id: `cevt_pat_${pat.pat_id}`,
      ts: pat.created_at,
      event_type: "pat.created",
      severity: "info",
      actor: "user_e2e_admin",
      summary: `created PAT '${pat.name}'`,
    });
    return {
      status: 201,
      body: { ...pat, token: `crl_pat_${pat.pat_id}_secret_shown_once` },
    };
  }
  if (path.startsWith("/v1/customer/keys/") && path.endsWith("/revoke") && method === "POST") {
    const segs = path.split("/");
    const id = segs[4]!;
    const pat = state.customer.pats.find((p) => p.pat_id === id);
    if (!pat) return rfc7807(404, "Not Found", `pat ${id} not found`);
    pat.revoked_at = new Date().toISOString();
    state.customer.audit.unshift({
      event_id: `cevt_pat_rev_${pat.pat_id}`,
      ts: pat.revoked_at,
      event_type: "pat.revoked",
      severity: "warn",
      actor: "user_e2e_admin",
      summary: `revoked PAT '${pat.name}'`,
    });
    return { status: 200, body: pat };
  }

  if (path === "/v1/customer/team" && method === "GET") {
    return { status: 200, body: { members: state.customer.team } };
  }
  if (path === "/v1/customer/team/invite" && method === "POST") {
    const b = (body ?? {}) as { email?: string; role?: CustomerTeamMember["role"] };
    if (!b.email) return rfc7807(400, "Bad Request", "email required");
    const m: CustomerTeamMember = {
      user_id: `user_invite_${state.customer.team.length + 1}`,
      email: b.email,
      role: b.role ?? "Developer",
      joined_at: new Date().toISOString(),
      status: "invited",
    };
    state.customer.team.push(m);
    state.customer.audit.unshift({
      event_id: `cevt_invite_${m.user_id}`,
      ts: m.joined_at,
      event_type: "team.invited",
      severity: "info",
      actor: "user_e2e_admin",
      summary: `invited ${b.email} as ${m.role}`,
    });
    return { status: 201, body: m };
  }

  return rfc7807(404, "Not Found", `unmocked ${method} ${path}`);
}
