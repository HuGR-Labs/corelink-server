/**
 * Per-tier quota definitions and enforcement helpers for the CoreLink Worker.
 *
 * Quota checks run AFTER PAT auth succeeds and BEFORE forwarding to the
 * Durable Object. Enforcement is fail-open on D1 errors (quota status
 * unknown → allow through) to preserve availability; DO performs its own
 * deeper quota enforcement (CAS / quota_fsm_state) on every mutation.
 *
 * Tier taxonomy (canonical 6-tier launch ladder + legacy classes). Numbers
 * come from the signed launch rate card (apps/docs/src/lib/pricing.ts
 * TIER_RATE_CARD / specs/_audits/2026-05-27-pricing-benchmarks.md §5):
 *   free       10 GB storage / 500 K requests per month
 *   solo       50 GB / 2 M requests per month
 *   starter    150 GB / 6 M requests per month
 *   team       1 TB / no request cap        (legacy class, retained)
 *   pro        500 GB / 20 M requests per month
 *   org        legacy pre-S-19 name of 'pro' — same quota class
 *   max        2 TB / 80 M requests per month
 *   enterprise no caps
 *
 * Storage quota is enforced via SUM(bytes_used) from tenant_storage_state.
 * Request quota is deferred behind a feature flag until a monthly request
 * counter table is wired (TODO below). Over quota → HTTP 429 + Retry-After.
 *
 * INV-NO-PII-IN-LOGS: tenant_id is not logged; only included in structured
 * responses forwarded to the caller.
 */

import type { D1Database } from "@cloudflare/workers-types";

// ─────────────────────────────────────────────────────────────────────────────
// Tier taxonomy
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Canonical tier names used internally — the 6-tier launch ladder
 * (free/solo/starter/pro/max/enterprise) plus two legacy classes kept for
 * back-compat with existing rows: 'team' (pre-6-tier SKU, retained by the
 * 0062 CHECK) and 'org' (pre-S-19 name of 'pro', same quota class).
 */
export type Tier =
  | "free"
  | "solo"
  | "starter"
  | "team"
  | "pro"
  | "org"
  | "max"
  | "enterprise";

/** Quota ceilings for a tier. MAX_SAFE_INTEGER means "no cap". */
export interface Quota {
  /** Maximum aggregate storage bytes across all regions. */
  readonly storageBytesMax: number;
  /**
   * Maximum HTTP requests per calendar month.
   * TODO(request-counter): deferred until a monthly counter table exists.
   * Caps follow the signed rate card but are NOT yet enforced (storage is).
   */
  readonly requestsPerMonthMax: number;
}

/** Per-tier quota table. */
export const QUOTAS: Record<Tier, Quota> = {
  free:       { storageBytesMax: 10 * 1_073_741_824,         requestsPerMonthMax: 500_000 },
  solo:       { storageBytesMax: 50 * 1_073_741_824,         requestsPerMonthMax: 2_000_000 },
  starter:    { storageBytesMax: 150 * 1_073_741_824,        requestsPerMonthMax: 6_000_000 },
  team:       { storageBytesMax: 1_099_511_627_776,          requestsPerMonthMax: Number.MAX_SAFE_INTEGER },
  pro:        { storageBytesMax: 500 * 1_073_741_824,        requestsPerMonthMax: 20_000_000 },
  org:        { storageBytesMax: 500 * 1_073_741_824,        requestsPerMonthMax: 20_000_000 },
  max:        { storageBytesMax: 2_000 * 1_073_741_824,      requestsPerMonthMax: 80_000_000 },
  enterprise: { storageBytesMax: Number.MAX_SAFE_INTEGER,    requestsPerMonthMax: Number.MAX_SAFE_INTEGER },
};

// ─────────────────────────────────────────────────────────────────────────────
// Tier lookup
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Resolve the effective tier for a tenant.
 *
 * Lookup order:
 *   1. tier_selections.tier  — authoritative subscription record (migration 0039).
 *   2. tenant.tier           — default column added by migration 0057 (DEFAULT 'free').
 *   3. Hard-coded 'free'     — fallback when both D1 queries fail or return null.
 *
 * Fail-open: D1 errors return 'free' (the most restrictive tier) to avoid
 * blocking traffic on a transient control-plane outage. The DO's deeper
 * quota FSM provides the safety net.
 */
