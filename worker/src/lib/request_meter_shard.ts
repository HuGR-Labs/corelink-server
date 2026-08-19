// Pure local-shard accounting for P3 edge-local request metering.
//
// NO Durable-Object / Workers-runtime deps — the correctness CORE that
// `RequestMeterShardDO` (the DO shell, same WP) persists and calls. Same
// "pure decision tree + thin DO shell" split as `request_meter_lease.ts`
// (the coordinator side). Design:
// docs/design/2026-08-19-adr-edge-local-do-request-metering.md
//
// Model — a per-(tenant,region) SHARD holds a local lease `balance` handed to
// it by the per-tenant COORDINATOR (`request_meter_lease.ts`). Each request
// debits the local balance with NO cross-DO hop; the shard refills from the
// coordinator when it runs low, reporting what it spent since the last sync.
//
// State (per tenant+region, per UTC calendar month):
//   yearMonth      — the UTC month this balance belongs to (`YYYY-MM`).
//   balance        — unspent leased tokens the shard currently holds (≥ 0).
//   spentSinceSync — tokens debited since the last coordinator sync (≥ 0). Sent
//                    to the coordinator as `spentDelta` on the next refill, then
//                    reset to 0 (they become the coordinator's `consumed`).
//
// INVARIANTS (proven in request_meter_shard.test.ts):
//   • local over-serve = 0: `debit` serves a request ONLY when `balance > 0`,
//     and then decrements by exactly 1 — the shard can never serve past the
//     tokens the coordinator leased it. Composed with the coordinator's
//     `consumed + Σoutstanding ≤ cap`, no region serves past the monthly cap.
//   • month isolation: a `debit` in a NEW `yearMonth` first drops the stale
//     balance to 0 (last month's lease never carries over) and forces a refill.
//   • spend accounting is exact: `spentSinceSync` accumulates every served
//     request and is zeroed only by `applyRefill` (which reports it upstream),
//     so no debit is ever double-counted or lost across a sync.

/** Local shard state for ONE tenant+region + ONE UTC calendar month. Immutable. */
export interface ShardState {
  /** UTC calendar month, `YYYY-MM` (matches quota.ts month key). */
  readonly yearMonth: string;
  /** Unspent leased tokens held locally (≥ 0). */
  readonly balance: number;
  /** Tokens debited since the last coordinator sync (≥ 0). */
  readonly spentSinceSync: number;
}

/** A fresh, empty shard for `yearMonth` (no balance, nothing to report). */
export function emptyShard(yearMonth: string): ShardState {
  return { yearMonth, balance: 0, spentSinceSync: 0 };
}

export interface DebitResult {
  /** True iff a token was available and the request may be served. */
  readonly served: boolean;
  /**
   * True iff the shard should refill from the coordinator: either it just
   * denied (balance hit 0) or it is at/below `lowWater` and should top up
   * asynchronously while still serving from the remaining balance.
   */
  readonly needsRefill: boolean;
  /** New shard state (caller persists it). */
  readonly state: ShardState;
}

/**
 * Attempt to serve ONE request from the local balance.
 *
 * @param lowWater  refill-eagerly threshold: when the post-debit balance is
 *                  ≤ lowWater the result flags `needsRefill` so the shell can
 *                  async-refill BEFORE the balance is exhausted (keeps serving
 *                  with no hop on the hot path). Use 0 to refill only on empty.
 *
 * A `debit` whose `yearMonth` differs from the state's rolls the month first:
 * the stale balance is dropped to 0 (last month's lease does not carry over),
 * so the request is denied and a refill is forced.
 */
export function debit(
  state: ShardState,
  yearMonth: string,
  lowWater: number,
): DebitResult {
  // Month rollover — stale balance belongs to last month, drop it.
  if (state.yearMonth !== yearMonth) {
    return {
      served: false,
      needsRefill: true,
      state: emptyShard(yearMonth),
    };
  }
  if (state.balance <= 0) {
    return {
      served: false,
      needsRefill: true,
      state: { ...state, balance: 0 },
    };
  }
  const balance = state.balance - 1;
  return {
    served: true,
    needsRefill: balance <= Math.max(0, lowWater),
    state: {
      yearMonth,
      balance,
      spentSinceSync: state.spentSinceSync + 1,
    },
  };
}

export interface RefillRequest {
  /** Tokens spent since the last sync — the coordinator's `spentDelta`. */
  readonly spentDelta: number;
  /** The balance the shard still holds — the coordinator's `reportedBalance`. */
  readonly reportedBalance: number;
}

/**
 * The values to send to the coordinator's `refill` op. Pure read of the current
 * state — does NOT mutate; the mutation happens in `applyRefill` once the
 * coordinator has authoritatively reconciled and granted.
 */
export function prepareRefill(state: ShardState): RefillRequest {
  return { spentDelta: state.spentSinceSync, reportedBalance: state.balance };
}

/**
 * Apply the coordinator's refill response. The coordinator is authoritative for
 * the balance (it returns `newBalance = reportedBalance + granted`), so the
 * shard adopts it verbatim and zeroes `spentSinceSync` (those spends are now the
 * coordinator's `consumed`). `yearMonth` is set to the coordinator's month, so a
 * refill that crossed a month boundary lands the shard in the new month cleanly.
 */
export function applyRefill(
  state: ShardState,
  yearMonth: string,
  newBalance: number,
): ShardState {
  return {
    yearMonth,
    balance: Math.max(0, newBalance),
    spentSinceSync: 0,
  };
}
