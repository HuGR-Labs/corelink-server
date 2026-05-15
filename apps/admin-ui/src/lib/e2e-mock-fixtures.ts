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

  // ----- Customer Billing Portal session (wt/r-prep-stripe-portal) -----
  // POST /v1/customer/billing/portal-session — mirrors Stripe Billing
  // Portal API. Returns a fresh single-use HTTPS URL on every call so
  // E2E can assert no-cache behaviour.
  if (path === "/v1/customer/billing/portal-session" && method === "POST") {
    const rb =
      typeof body === "object" && body !== null
        ? (body as { return_url?: string; tenant_id?: string })
        : {};
    if (!rb.return_url || !rb.return_url.startsWith("https://")) {
      return rfc7807(400, "Bad Request", "return_url must be HTTPS");
    }
    if (!rb.tenant_id) {
      return rfc7807(400, "Bad Request", "tenant_id required");
    }
    // Monotonic counter so the URL changes per call.
    portalSessionSeq += 1;
    const id = `bps_e2e_${portalSessionSeq.toString(16).padStart(8, "0")}`;
    return {
      status: 200,
      body: {
        portal_url: `https://billing.stripe.com/p/session/${id}`,
        session_id: id,
        // 5-minute expiry matches Stripe's documented contract.
        expires_at_unix: Math.floor(Date.now() / 1000) + 300,
      },
    };
  }

  return rfc7807(404, "Not Found", `unmocked ${method} ${path}`);
}

// Module-level counter so each portal-session call returns a unique
// URL — Stripe's portal URLs are single-use, the fake mirrors that.
let portalSessionSeq = 0;