export async function getTierForTenant(
  db: D1Database,
  tenantId: string,
): Promise<Tier> {
  // ── 1. tier_selections (canonical, written by S-19 subscription flow) ──────
  interface TierSelRow { tier: string }
  let tierSel: TierSelRow | null = null;
  try {
    tierSel = await db
      .prepare("SELECT tier FROM tier_selections WHERE tenant_id = ?1 LIMIT 1")
      .bind(tenantId)
      .first<TierSelRow>();
  } catch {
    // D1 error — fall through to tenant.tier lookup.
  }

  if (tierSel !== null && isValidTier(tierSel.tier)) {
    return tierSel.tier as Tier;
  }

  // ── 2. tenant.tier (migration 0057, DEFAULT 'free') ─────────────────────
  interface TenantTierRow { tier: string }
  let tenantTier: TenantTierRow | null = null;
  try {
    tenantTier = await db
      .prepare("SELECT tier FROM tenant WHERE tenant_id = ?1 LIMIT 1")
      .bind(tenantId)
      .first<TenantTierRow>();
  } catch {
    // D1 error — fall through to hard default.
  }

  if (tenantTier !== null && isValidTier(tenantTier.tier)) {
    return tenantTier.tier as Tier;
  }

  // ── 3. Hard default ───────────────────────────────────────────────────────
  return "free";
}

function isValidTier(value: string): value is Tier {
  return (
    value === "free" ||
    value === "solo" ||
    value === "starter" ||
    value === "team" ||
    value === "pro" ||
    value === "org" ||
    value === "max" ||
    value === "enterprise"
  );
}

// ─────────────────────────────────────────────────────────────────────────────
// Storage quota check
// ─────────────────────────────────────────────────────────────────────────────

/** Result of a quota check. */
export type QuotaCheckResult =
  | { readonly ok: true }
  | { readonly ok: false; readonly retryAfterSec: number; readonly reason: string };

/**
 * Check whether the tenant is within their storage quota.
 *
 * Reads SUM(bytes_used) across all regions for the tenant from
 * tenant_storage_state (the DO's 5-min-synced durable mirror).
 *
 * Returns ok:false only when:
 *   - The tier has a finite storageBytesMax, AND
 *   - The tenant's current bytes_used >= storageBytesMax.
 *
 * Fails OPEN on D1 errors (returns ok:true) to preserve availability;
 * the DO enforces the hard CAS boundary on every mutation.
 *
 * retryAfterSec is set to seconds-until-next-UTC-month-start (capped at
 * 31 days = 2678400 s), consistent with the ADR-0020 Retry-After semantic.
 */
export async function checkStorageQuota(
  db: D1Database,
  tenantId: string,
  tier: Tier,
): Promise<QuotaCheckResult> {
  const quota = QUOTAS[tier];

  // Enterprise (and unlimited tiers) — no cap to check.
  if (quota.storageBytesMax === Number.MAX_SAFE_INTEGER) {
    return { ok: true };
  }

  interface SumRow { total_bytes: number | null }
  let row: SumRow | null = null;
  try {
    row = await db
      .prepare(
        "SELECT SUM(bytes_used) AS total_bytes FROM tenant_storage_state WHERE tenant_id = ?1",
      )
      .bind(tenantId)
      .first<SumRow>();
  } catch {
    // D1 error → fail open.
    return { ok: true };
  }

  const totalBytes = row?.total_bytes ?? 0;

  if (totalBytes < quota.storageBytesMax) {
    return { ok: true };
  }

  return {
    ok: false,
    retryAfterSec: secondsUntilNextMonthStart(),
    reason: `Storage quota exceeded: ${totalBytes} bytes used, limit is ${quota.storageBytesMax} bytes (tier: ${tier})`,
  };
}

/**
 * Check whether the tenant is within their monthly request quota.
 *
 * TODO(request-counter): request quota enforcement is deferred until a
 * monthly_request_counts table is wired. This function currently always
 * returns ok:true for all tiers, including 'free'. Controlled by the
 * REQUEST_QUOTA_ENABLED feature flag (Env.REQUEST_QUOTA_ENABLED).
 *
 * When the counter table exists, the check will be:
 *   SELECT request_count FROM monthly_request_counts
 *   WHERE tenant_id = ?1 AND year_month = ?2
 * and return ok:false + 429 if request_count >= quota.requestsPerMonthMax.
 */
export function checkRequestQuota(
  _tier: Tier,
  _requestQuotaEnabled: boolean,
): QuotaCheckResult {
  // Deferred — see function docstring.
  return { ok: true };
}

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

/**
 * Compute seconds from now until the first instant of the next UTC calendar
 * month, capped at 31 days (2 678 400 s) per ADR-0020 Retry-After contract.
 */
export function secondsUntilNextMonthStart(): number {
  const now = new Date();
  const nextMonth = new Date(Date.UTC(now.getUTCFullYear(), now.getUTCMonth() + 1, 1, 0, 0, 0, 0));
  const diffMs = nextMonth.getTime() - now.getTime();
  const diffSec = Math.ceil(diffMs / 1000);
  return Math.min(diffSec, 2_678_400);
}
