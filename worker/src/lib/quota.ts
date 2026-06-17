/**
 * Per-tier quota definitions and enforcement helpers for the CoreLink Worker.
 *
 * Quota checks run AFTER PAT auth succeeds and BEFORE forwarding to the
 * Durable Object. On a D1 error the posture is verb-aware (CAA-360 #25):
 * READ-style requests fail OPEN (quota status unknown → allow through) to
 * preserve availability, while MUTATING (PUT/POST) requests fail CLOSED with a
 * short Retry-After so an outage cannot be used to write past the cap. The DO
 * performs its own deeper quota enforcement (CAS / quota_fsm_state) on every
 * mutation.
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
 * Request quota is enforced via an atomic monthly counter UPSERT against
 * monthly_request_counts (migration 0071). Enforcement is ON BY DEFAULT
 * (fail-CLOSED): the Worker derives the boolean from an explicit opt-OUT
 * kill-switch (REQUEST_QUOTA_DISABLED="true" → off; unset → ENFORCED), so a
 * missing prod env var keeps the contracted cap live (Cluster D).
 * Over quota → HTTP 429 + Retry-After.
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
   * Maximum HTTP requests per calendar month. Enforced by
   * {@link checkRequestQuota} via the monthly_request_counts counter
   * (migration 0071), enforced by default (Cluster D). MAX_SAFE_INTEGER
   * means "no cap" (team / enterprise). Caps follow the signed rate card.
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

/**
 * Name of the server-trusted header carrying the tenant's resolved per-tier
 * storage cap (bytes) to the container, where the byte-accounting reservation
 * uses it to seed a FRESH `tenant_storage_state` row with the REAL cap instead
 * of the legacy hard-coded `0` (which the container conflated with "unlimited",
 * leaving a fresh/unsynced tenant uncapped). The Worker is the SOLE setter (it
 * is the quota-resolution authority); it MUST be in {@link stripClientTrustHeaders}
 * so a client can never forge it — exactly like `x-corelink-tenant-id`.
 */
export const STORAGE_QUOTA_HEADER = "x-corelink-storage-quota-bytes";

/**
 * Compute the value of {@link STORAGE_QUOTA_HEADER} for a resolved tier, or
 * `null` when the header MUST NOT be injected (so the container's fail-closed
 * `None` default applies).
 *
 * Semantics, kept symmetric with the container's `storage_quota_from_headers` +
 * fresh-row seed:
 *   - tier cap is a finite byte count → the decimal string of that count
 *     (the container seeds `bytes_quota = n`);
 *   - tier is genuinely unlimited (`storageBytesMax === MAX_SAFE_INTEGER`,
 *     i.e. enterprise/team) → `"0"` (the container's deliberate
 *     unlimited sentinel — distinct from absence);
 *   - the tier was derived from a D1 error (`d1Error === true`) → `null`
 *     (cap unconfirmed; do not inject — let the container fail closed on a
 *     fresh row rather than seed a possibly-wrong cap). The verb-aware
 *     storage-quota gate already fails writes closed on a D1 error, so a fresh
 *     tenant cannot slip through here either.
 */
export function storageQuotaHeaderValue(tierResult: TierResult): string | null {
  if (tierResult.d1Error) {
    return null;
  }
  const max = QUOTAS[tierResult.tier].storageBytesMax;
  if (max === Number.MAX_SAFE_INTEGER) {
    return "0"; // genuine unlimited tier
  }
  return String(max);
}

// ─────────────────────────────────────────────────────────────────────────────
// Storage quota check
// ─────────────────────────────────────────────────────────────────────────────

/** Result of a quota check. */
export type QuotaCheckResult =
  | { readonly ok: true }
  | { readonly ok: false; readonly retryAfterSec: number; readonly reason: string };

/**
 * Retry-After (seconds) advertised when a storage-quota check on a MUTATING
 * request cannot be resolved due to a transient D1 error (CAA-360 #25).
 * Deliberately short — the condition is a store blip, not a real monthly-cap
 * breach, so the client should retry promptly rather than wait out the cycle.
 */
