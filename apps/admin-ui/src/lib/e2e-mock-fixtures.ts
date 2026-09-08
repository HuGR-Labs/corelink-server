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
 *     an admin DSR mutation causes a subsequent "list audit" to reflect the audit row.
 *   - State is a `globalThis`-backed singleton (see `mockGlobals()` below) — a
 *     single Node process per playwright run, and the global anchor keeps the
 *     state alive across Next.js dev route recompiles (a plain module-level
 *     `let` gets reset when the dev server first compiles a newly-visited
 *     route). Each test resets to a pristine fixture via `POST /v1/_e2e/reset`
 *     (→ `__resetMockState()`), wired in the shared Playwright `test` fixture's
 *     beforeEach (`tests/e2e/fixtures/test.ts`).
 *
 * Production safety: this module is imported only by the catch-all route
 * (which itself is gated on NEXT_PUBLIC_E2E_TEST_MODE=1) and by the
 * playwright fixture (which never ships to production). The functions are
 * pure and have no side effects on real backends.
 */

import type { AdminOp } from "./types";
import type {
  CustomerOverview,
  CustomerPat,
  CustomerTeamMember,
  CustomerUsage,
} from "./customer-types";
import { freshState, makeAuditDetail, type MockState } from "./e2e-mock-fixture-state";

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

function rfc7807(status: number, title: string, detail: string): MockResponse {
  return {
    status,
    contentType: "application/problem+json",
    body: { type: "about:blank", title, status, detail },
  };
}

interface E2EIdentity {
  sub: string;
  tenant_id: string;
  role: "user" | "admin" | "approver";
  mfaAt?: number;
}

function parseIdentity(raw: string | undefined): E2EIdentity | null {
  if (!raw) return null;
  try {
    const parsed = JSON.parse(decodeURIComponent(raw)) as Partial<E2EIdentity>;
    if (
      typeof parsed.sub !== "string" ||
      typeof parsed.tenant_id !== "string" ||
      (parsed.role !== "user" && parsed.role !== "admin" && parsed.role !== "approver")
    ) return null;
    return parsed as E2EIdentity;
  } catch {
    return null;
  }
}

function authFromHeaders(headers: Record<string, string>): E2EIdentity | null {
  const bearer = headers.authorization?.match(/^Bearer\s+e2e:(.+)$/i)?.[1];
  const tokenIdentity = parseIdentity(bearer);
  if (tokenIdentity) return tokenIdentity;
  const raw = headers.cookie?.match(/(?:^|;\s*)__corelink_e2e_session=([^;]+)/)?.[1];
  return parseIdentity(raw);
}

function mfaIsFresh(identity: E2EIdentity, headers: Record<string, string>): boolean {
  const override = headers.cookie?.match(/(?:^|;\s*)__corelink_e2e_mfa=([^;]+)/)?.[1];
  const at = override ? Number(override) : identity.mfaAt;
  return typeof at === "number" && Number.isFinite(at) &&
    Date.now() - at * 1000 <= 30 * 60 * 1000 && Date.now() >= at * 1000;
}

// ─── Persistent mock state (globalThis-backed singleton) ─────────────────────
//
// The mock's mutable state MUST survive across sequential HTTP requests within a
// Playwright run. A plain module-level `let state` does NOT: the Next.js dev
// server re-evaluates shared lib modules when it compiles a not-yet-visited
// route (e.g. the first navigation to /admin/audit after a mutation), which
// re-runs the module top-level and silently resets `state` — wiping the very
// mutation the next assertion depends on. The symptom was "audit row never
// appears", flaky-green only on retry (retry = warm server, no recompile).
// Anchoring the state on `globalThis` (the standard
// Next.js dev-singleton pattern) makes it a true per-process singleton that
// survives module re-evaluation, so a mutation is observed by every later
// request until an explicit reset. `portalSessionSeq` lives here too so its
// monotonic uniqueness also survives a recompile.
interface E2EMockGlobals {
  state: MockState;
  portalSessionSeq: number;
}

const GLOBAL_KEY = "__corelinkE2EMock__" as const;
const globalRef = globalThis as typeof globalThis & {
  [GLOBAL_KEY]?: E2EMockGlobals;
};

