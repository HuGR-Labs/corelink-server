import type { AdminOp, AuditEventDetail, AuditPage, Tenant } from "./types";
import type {
  CustomerAuditEvent,
  CustomerBilling,
  CustomerOverview,
  CustomerPat,
  CustomerTeamMember,
} from "./customer-types";
export interface MockState {
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
function makeTenants(): Tenant[] {
  const regions: Array<Tenant["region"]> = ["us-east", "us-west", "eu-west", "ap-south"];
  const plans: Array<Tenant["plan"]> = [
    "free",
    "solo",
    "starter",
    "team",
    "pro",
    "max",
    "enterprise",
  ];
  return [{
    tenant_id: "tenant_acme",
    name: "Acme Inc.",
    plan: "team",
    region: "us-east",
    byok_status: "active",
    created_at: "2026-01-01T00:00:00Z",
  }, ...Array.from({ length: 25 }, (_, i) => ({
    tenant_id: `tenant_${String(i + 1).padStart(3, "0")}`,
    name: `Tenant ${i + 1}`,
    plan: plans[i % plans.length]!,
    region: regions[i % regions.length]!,
    byok_status: (i % 5 === 0 ? "active" : "none") as Tenant["byok_status"],
    created_at: `2026-0${(i % 5) + 1}-01T00:00:00Z`,
  }))];
}

function makeAuditEvents(): MockState["auditEvents"] {
  return [
    {
      event_id: "evt_001",
      ts: "2026-05-10T10:00:00Z",
      tenant_id: "tenant_acme",
      event_type: "auth.login",
      severity: "info",
      actor: "user_e2e_admin",
      summary: "operator login from 10.0.0.1",
      correlation_id: "corr-001",
    },
    {
      event_id: "evt_002",
      ts: "2026-05-12T11:30:00Z",
      tenant_id: "tenant_acme",
      event_type: "byok.cmk_rotated",
      severity: "warn",
      actor: "user_e2e_approver",
      summary: "CMK rotated for tenant_acme",
      correlation_id: "corr-002",
    },
    {
      event_id: "evt_003",
      ts: "2026-05-13T09:00:00Z",
      tenant_id: "tenant_acme",
      event_type: "dsr.request",
      severity: "info",
      actor: "user_e2e_admin",
      summary: "DSR access submitted",
      correlation_id: "corr-003",
    },
  ];
}

export function makeAuditDetail(id: string, state: MockState): AuditEventDetail {
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
  return [{
    op_id: "op_byok_001",
    op_type: "byok_cmk_rotation",
    requestor: "user_e2e_admin",
    requested_at: "2026-05-13T08:00:00Z",
    status: "awaiting_approval",
    payload: { tenant_id: "tenant_acme", new_cmk_id: "cmk_v2" },
    impact_summary: "Rotate CMK for tenant_acme (BYOK).",
    tenant_scope: ["tenant_acme"],
    approvals: [],
  }];
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

// The `scopes` values below MUST come from the real codomain of
// `customer_d1.rs::scope_to_list`, which is the only producer the dashboard
// ever sees: ["cache:read"], ["cache:read","cache:write"],
// ["cache:find-missing"] (find-only is EXCLUSIVE — never combined with
// read/write), [] , or an unrecognized legacy value surfaced verbatim.
// `cache:r` / `cache:w` are REQUEST-side spellings and are never returned.
function makeCustomerPats(): CustomerPat[] {
  return [
    {
      pat_id: "pat_001",
      name: "ci-runner",
      scopes: ["cache:read", "cache:write"],
      created_at: "2026-04-15T09:00:00Z",
      last_used_at: "2026-05-14T22:01:00Z",
    },
    {
      pat_id: "pat_002",
      name: "dashboard-readonly",
      scopes: ["cache:read"],
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
      role: "owner",
      joined_at: "2026-01-15T09:00:00Z",
      status: "active",
    },
    {
      user_id: "user_e2e_approver",
      email: "approver@acme.example",
      role: "admin",
      joined_at: "2026-02-01T09:00:00Z",
      status: "active",
    },
    {
      user_id: "user_e2e_member",
      email: "member@acme.example",
      role: "member",
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

export function freshState(): MockState {
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