const STORAGE_QUOTA_D1_ERROR_RETRY_SEC = 2;

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
 * 31 days = 2678400 s) for a real over-cap breach, consistent with the
 * ADR-0020 Retry-After semantic.
 *
 * # D1-error posture (CAA-360 #25 fix)
 *
 * When the quota cannot be confirmed because of a transient D1 error (either
 * the tier lookup errored — `tierResult.d1Error` — or the storage SUM query
 * threw), the failure mode now depends on the request verb:
 *
 *   - **READ-style requests** (`isMutating === false`) → fail OPEN (ok:true),
 *     preserving availability — a read cannot grow storage past the cap.
 *   - **MUTATING requests** (`isMutating === true`, i.e. PUT/POST writes that
 *     ADD bytes) → fail CLOSED with a SHORT Retry-After, so a D1 outage cannot
 *     be used to write past the storage cap unbounded. This mirrors the
 *     residency gate's fail-closed posture on writes. The DO's CAS quota FSM
 *     is the deeper net; this closes the edge hole.
 *
 * @param tierResult  Result from {@link getTierForTenant} carrying the
 *                    resolved tier and the d1Error flag (F21).
 * @param isMutating  Whether the request adds storage (PUT/POST write verbs).
 *                    Controls the D1-error failure mode (fail-closed on writes).
 */