function mockGlobals(): E2EMockGlobals {
  if (!globalRef[GLOBAL_KEY]) {
    globalRef[GLOBAL_KEY] = { state: freshState(), portalSessionSeq: 0 };
  }
  return globalRef[GLOBAL_KEY];
}

export function __resetMockState(): void {
  globalRef[GLOBAL_KEY] = { state: freshState(), portalSessionSeq: 0 };
}

export function getFixtureResponse(req: MockRequest): MockResponse {
  const { method, path, query, body, headers } = req;
  const g = mockGlobals();
  const state = g.state;

  // ----- test-only state reset (per-test isolation) -----
  // Gated behind the same NEXT_PUBLIC_E2E_TEST_MODE=1 catch-all that mounts
  // this mock; the Playwright fixture hits it in a beforeEach so each test
  // starts from a pristine deterministic fixture (no cross-test leakage now
  // that state is a persistent singleton).
  if (path === "/v1/_e2e/reset" && method === "POST") {
    __resetMockState();
    return { status: 200, body: { ok: true } };
  }

  // A step-up is a server-validated transition. The browser writes the
  // returned timestamp to its short-lived E2E MFA cookie; subsequent approval
  // requests are checked here again, so disabling the button cannot bypass it.
  if (path === "/v1/_e2e/mfa/step-up" && method === "POST") {
    const identity = authFromHeaders(headers);
    if (!identity) return rfc7807(401, "Unauthorized", "operator identity required");
    if (identity.role !== "admin" && identity.role !== "approver") {
      return rfc7807(403, "Forbidden", "operator role required");
    }
    return { status: 200, body: { mfa_verified_at: Math.floor(Date.now() / 1000) } };
  }

  const identity = authFromHeaders(headers);

  // ----- tenants -----
  if (path === "/v1/admin/tenants" && method === "GET") {
    if (!identity || (identity.role !== "admin" && identity.role !== "approver")) {
      return rfc7807(403, "Forbidden", "operator role required");
    }
    const q = (query["q"] ?? "").toLowerCase();
    const cursor = query["cursor"] ?? "0";
    const offset = Number.parseInt(cursor, 10) || 0;
    const pageSize = 10;
    const scoped = state.tenants.filter((t) => t.tenant_id === identity.tenant_id);
    const filtered = q
      ? scoped.filter(
          (t) => t.tenant_id.toLowerCase().includes(q) || t.name.toLowerCase().includes(q),
        )
      : scoped;
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
    if (!identity || (identity.role !== "admin" && identity.role !== "approver")) {
      return rfc7807(403, "Forbidden", "operator role required");
    }
    const id = path.split("/").pop()!;
    if (id !== identity.tenant_id) return rfc7807(404, "Not Found", `tenant ${id} not found`);
    const t = state.tenants.find((x) => x.tenant_id === id);
    if (!t) return rfc7807(404, "Not Found", `tenant ${id} not found`);
    return { status: 200, body: t };
  }

  // ----- audit -----
  if (path === "/v1/admin/audit" && method === "GET") {
    if (!identity || (identity.role !== "admin" && identity.role !== "approver")) {
      return rfc7807(403, "Forbidden", "operator role required");
    }
    const sinceParam = query["since"];
    let rows = state.auditEvents.filter((row) => row.tenant_id === identity.tenant_id);
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
    if (!identity || (identity.role !== "admin" && identity.role !== "approver")) {
      return rfc7807(403, "Forbidden", "operator role required");
    }
    const id = path.split("/").pop()!;
    const event = state.auditEvents.find((row) => row.event_id === id);
    if (!event || event.tenant_id !== identity.tenant_id) {
      return rfc7807(404, "Not Found", "audit event not found");
    }
    if (!state.auditDetails.has(id)) state.auditDetails.set(id, makeAuditDetail(id, state));
    return { status: 200, body: state.auditDetails.get(id) };
  }

  // ----- dual-approval operations -----
  if (path === "/v1/admin/ops" && method === "GET") {
    if (!identity || (identity.role !== "admin" && identity.role !== "approver")) {
      return rfc7807(403, "Forbidden", "operator role required");
    }
    const requestedStatus = query["status"];
    const ops = requestedStatus
      ? state.ops.filter((op) => op.status === requestedStatus)
      : state.ops;
    const scoped = ops.filter((op) => op.tenant_scope.includes(identity.tenant_id));
    return { status: 200, body: { ops: scoped } };
  }
  if (path === "/v1/admin/ops" && method === "POST") {
    if (!identity || (identity.role !== "admin" && identity.role !== "approver")) {
      return rfc7807(403, "Forbidden", "operator role required");
    }
    if (!mfaIsFresh(identity, headers)) return rfc7807(403, "Forbidden", "fresh MFA required");
    const b = (body ?? {}) as { op_type?: string; payload?: Record<string, unknown> };
    const payload = b.payload ?? {};
    const allowedTypes: AdminOp["op_type"][] = [
      "tenant_data_export",
      "byok_cmk_rotation",
      "tenant_account_deletion",
      "data_residency_change",
    ];
    if (!b.op_type || !allowedTypes.includes(b.op_type as AdminOp["op_type"]) || typeof payload.tenant_id !== "string") {
      return rfc7807(400, "Bad Request", "operation type and payload required");
    }
    if (payload.tenant_id !== identity.tenant_id) {
      return rfc7807(404, "Not Found", "tenant not found");
    }
    const op = {
      op_id: `op_e2e_${state.ops.length + 1}`,
      op_type: b.op_type as AdminOp["op_type"],
      requestor: identity.sub,
      requested_at: new Date().toISOString(),
      status: "awaiting_approval" as const,
      payload,
      impact_summary: `Sensitive operation for ${identity.tenant_id}.`,
      tenant_scope: [identity.tenant_id],
      approvals: [],
    } satisfies AdminOp;
    state.ops.push(op);
    return { status: 201, body: op };
  }
  if (path.startsWith("/v1/admin/ops/") && path.endsWith("/approve") && method === "POST") {
    const opId = path.split("/")[4];
    const op = state.ops.find((candidate) => candidate.op_id === opId);
    if (!op) return rfc7807(404, "Not Found", "operation not found");
    const actor = identity?.sub;
    const reason = headers["x-admin-operation-reason"]?.trim();
    if (!identity || !actor) return rfc7807(401, "Unauthorized", "operator identity required");
    if (identity.role !== "admin" && identity.role !== "approver") return rfc7807(403, "Forbidden", "approver role required");
    if (!mfaIsFresh(identity, headers)) return rfc7807(403, "Forbidden", "fresh MFA required");
    if (!op.tenant_scope.includes(identity.tenant_id)) return rfc7807(404, "Not Found", "operation not found");
    if (!reason) return rfc7807(400, "Bad Request", "operation reason required");
    if (actor === op.requestor) {
      return rfc7807(409, "Conflict", "requestor cannot approve own operation");
    }
    if (op.approvals.some((approval) => approval.approver === actor)) {
      return rfc7807(409, "Conflict", "operator already approved operation");
    }
    op.approvals.push({ approver: actor, approved_at: new Date().toISOString(), reason });
    op.status = op.approvals.length >= 2 ? "executed" : "approved";
    state.auditEvents.unshift({
      event_id: `evt_op_${op.op_id}_${op.approvals.length}`,
      ts: new Date().toISOString(),
      tenant_id: op.tenant_scope[0] ?? "tenant_001",
      event_type: "admin.op_approved",
      severity: "warn",
      actor,
      summary: `approved ${op.op_type} ${op.op_id}`,
      correlation_id: `corr-${op.op_id}`,
    });
    return { status: 200, body: op };
  }
  if (path.startsWith("/v1/admin/ops/") && path.endsWith("/reject") && method === "POST") {
    const opId = path.split("/")[4];
    const op = state.ops.find((candidate) => candidate.op_id === opId);
    if (!op) return rfc7807(404, "Not Found", "operation not found");
    const actor = identity?.sub;
    const reason = headers["x-admin-operation-reason"]?.trim();
    if (!identity || !actor) return rfc7807(401, "Unauthorized", "operator identity required");
    if (identity.role !== "admin" && identity.role !== "approver") return rfc7807(403, "Forbidden", "approver role required");
    if (!mfaIsFresh(identity, headers)) return rfc7807(403, "Forbidden", "fresh MFA required");
    if (!op.tenant_scope.includes(identity.tenant_id)) return rfc7807(404, "Not Found", "operation not found");
    if (!reason) return rfc7807(400, "Bad Request", "operation reason required");
    if (actor === op.requestor) return rfc7807(409, "Conflict", "requestor cannot reject own operation");
    if (op.status === "rejected" || op.status === "executed") return rfc7807(409, "Conflict", "operation is terminal");
    op.rejection = { rejector: actor, rejected_at: new Date().toISOString(), reason };
    op.status = "rejected";
    state.auditEvents.unshift({
      event_id: `evt_op_${op.op_id}_rejected`,
      ts: op.rejection.rejected_at,
      tenant_id: identity.tenant_id,
      event_type: "admin.op_rejected",
      severity: "warn",
      actor,
      summary: `rejected ${op.op_type} ${op.op_id}`,
      correlation_id: `corr-${op.op_id}`,
    });
    return { status: 200, body: op };
  }
  if (path.startsWith("/v1/admin/ops/") && method === "GET") {
    const opId = path.split("/").pop()!;
    const op = state.ops.find((candidate) => candidate.op_id === opId);
    if (op && (!identity || (identity.role !== "admin" && identity.role !== "approver") || !op.tenant_scope.includes(identity.tenant_id))) {
      return rfc7807(404, "Not Found", "operation not found");
    }
    return op ? { status: 200, body: op } : rfc7807(404, "Not Found", "operation not found");
  }

  // ----- customer DSR action flow (legacy B-069 browser journey) -----
  // These responses are test-only and deliberately use the same synthetic
  // state as the admin queue. Production cannot reach this branch because the
  // catch-all route is double-gated on E2E mode and non-production runtime.
  if (path === "/v1/users/me" && method === "GET") {
    return {
      status: 200,
      body: { email: "user@acme.example", name: "E2E User", language: "en" },
    };
  }
  if (path === "/v1/data-categories" && method === "GET") {
    return {
      status: 200,
      body: [
        { id: "profile", label: "Profile" },
        { id: "activity", label: "Activity" },
      ],
    };
  }
  if (path.startsWith("/v1/privacy/dsr/") && method === "POST") {
    if (!identity) return rfc7807(401, "Unauthorized", "authenticated user required");
    if (identity.role !== "user" && identity.role !== "admin" && identity.role !== "approver") {
      return rfc7807(403, "Forbidden", "valid tenant role required");
    }
    if (!mfaIsFresh(identity, headers)) return rfc7807(403, "Forbidden", "fresh MFA required");
    const action = path.split("/").pop() ?? "access";
    const request_id = `dsr_e2e_${state.dsrRequests.length + 1}`;
    const submitted_at = new Date().toISOString();
    const sla_deadline = new Date(Date.now() + 30 * 24 * 3600 * 1000).toISOString();
    state.dsrRequests.push({
      request_id,
      action,
      submitted_at,
      status: "pending",
      actor: identity.sub,
    });
    return {
      status: 201,
      body: {
        request_id,
        action,
        jurisdiction: "gdpr",
        sla_deadline,
        jwt_receipt: `e2e.${Buffer.from(JSON.stringify({
          request_id,
          action,
          jurisdiction: "gdpr",
          sla_deadline,
        })).toString("base64url")}.signature`,
      },
    };
  }
  if (path === "/v1/privacy/dsr" && method === "GET") {
    if (!identity) return rfc7807(401, "Unauthorized", "authenticated user required");
    return {
      status: 200,
      body: {
        items: state.dsrRequests.filter((r) => r.actor === identity.sub).map((r) => ({
          request_id: r.request_id,
          action: r.action,
          status: r.status,
          submitted_at: r.submitted_at,
          sla_deadline: new Date(Date.parse(r.submitted_at) + 30 * 24 * 3600 * 1000).toISOString(),
          jurisdiction: "gdpr",
        })),
        next_cursor: null,
      },
    };
  }
  if (path.startsWith("/v1/privacy/dsr/") && path.endsWith("/status") && method === "GET") {
    if (!identity) return rfc7807(401, "Unauthorized", "authenticated user required");
    const request_id = path.split("/").at(-2) ?? "";
    const found = state.dsrRequests.find((r) => r.request_id === request_id);
    if (!found) return rfc7807(404, "Not Found", "DSR not found");
    if (found.actor !== identity.sub) return rfc7807(404, "Not Found", "DSR not found");
    return {
      status: 200,
      body: {
        request_id,
        action: found.action,
        status: found.status,
        submitted_at: found.submitted_at,
        sla_deadline: new Date(Date.parse(found.submitted_at) + 30 * 24 * 3600 * 1000).toISOString(),
        jurisdiction: "gdpr",
        timeline: [{ at: found.submitted_at, to: found.status }],
      },
    };
  }

  // ----- customer audit-chain visualization (wt/r-prep-audit-chain-viz) -----
  if (path === "/v1/customer/audit/chain/head" && method === "GET") {
    // Deterministic head digest so the Playwright assertion can lock it.
    return {
      status: 200,
      body: {
        head_digest: "ab".repeat(32),
        total_events: 3,
        last_updated: "2026-05-14T12:00:00Z",
        algorithm: "blake3",
      },
    };
  }
  if (path === "/v1/customer/audit/events" && method === "GET") {
    // Each leaf carries a 32-byte hex digest; tests use a single-leaf
    // trivial proof (siblings=[], expected_root==leaf_hash) so the WASM
    // verifier isn't required to assert the "valid" path.
    return {
      status: 200,
      body: {
        rows: [
          {
            event_id: "cevt_001",
            ts: "2026-05-14T11:00:00Z",
            event_type: "cas.put",
            audit_chain_seq: 0,
            leaf_digest: "aa".repeat(32),
          },
          {
            event_id: "cevt_002",
            ts: "2026-05-14T11:15:00Z",
            event_type: "cas.get",
            audit_chain_seq: 1,
            leaf_digest: "bb".repeat(32),
          },
          {
            event_id: "cevt_003",
            ts: "2026-05-14T12:00:00Z",
            event_type: "ac.lookup",
            audit_chain_seq: 2,
            leaf_digest: "cc".repeat(32),
          },
        ],
        next_cursor: null,
      },
    };
  }
  if (
    path.startsWith("/v1/customer/audit/events/") &&
    path.endsWith("/proof") &&
    method === "GET"
  ) {
    const parts = path.split("/");
    const eventId = parts[parts.length - 2] ?? "cevt_001";
    // Trivial single-leaf proof: leaf_hash == expected_root, no siblings.
    // verify-proof.ts handles this without WASM (root == leaf for 1-leaf tree).
    const leaf =
      eventId === "cevt_001"
        ? "aa".repeat(32)
        : eventId === "cevt_002"
          ? "bb".repeat(32)
          : "cc".repeat(32);
    const seq =
      eventId === "cevt_001" ? 0 : eventId === "cevt_002" ? 1 : 2;
    return {
      status: 200,
      body: {
        event_id: eventId,
        leaf_hash: leaf,
        siblings: [],
        expected_root: leaf,
        algorithm: "blake3",
        chain_seq: seq,
      },
    };
  }
  if (path === "/v1/customer/audit/chain/history" && method === "GET") {
    // Strictly non-decreasing — no anomalies expected by default.
    return {
      status: 200,
      body: {
        snapshots: [
          { ts: "2026-05-13T00:00:00Z", head_digest: "11".repeat(32), total_events: 1 },
          { ts: "2026-05-13T12:00:00Z", head_digest: "22".repeat(32), total_events: 2 },
          { ts: "2026-05-14T12:00:00Z", head_digest: "ab".repeat(32), total_events: 3 },
        ],
      },
    };
  }

  // ----- DSR (admin-facing list / approve) -----
  if (path === "/v1/admin/dsr/requests" && method === "GET") {
    if (!identity || (identity.role !== "admin" && identity.role !== "approver")) {
      return rfc7807(403, "Forbidden", "operator role required");
    }
    return { status: 200, body: { items: state.dsrRequests } };
  }
  if (path.startsWith("/v1/admin/dsr/requests/") && path.endsWith("/approve") && method === "POST") {
    const id = path.split("/")[4];
    const r = state.dsrRequests.find((x) => x.request_id === id);
    if (!r) return rfc7807(404, "Not Found", "DSR not found");
    if (!identity || identity.role !== "admin" || !mfaIsFresh(identity, headers)) {
      return rfc7807(403, "Forbidden", "admin role and fresh MFA required");
    }
    r.status = "approved";
    state.auditEvents = [
      {
        event_id: `evt_dsr_${r.request_id}`,
        ts: new Date().toISOString(),
        tenant_id: identity.tenant_id,
        event_type: "dsr.request",
        severity: "info",
        actor: identity.sub,
        summary: `approved DSR ${r.request_id}`,
        correlation_id: `corr-${r.request_id}`,
      },
      ...state.auditEvents,
    ];
    return { status: 200, body: r };
  }
  if (path.startsWith("/v1/admin/dsr/requests/") && method === "GET") {
    if (!identity || (identity.role !== "admin" && identity.role !== "approver")) {
      return rfc7807(403, "Forbidden", "operator role required");
    }
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
      request_count: 133_214,
      quota_bytes: 10_737_418_240,
      hit_rate: 0.86,
      time_saved_seconds: 1_607_000,
      dollars_saved_cents: 66_900,
      daily: Array.from({ length: 7 }, (_, i) => ({
        day: `2026-05-${String(8 + i).padStart(2, "0")}`,
        reads: 14_000 + i * 1_200,
        writes: 800 + i * 90,
        cas_bytes: 0, // no per-day byte history (frozen contract)
      })),
    };
    return { status: 200, body: usage };
  }

  if (path === "/v1/customer/audit" && method === "GET") {
    // Canonical backend params (`AuditQuery` in routes/customer.rs): from / to /
    // kind. `from` is the ISO lower bound, `kind` is a comma-separated event-type
    // filter, and `to` is accepted-but-not-filtered server-side (parity with the
    // Rust handler — the client narrows the upper bound locally).
    let rows = state.customer.audit;
    const fromParam = query["from"];
    if (fromParam) {
      const from = Date.parse(fromParam);
      if (!Number.isNaN(from)) {
        rows = rows.filter((r) => Date.parse(r.ts) >= from);
      }
    }
    const kind = query["kind"];
    if (kind) {
      const types = new Set(kind.split(","));
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
      scopes: b.scopes ?? ["cache:read"],
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
    // WIRE-SHAPE: the container replies `{ "pat": { … }, "token": "…" }` — the row
    // is ENVELOPED and the shown-once secret rides alongside it
    // (routes/customer.rs:810-820). This fixture used to return the flat row +
    // token, a shape the server never sends, which left the E2E gate structurally
    // unable to catch the missing unwrap in `createPat`. Mirror the real wire.
    return {
      status: 201,
      body: { pat, token: `crl_pat_${pat.pat_id}_secret_shown_once` },
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
    // WIRE-SHAPE: the container replies `{ "pat": { … } }` (routes/customer.rs:860-870),
    // not a bare row. Mirror the real wire so the E2E gate can fail on a dropped unwrap.
    return { status: 200, body: { pat } };
  }

  if (path === "/v1/customer/team" && method === "GET") {
    return { status: 200, body: { members: state.customer.team } };
  }
  if (path === "/v1/customer/team/invite" && method === "POST") {
    const b = typeof body === "object" && body !== null ? (body as Record<string, unknown>) : {};
    if (!b.email) return rfc7807(400, "Bad Request", "email required");
    if (typeof b.role !== "string") return rfc7807(422, "Unprocessable Entity", "role is required");
    const role = b.role.trim().toLowerCase();
    if (role === "owner") return rfc7807(403, "Forbidden", "owner role is not grantable");
    if (role !== "admin" && role !== "member" && role !== "viewer") {
      return rfc7807(400, "Bad Request", "unsupported team invite role");
    }
    const m: CustomerTeamMember = {
      user_id: `user_invite_${state.customer.team.length + 1}`,
      email: String(b.email),
      role,
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
    // WIRE-SHAPE: the container replies `{ "member": { … } }`, not a bare member
    // (routes/customer.rs:961, asserted server-side at :2062). This fixture used
    // to return the flat row — a shape the server never sends — which left the
    // E2E gate structurally unable to catch the missing unwrap in `inviteTeam`.
    // Mirror the real wire here, always.
    return { status: 201, body: { member: m } };
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
    g.portalSessionSeq += 1;
    const id = `bps_e2e_${g.portalSessionSeq.toString(16).padStart(8, "0")}`;
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
