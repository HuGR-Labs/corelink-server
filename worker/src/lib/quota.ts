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
 *
 * # Failure-mode symmetry (F21 fix)
 *
 * getTierForTenant and checkStorageQuota had asymmetric D1 failure modes:
 * a partial outage where only the tier query fails would return 'free' (most
 * restrictive) while the storage query succeeded — causing false-positive 429s
 * for paid tenants with > 10 GiB stored.
 *
 * Fix: getTierForTenant now returns `{ tier, d1Error: boolean }`. When
 * d1Error=true, the caller (checkStorageQuota and any combined check) MUST
 * skip the storage check entirely and return ok:true (consistent fail-open),
 * matching the file's documented "fail-open on D1 errors" posture.
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
 * Result of a tier lookup, carrying the resolved tier and a flag indicating
 * whether the result was derived from a D1 error rather than a confirmed DB
 * read (F21 fix — symmetric failure-mode signalling).
 *
 * When `d1Error` is true, the caller MUST treat the tier as unconfirmed and
 * skip any downstream storage-quota check (fail-open the entire quota
 * decision), preventing the false-positive 429 that arises from combining an
 * error-derived 'free' tier with an actual high-storage-usage query.
 */
export interface TierResult {
  /** Resolved tier (may be the hard-coded 'free' fallback on D1 error). */
  readonly tier: Tier;
  /**
   * True when both tier queries failed with a D1 error and the returned tier
   * is the hard-coded 'free' fallback (NOT a confirmed DB value). Callers
   * must skip the storage-quota check to maintain fail-open symmetry (F21).
   */
  readonly d1Error: boolean;
}

/**
 * Resolve the effective tier for a tenant.
 *
 * Lookup order:
 *   1. tier_selections.tier  — authoritative subscription record (migration 0039).
 *   2. tenant.tier           — default column added by migration 0057 (DEFAULT 'free').
 *   3. Hard-coded 'free'     — fallback when both D1 queries fail or return null.
 *
 * Returns a {@link TierResult} with `d1Error=true` when BOTH D1 queries
 * failed (i.e. the 'free' tier is a fallback, not a confirmed value). Callers
 * MUST skip the storage-quota check when `d1Error=true` to preserve
 * fail-open symmetry and avoid false-positive 429s for paid tenants (F21).
 *
 * The DO's deeper quota FSM provides the safety net on the allow path.
 */