export async function checkStorageQuota(
  db: D1Database,
  tenantId: string,
  tierResult: TierResult,
  isMutating: boolean,
): Promise<QuotaCheckResult> {
  // On a D1 error we cannot confirm storage headroom (CAA-360 #25). Reads fail
  // OPEN (availability); writes fail CLOSED with a short Retry-After so an
  // outage cannot be used to write past the cap unbounded.
  const onD1Error = (): QuotaCheckResult =>
    isMutating
      ? {
          ok: false,
          retryAfterSec: STORAGE_QUOTA_D1_ERROR_RETRY_SEC,
          reason: "storage quota temporarily unverifiable (store error); retry",
        }
      : { ok: true };

  // F21 + #25: if the tier lookup itself errored, the tier is unconfirmed.
  // Reads pass through (avoid a false-positive 429 for a paid tenant); writes
  // fail closed (cannot confirm headroom on a byte-adding op).
  if (tierResult.d1Error) {
    return onD1Error();
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
    // D1 error → fail open on reads, fail closed on writes (CAA-360 #25).
    return onD1Error();
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
 * Current UTC calendar month as a fixed `YYYY-MM` bucket key (e.g. `2026-06`).
 *
 * The bucket boundary matches {@link secondsUntilNextMonthStart} (both are
 * UTC-calendar-month aligned), so a tenant's counter implicitly resets when a
 * new month's first request INSERTs a fresh `(tenant_id, year_month)` row and
 * the advertised Retry-After lands exactly on that reset.
 */
function currentYearMonthUtc(): string {
  // `toISOString()` is always UTC `YYYY-MM-DDТHH:mm:ss.sssZ`; slice → `YYYY-MM`.
  return new Date().toISOString().slice(0, 7);
}

/**
 * Check whether the tenant is within their monthly request quota AND count
 * this request.
 *
 * ## Enforcement (red-team #5 fix)
 *
 * This is the *monthly aggregate* request-count cap from the signed rate card
 * (`requestsPerMonthMax`). It is DISTINCT from the *data-plane* per-second rate
 * limit enforced in-app by the container's `corelink-ratelimit` token-bucket
 * middleware (per-tenant req/s + burst; see
 * `crates/corelink-container/src/routes/ratelimit_layer.rs`). A slow-but-steady
 * tenant can stay under the per-second limit yet exceed the contracted monthly
 * allowance — this closes that gap at the Worker edge.
 *
 * Backing: the `monthly_request_counts(tenant_id, year_month, request_count)`
 * table (migration 0071). Enforcement is a single ATOMIC increment-and-check
 * UPSERT, so concurrent isolates cannot race a read-modify-write:
 *
 *   INSERT INTO monthly_request_counts (tenant_id, year_month, request_count, updated_at_ms)
 *   VALUES (?1, ?2, 1, ?3)
 *   ON CONFLICT(tenant_id, year_month)
 *   DO UPDATE SET request_count = request_count + 1, updated_at_ms = ?3
 *   RETURNING request_count;
 *
 * The post-increment `request_count` is compared to
 * `QUOTAS[tier].requestsPerMonthMax`. When it EXCEEDS the cap (i.e. this is the
 * request that crossed the line, or any beyond) the function returns
 * ok:false + Retry-After = {@link secondsUntilNextMonthStart}; the caller maps
 * that to HTTP 429.
 *
 * ## Flag semantics (`requestQuotaEnabled`)
 *
 * The caller derives this boolean fail-CLOSED from the opt-OUT kill-switch
 * `REQUEST_QUOTA_DISABLED` (Cluster D): unset/anything-but-"true" ⇒ ENABLED.
 * This function itself only sees the resolved boolean:
 *
 *   - `false` (dev/test, kill-switch on) → ok:true WITHOUT touching the counter.
 *     Enforcement is openly off; we do not even pay the write. (Storage quota is
 *     unaffected.)
 *   - `true`  (DEFAULT, incl. unset prod env) → atomic increment + cap check.
 *
 * ## D1-error / unconfirmed-tier posture
 *
 * Consistent with {@link checkStorageQuota} and the file's documented posture,
 * this READ-style check fails OPEN:
 *   - `tierResult.d1Error === true` (tier unconfirmed) → ok:true, no count
 *     (we cannot know the cap; avoid a false-positive 429 for a paid tenant).
 *   - the UPSERT throws (transient store error) → ok:true (availability).
 * Uncapped tiers (team / enterprise, MAX_SAFE_INTEGER) skip the counter write
 * entirely — there is nothing to enforce and no reason to pay a D1 write.
 *
 * @param db                  D1 handle (CONFIG_DB) carrying monthly_request_counts.
 * @param tenantId            Resolved tenant id (the counter key; never logged).
 * @param tierResult          Result from {@link getTierForTenant} (tier + d1Error).
 * @param requestQuotaEnabled Resolved enforcement boolean (fail-CLOSED default;
 *                            derived from the REQUEST_QUOTA_DISABLED opt-out
 *                            kill-switch). false → no-op, no write.
 */
export async function checkRequestQuota(
  db: D1Database,
  tenantId: string,
  tierResult: TierResult,
  requestQuotaEnabled: boolean,
): Promise<QuotaCheckResult> {
  // Flag off → enforcement is openly disabled; do not touch the counter.
  if (!requestQuotaEnabled) {
    return { ok: true };
  }

  // Unconfirmed tier (tier lookup hit a D1 error) → fail OPEN, no count. We
  // cannot know which cap applies, so counting against a fallback 'free' cap
  // would false-positive a paid tenant during a partial outage (F21 symmetry).
  if (tierResult.d1Error) {
    return { ok: true };
  }

  const tier = tierResult.tier;
  const cap = QUOTAS[tier].requestsPerMonthMax;

  // Uncapped tier (team / enterprise) → nothing to enforce; skip the write.
  if (cap === Number.MAX_SAFE_INTEGER) {
    return { ok: true };
  }

  // Atomic increment-and-check (single round trip; no read-modify-write race).
  const inc = await incrementMonthlyRequestCount(db, tenantId, true);
  if (!inc.counted) {
    // Transient store error → fail OPEN (availability); not counted.
    return { ok: true };
  }
  return requestCapResultForCount(inc.count, tier);
}

/** Lowest monthly request cap across all tiers (the FREE tier ceiling). Any
 * tenant whose post-increment count is <= this value is within EVERY tier's
 * request cap, so the request-quota gate can be cleared without knowing the
 * tenant's tier. Used by the Worker edge pipeline to short-circuit the cheap
 * request-count check ahead of the costlier tier/storage D1 lookups
 * (rt-nuclear #24). */
export const FREE_REQUEST_CAP = QUOTAS.free.requestsPerMonthMax;

/** Outcome of the atomic monthly-counter increment. */
export interface RequestCountIncrement {
  /** True iff the counter was atomically incremented and a count returned. A
   * disabled gate or a transient D1 error yields `counted:false` (fail-open;
   * the caller must NOT enforce a cap). */
  readonly counted: boolean;
  /** Post-increment count for this UTC month (0 when `counted` is false). */
  readonly count: number;
}

/**
 * Atomically increment-and-read this tenant's monthly request counter (a single
 * round trip, no read-modify-write race) and return the post-increment count.
 *
 * This is the WRITE half of {@link checkRequestQuota}, factored out so the
 * Worker edge can run the cheap counter check FIRST and short-circuit a 429 for
 * an over-cap tenant BEFORE paying the costlier tier-lookup + storage-SUM D1
 * reads (rt-nuclear #24 — D1 cost-amplification on doomed/over-cap requests).
 *
 * Posture (unchanged from the original inline logic):
 *   - `requestQuotaEnabled === false` → no write; `counted:false` (gate off).
 *   - UPSERT throws (transient store error) → `counted:false` (fail OPEN).
 * The cap COMPARISON lives in {@link requestCapResultForCount} so it can be
 * applied against either the FREE cap (short-circuit) or the tenant's resolved
 * tier cap (paid headroom) without a second write.
 */
export async function incrementMonthlyRequestCount(
  db: D1Database,
  tenantId: string,
  requestQuotaEnabled: boolean,
): Promise<RequestCountIncrement> {
  if (!requestQuotaEnabled) {
    return { counted: false, count: 0 };
  }

  const yearMonth = currentYearMonthUtc();
  const nowMs = Date.now();

  interface CountRow { request_count: number }
  let row: CountRow | null = null;
  try {
    row = await db
      .prepare(
        "INSERT INTO monthly_request_counts (tenant_id, year_month, request_count, updated_at_ms) " +
          "VALUES (?1, ?2, 1, ?3) " +
          "ON CONFLICT(tenant_id, year_month) " +
          "DO UPDATE SET request_count = request_count + 1, updated_at_ms = ?3 " +
          "RETURNING request_count",
      )
      .bind(tenantId, yearMonth, nowMs)
      .first<CountRow>();
  } catch {
    // Transient store error → fail OPEN (availability), consistent with the
    // file's posture. The request is NOT counted; the DO/container remain the
    // deeper net on mutations.
    return { counted: false, count: 0 };
  }

  // A successful RETURNING UPSERT always yields exactly one row; treat a
  // (theoretically impossible) null as 0 → within cap, fail-open.
  return { counted: true, count: row?.request_count ?? 0 };
}

/**
 * Pure cap comparison for an already-incremented monthly count against a tier's
 * `requestsPerMonthMax`. No D1 access — the increment happened in
 * {@link incrementMonthlyRequestCount}. Uncapped tiers (team / enterprise)
 * always pass.
 *
 * The cap is a maximum allowance: reject only requests BEYOND it (count > cap);
 * the request whose increment lands exactly ON the cap is still served.
 */
export function requestCapResultForCount(count: number, tier: Tier): QuotaCheckResult {
  const cap = QUOTAS[tier].requestsPerMonthMax;
  if (count <= cap) {
    return { ok: true };
  }
  return {
    ok: false,
    retryAfterSec: secondsUntilNextMonthStart(),
    reason: `Monthly request quota exceeded: ${count} requests this month, limit is ${cap} (tier: ${tier})`,
  };
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
