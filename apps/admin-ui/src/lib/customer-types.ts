// Customer self-serve dashboard types.
//
// All shapes are tenant-scoped (the caller's own tenant). The backend resolves
// tenant from the session — there is no tenant_id on the wire.
//
// DATA-TRUTH legend (mirrors crates/corelink-container/src/customer_d1.rs, the
// prod "HONEST v1 contract"). Screens MUST honor these — a [stub]/[not-wired]
// field renders a teaching EmptyState, NEVER a fabricated number:
//   [live]      — backed by a deployed D1 table / Stripe, real value in prod.
//   [stub]      — endpoint exists but prod returns 0/[]/absent (no table yet).
//   [not-wired] — no endpoint at all yet; client method throws NotWiredError.
// Backend backlog to close the stubs: docs/design/2026-07-06-customer-dashboard-BUILD-WAVE.md (BE-1..11).

export interface CustomerOverview {
  tenant_id: string;
  tenant_name: string;
  plan: "free" | "solo" | "starter" | "team" | "pro" | "max" | "enterprise";
  usage: {
    period: string; // e.g. "2026-05"
    cas_bytes: number;
    reads: number;
    writes: number;
    quota_bytes: number;
  };
  billing: {
    // Keep in sync with CustomerBilling["status"] — the overview snapshot is
    // derived from the same backend billing record.
    status: "trialing" | "active" | "past_due" | "canceled" | "inactive";
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
  cas_bytes: number; // [live] tenant_storage_state
  reads: number; // [live] period read total (BE-1)
  writes: number; // [live] period write total (BE-1)
  request_count: number; // [live] monthly_request_counts (0071) via BE-1a — billable requests this period
  quota_bytes: number; // [live]
  hit_rate: number | null; // [live] BE-2 — fraction 0..1; null = no reads yet (cold start → teaching state, NEVER a fake %)
  time_saved_seconds: number; // [live] BE-2 — build time saved (render humanized)
  dollars_saved_cents: number; // [live] BE-2 — MODELED estimate (~15s compute saved per hit); render "$X" + label "estimated"
  daily: Array<{ day: string; reads: number; writes: number; cas_bytes: number }>; // [live] BE-1 — per-day reads/writes; cas_bytes always 0 (no per-day byte history)
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
  // "inactive" = no Stripe subscription on file (the worker/container emits it
  // for tenants that never checked out). Additive widening — keep in sync with
  // the backend's billing status enum.
  status: "trialing" | "active" | "past_due" | "canceled" | "inactive";
  plan: "free" | "solo" | "starter" | "team" | "pro" | "max" | "enterprise";
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

// ── New-surface types (W0.c). Tags per DATA-TRUTH legend above. ──────────────

/** The cache tier ladder (frozen: Free/Solo/Starter/Pro/Max/Enterprise). "team"
 *  retained only for back-compat with older rows. */
export type CachePlan = CustomerOverview["plan"];

/** Per-surface connect config snippet (Connect-a-tool screen, W2). Endpoints are
 *  [live]; the token is referenced via env var, NEVER inlined (CTRL-CRED-001). */
export interface ConnectSurface {
  id: "bazel" | "turbo" | "sccache" | "npm" | "pip" | "cas";
  label: string;
  code: string; // pre-filled with the tenant endpoint + an env-var token ref
  lang?: string;
  docsHref?: string;
}

/** Spend controls (Settings, W8). [not-wired] until BE-7 (read/update endpoint). */
export interface CustomerDollarCeiling {
  ceiling_cents: number | null; // null = no cap set
  spent_cents: number; // [stub] until BE-7
  currency: "usd" | "eur" | "brl";
}

/** BYOK config (Trust, W8). Status is [live]; the KMS config is [not-wired] (BE-8). */
export interface CustomerByokConfig {
  status: "none" | "active" | "rotation_pending";
  provider?: "aws_kms" | "gcp_kms" | "azure_kv" | "vault";
  cmk_id?: string;
  last_rotated_at?: string;
}

/** Runner entitlement (Runners, W9). [not-wired] until BE-10. */
export interface CustomerRunnerEntitlement {
  sku: string | null; // e.g. "runner_pro"; null = none
  max_concurrency: number;
  max_vcpu_h: number;
  consumed_vcpu_h: number; // [stub] until BE-10
  install_status: "installed" | "not_installed";
  repo_allowlist: string[]; // [not-wired] until BE-10
}

/** Workspace snapshot (Workspaces, W10). [not-wired] until BE-11. */
export interface CustomerWorkspace {
  workspace_id: string;
  name: string;
  size_bytes: number;
  created_at: string;
  pinned: boolean;
}

/**
 * A recent runner run. The server surface exists (BE-10) but returns an empty
 * list until a runner-runs history table lands, so this list is honestly empty
 * today (never fabricated).
 */
export interface CustomerRunnerRun {
  run_id: string;
  repo_full_name: string;
  status: "queued" | "running" | "success" | "failed" | "cancelled";
  started_at: string;
  duration_s: number | null;
}