export async function getTierForTenant(
  db: D1Database,
  tenantId: string,
): Promise<TierResult> {
  // ── 1. tier_selections (canonical, written by S-19 subscription flow) ──────
  // SECURITY: only an ACTIVE subscription grants its paid tier. The
  // checkout backend writes the requested paid tier at click time with
  // `subscription_state='pending_checkout'` (before any payment); without
  // the `= 'active'` filter, a user could select a paid tier, abandon
  // Stripe, and still be served full paid quota for free. `pending_checkout`
  // / `inactive` rows fall through to tenant.tier → 'free'.
  interface TierSelRow { tier: string }
  let tierSel: TierSelRow | null = null;
  let tierSelError = false;
  try {
    tierSel = await db
      .prepare(
        "SELECT tier FROM tier_selections WHERE tenant_id = ?1 AND subscription_state = 'active' LIMIT 1",
      )
      .bind(tenantId)
      .first<TierSelRow>();
  } catch {
    // D1 error — fall through to tenant.tier lookup; track the error.
    tierSelError = true;
  }

  if (tierSel !== null && isValidTier(tierSel.tier)) {
    return { tier: tierSel.tier as Tier, d1Error: false };
  }

  // ── 2. tenant.tier (migration 0057, DEFAULT 'free') ─────────────────────
  interface TenantTierRow { tier: string }
  let tenantTier: TenantTierRow | null = null;
  let tenantTierError = false;
  try {
    tenantTier = await db
      .prepare("SELECT tier FROM tenant WHERE tenant_id = ?1 LIMIT 1")
      .bind(tenantId)
      .first<TenantTierRow>();
  } catch {
    // D1 error — fall through to hard default; track the error.
    tenantTierError = true;
  }

  if (tenantTier !== null && isValidTier(tenantTier.tier)) {
    return { tier: tenantTier.tier as Tier, d1Error: false };
  }

  // ── 3. Hard default ───────────────────────────────────────────────────────
  // d1Error=true when both queries failed, signalling the 'free' tier is
  // unconfirmed (F21: callers must skip the storage check to avoid false
  // positive 429s for paid tenants during partial D1 outages).
  const d1Error = tierSelError && tenantTierError;
  return { tier: "free", d1Error };
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
 * F21 fix — symmetric failure modes: if the tier was derived from a D1
 * error (`tierResult.d1Error === true`), this function SKIPS the storage
 * query entirely and returns ok:true. Combining an error-derived 'free'
 * tier with a successful storage query would produce false-positive 429s
 * for paid tenants (e.g. solo with 40 GiB stored → free cap 10 GiB → 429).
 * Skipping the check when the tier is unconfirmed keeps the overall
 * posture consistently fail-open during partial D1 outages, matching the
 * file's documented "fail-open on D1 errors" intent.
 *
 * retryAfterSec is set to seconds-until-next-UTC-month-start (capped at
 * 31 days = 2678400 s), consistent with the ADR-0020 Retry-After semantic.
 *
 * @param tierResult  Result from {@link getTierForTenant} carrying the
 *                    resolved tier and the d1Error flag (F21).
 */
export async function checkStorageQuota(
  db: D1Database,
  tenantId: string,
  tierResult: TierResult,
): Promise<QuotaCheckResult> {
  // F21 fix: if the tier lookup itself errored, skip the storage check to
  // avoid a false-positive 429 (an unconfirmed 'free' tier + actual high
  // usage = incorrect denial for a paid tenant).
  if (tierResult.d1Error) {
    return { ok: true };
  }

  const tier = tierResult.tier;
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
 * Tracks whether the "request-quota flag is ON but no counter backend exists"
 * warning has already been emitted, so we log it ONCE per Worker isolate
 * rather than on every request (a per-request log would flood the logs and is
 * itself a cost/availability hazard). Module-scoped: reset on each cold start,
 * which is the desired cadence for a misconfiguration signal.
 */
let requestQuotaMisconfigWarned = false;

/**
 * Check whether the tenant is within their monthly request quota.
 *
 * ## Enforcement status (HONEST): NOT YET ENFORCED — no counter backend
 *
 * Request-rate limiting at the *data plane* is enforced in-app by the
 * container's `corelink-ratelimit` token-bucket middleware (per-tenant
 * req/s + burst; see `crates/corelink-container/src/routes/ratelimit_layer.rs`).
 * This function is the SEPARATE *monthly aggregate* request-count cap from the
 * signed rate card (`requestsPerMonthMax`), which requires a durable
 * per-tenant monthly counter that does NOT yet exist.
 *
 * TODO(request-counter): wire a `monthly_request_counts(tenant_id, year_month,
 * request_count)` table + an atomic increment on the allow path, then change
 * the body to:
 *   SELECT request_count FROM monthly_request_counts
 *   WHERE tenant_id = ?1 AND year_month = ?2
 * returning ok:false + 429 (Retry-After = secondsUntilNextMonthStart) when
 * request_count >= quota.requestsPerMonthMax. Until then this function CANNOT
 * enforce the cap — it has no DB handle and no counter to read.
 *
 * Because we must NOT silently ship a no-op that *claims* enforcement, the
 * `REQUEST_QUOTA_ENABLED` flag is honoured explicitly:
 *   - flag OFF (default) → ok:true, no log (enforcement is openly deferred).
 *   - flag ON            → ok:true (fail-OPEN for availability), but emit a
 *     LOUD once-per-isolate warning that the operator asked for enforcement
 *     the backend cannot yet deliver. Flipping the flag must never give a
 *     false sense of a cap being applied.
 *
 * F21: accepts TierResult for consistency with checkStorageQuota; when
 * d1Error=true the check is a no-op (returns ok:true) — the same fail-open
 * symmetry applies.
 */
export function checkRequestQuota(
  _tierResult: TierResult,
  requestQuotaEnabled: boolean,
): QuotaCheckResult {
  if (requestQuotaEnabled && !requestQuotaMisconfigWarned) {
    requestQuotaMisconfigWarned = true;
    // INV-NO-PII-IN-LOGS: no tenant id logged.
    console.warn(
      "[quota] REQUEST_QUOTA_ENABLED=true but the monthly_request_counts " +
        "backend is NOT wired — monthly request-cap enforcement is a NO-OP " +
        "(failing OPEN). See checkRequestQuota TODO(request-counter). " +
        "Per-second rate limiting IS enforced by the container token-bucket.",
    );
  }
  // Deferred — see function docstring. No counter backend ⇒ cannot enforce;
  // fail-OPEN to preserve availability (consistent with the file's posture).
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
