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
//   • spend accounting is exact: `spentTotal` accumulates every served request
//     for the month and is NEVER reset by a sync, so no debit is double-counted
//     or lost across one. The coordinator reconciles against its own high-water
//     mark per region, which makes a repeated refill a no-op rather than a
//     second charge. (`spentSinceSync` is the pre-2026-08-26 delta field, kept
//     on the wire only so a shard and a coordinator running different code
//     during a rollout still agree; it is not the accounting source.)

/** Local shard state for ONE tenant+region + ONE UTC calendar month. Immutable. */
export interface ShardState {
  /** UTC calendar month, `YYYY-MM` (matches quota.ts month key). */
  readonly yearMonth: string;
  /** Unspent leased tokens held locally (≥ 0). */
  readonly balance: number;
  /** Tokens debited since the last coordinator sync (≥ 0). ROLLOUT-COMPAT
   * ONLY — a delta is not idempotent under a retried or raced refill; read
   * {@link spentTotalOf} for the accounting value. */
  readonly spentSinceSync: number;
  /**
   * Tokens debited this month, cumulative and monotonic — reset only by a month
   * rollover, never by a sync. Absent on state persisted before 2026-08-26;
   * {@link spentTotalOf} folds that legacy shape in without a migration.
   */
  readonly spentTotal?: number;
}

/**
 * The month's cumulative spend. Legacy state (no `spentTotal`) carries its
 * unreported spend in `spentSinceSync`, which is exactly the amount the
 * coordinator has not yet reconciled — so adopting it as the starting
 * high-water mark keeps the first post-upgrade refill exact.
 */
export function spentTotalOf(state: ShardState): number {
  return state.spentTotal ?? state.spentSinceSync;
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
      spentTotal: spentTotalOf(state) + 1,
    },
  };
}

export interface RefillRequest {
  /** Tokens spent since the last sync — the coordinator's legacy `spentDelta`. */
  readonly spentDelta: number;
  /** The balance the shard still holds — the coordinator's `reportedBalance`. */
  readonly reportedBalance: number;
  /**
   * The month's cumulative spend. The coordinator reconciles against this
   * (its own per-region high-water mark), so replaying an identical refill —
   * two requests racing an empty shard both snapshot the same payload —
   * contributes 0 instead of charging the same requests twice.
   */
  readonly spentTotal: number;
}

/**
 * The values to send to the coordinator's `refill` op. Pure read of the current
 * state — does NOT mutate; the mutation happens in `applyRefill` once the
 * coordinator has authoritatively reconciled and granted.
 */
export function prepareRefill(state: ShardState): RefillRequest {
  return {
    spentDelta: state.spentSinceSync,
    reportedBalance: state.balance,
    spentTotal: spentTotalOf(state),
  };
}

/**
 * Apply the coordinator's refill response.
 *
 * The grant is ADDED to whatever the shard holds right now — it is not the
 * absolute balance the coordinator computed. Between the debit that snapshotted
 * `reportedBalance` and this call, other requests may have been served from the
 * same shard; adopting `newBalance` verbatim would hand those already-spent
 * tokens back as spendable balance (an over-serve past the cap) and wipe the
 * spend record along with it.
 *
 * `spentTotal` is deliberately NOT reset: it is the month's cumulative spend and
 * the coordinator dedupes against its own high-water mark. `spentSinceSync` is
 * still zeroed for the rollout-compat wire field.
 *
 * `yearMonth` is the coordinator's month, so a refill that crossed a month
 * boundary lands the shard in the new month cleanly — with a fresh spend
 * counter, since last month's total does not belong to it.
 *
 * @param granted tokens the coordinator added. Omit ONLY when talking to a
 *                coordinator old enough not to report it, in which case
 *                `newBalance` is adopted as before.
 */
export function applyRefill(
  state: ShardState,
  yearMonth: string,
  newBalance: number,
  granted?: number,
): ShardState {
  if (granted === undefined || !Number.isFinite(granted)) {
    return {
      yearMonth,
      balance: Math.max(0, newBalance),
      spentSinceSync: 0,
      spentTotal: state.yearMonth === yearMonth ? spentTotalOf(state) : 0,
    };
  }
  if (state.yearMonth !== yearMonth) {
    return {
      yearMonth,
      balance: Math.max(0, granted),
      spentSinceSync: 0,
      spentTotal: 0,
    };
  }
  return {
    yearMonth,
    balance: Math.max(0, state.balance + Math.max(0, granted)),
    spentSinceSync: 0,
    spentTotal: spentTotalOf(state),
  };
}
