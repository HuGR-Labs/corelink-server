// Pure token-lease accounting for P3 edge-local request metering.
//
// NO Durable-Object / Workers-runtime deps — this is the correctness CORE that
// `RequestMeterCoordinatorDO` (the DO shell, a later WP) persists and calls. Kept
// pure so the load-bearing invariant is proven by fast unit tests, not by the DO
// runtime (same "pure Rust decision tree + thin DO shell" split as
// ReplicationCoordinatorDO). Design: docs/design/2026-08-19-adr-edge-local-do-request-metering.md
//
// Model — a per-tenant COORDINATOR hands out token LEASES to per-(tenant,region)
// SHARDS. A shard debits its local lease balance per request (edge-local, no hop)
// and refills from the coordinator when low. The coordinator is the single
// authority for one tenant's monthly cap.
//
// State (per tenant, per UTC calendar month):
//   consumed    — tokens the shards have REPORTED as actually spent (monotonic ↑
//                 within a month). These are gone for good.
//   outstanding — region → the balance that region currently HOLDS unspent
//                 (leased but not yet reported spent).
//   remaining   = max(0, cap − consumed − Σ outstanding).
//
// INVARIANTS (proven in request_meter_lease.test.ts):
//   • over-serve = 0  (STRICT): after every operation
//       consumed + Σ(outstanding) ≤ cap.
//     A shard can only serve against tokens it holds, and the sum of all tokens
//     ever handed out (consumed + still-held) never exceeds cap — so NO
//     composition of regions can serve past the monthly cap.
//   • under-serve is BOUNDED, not zero: a region is denied (granted 0) only when
//     remaining == 0, but tokens can sit idle as `outstanding` in OTHER regions.
//     Worst-case stranded ≤ Σ(idle outstanding) ≤ (#regions)·(block). Drive it → 0
//     with a small `block` and by reclaiming idle balances (`reclaimIdle`). The
//     ADR's earlier "under-serve = 0" was optimistic; this is the exact bound.

/** Coordinator state for ONE tenant + ONE UTC calendar month. Immutable. */
export interface CoordinatorState {
  /** UTC calendar month, `YYYY-MM` (matches quota.ts `new Date().toISOString().slice(0,7)`). */
  readonly yearMonth: string;
  /** Tokens reported actually-spent this month (monotonic ↑, ≤ cap). */
  readonly consumed: number;
  /** region → currently-held unspent lease balance. */
  readonly outstanding: Readonly<Record<string, number>>;
}

/** A fresh coordinator for `yearMonth` (implicit monthly reset). */
export function emptyState(yearMonth: string): CoordinatorState {
  return { yearMonth, consumed: 0, outstanding: {} };
}

/** Σ of all regions' currently-held balances. */
export function totalOutstanding(state: CoordinatorState): number {
  let sum = 0;
  for (const b of Object.values(state.outstanding)) sum += b;
  return sum;
}

/** remaining = max(0, cap − consumed − Σ outstanding). The grantable budget. */
export function remaining(state: CoordinatorState, cap: number): number {
  return Math.max(0, cap - state.consumed - totalOutstanding(state));
}

export interface RefillResult {
  /** Tokens ADDED to this region's balance (0 when the cap is reached). */
  readonly granted: number;
  /** The region's balance AFTER this refill (reportedBalance + granted). */
  readonly newBalance: number;
  /** New coordinator state (caller persists it). */
  readonly state: CoordinatorState;
}

/**
 * A region reconciles-and-refills in one step (the only mutation on the hot path).
 *
 * @param spentDelta      tokens spent since this region's last sync — moved from
 *                        its outstanding into `consumed` (clamped ≥ 0).
 * @param reportedBalance the region's CURRENT unspent balance (what it still holds)
 *                        BEFORE this refill (clamped ≥ 0).
 * @param block           the max tokens to grant this refill (the lease block `L`).
 *
 * Rolls the month (fresh state) when `yearMonth` differs. Grants
 * `min(block, remaining)` so `consumed + Σ outstanding ≤ cap` STRICTLY holds after.
 * A non-positive `cap` (shouldn't happen for real tiers) grants 0.
 */
export function refill(
  state: CoordinatorState,
  cap: number,
  region: string,
  yearMonth: string,
  spentDelta: number,
  reportedBalance: number,
  block: number,
): RefillResult {
  const base = state.yearMonth === yearMonth ? state : emptyState(yearMonth);

  // Reconcile: spent tokens become permanent `consumed`; this region now holds
  // exactly `reportedBalance`.
  const consumed = base.consumed + Math.max(0, spentDelta);
  const held = Math.max(0, reportedBalance);
  const reconciled: CoordinatorState = {
    yearMonth,
    consumed,
    outstanding: { ...base.outstanding, [region]: held },
  };

  // Grant against what is left, never past the cap.
  const grantable = remaining(reconciled, cap);
  const granted = Math.max(0, Math.min(Math.max(0, block), grantable));
  const newBalance = held + granted;

  return {
    granted,
    newBalance,
    state: {
      yearMonth,
      consumed,
      outstanding: { ...reconciled.outstanding, [region]: newBalance },
    },
  };
}

/**
 * Reclaim a region's idle balance back to the pool (bounds under-serve). Sets the
 * region's outstanding to `keep` (e.g. 0 for a fully-idle region), freeing the
 * rest for other regions to lease. Does NOT touch `consumed` — reclaiming unspent
 * tokens is not spending them. Rolls the month if `yearMonth` differs.
 */
export function reclaimIdle(
  state: CoordinatorState,
  region: string,
  yearMonth: string,
  keep: number,
): CoordinatorState {
  const base = state.yearMonth === yearMonth ? state : emptyState(yearMonth);
  return {
    yearMonth,
    consumed: base.consumed,
    outstanding: { ...base.outstanding, [region]: Math.max(0, keep) },
  };
}
