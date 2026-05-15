// Customer self-serve dashboard types.
//
// All shapes are tenant-scoped (the caller's own tenant). The backend resolves
// tenant from the session — there is no tenant_id on the wire.

export interface CustomerOverview {
  tenant_id: string;
  tenant_name: string;
  plan: "free" | "starter" | "team" | "enterprise";
  usage: {
    period: string; // e.g. "2026-05"
    cas_bytes: number;
    reads: number;
    writes: number;
    quota_bytes: number;
  };
  billing: {
    status: "trialing" | "active" | "past_due" | "canceled";
    next_invoice_at: string; // ISO
    amount_due_cents: number;
    currency: "usd" | "eur" | "brl";
  };
  byok: {
    status: "none" | "active" | "rotation_pending";
    cmk_id?: string;
    last_rotated_at?: string;
  };
  recent_activity: CustomerAuditEvent[];
}

export interface CustomerUsage {
  period: string;
  cas_bytes: number;
  reads: number;
  writes: number;
  quota_bytes: number;
  daily: Array<{ day: string; reads: number; writes: number; cas_bytes: number }>;
}

export interface CustomerAuditEvent {
  event_id: string;
  ts: string;
  event_type: string;
  severity: "info" | "warn" | "critical";
  actor: string;
  summary: string;
}

export interface CustomerAuditFilter {
  since?: string;
  event_types?: string[];
}

export interface CustomerBilling {
  status: "trialing" | "active" | "past_due" | "canceled";
  plan: "free" | "starter" | "team" | "enterprise";
  current_period_start: string;
  current_period_end: string;
  amount_due_cents: number;
  currency: "usd" | "eur" | "brl";
  payment_method?: { brand: string; last4: string; exp_month: number; exp_year: number };
  invoices: Array<{
    invoice_id: string;
    issued_at: string;
    amount_cents: number;
    status: "paid" | "open" | "void";
    hosted_url: string;
  }>;
}

export interface CustomerPat {
  pat_id: string;
  name: string;
  scopes: string[];
  created_at: string;
  last_used_at?: string;
  revoked_at?: string;
}

export interface CustomerTeamMember {
  user_id: string;
  email: string;
  role: "Owner" | "Admin" | "Developer" | "Viewer";
  joined_at: string;
  status: "active" | "invited" | "suspended";
}
