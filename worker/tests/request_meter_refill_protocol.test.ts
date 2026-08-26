/**
 * Shard <-> coordinator REFILL PROTOCOL invariants.
 *
 * The existing suites prove each Durable Object correct in ISOLATION:
 * request_meter_do_concurrency.miniflare.test.ts hammers the shard alone
 * ("N concurrent debits serve exactly B") and the coordinator alone
 * ("concurrent refills never grant past the cap"). Neither exercises the
 * three-hop sequence meterViaDO actually runs in production
 * (shard.debit -> coordinator.refill -> shard.applyRefill), which is NOT
 * atomic: each hop is serialized by its own object, the triple is not.
 *
 * These cells pin the two invariants request_meter_shard.ts claims across a
 * sync, calling the same pure functions the DOs call, with the same arguments
 * edge_do_meter.ts passes.
 */
import { describe, it, expect } from "vitest";
import {
  debit,
  prepareRefill,
  applyRefill,
  spentTotalOf,
  type ShardState,
} from "../src/lib/request_meter_shard";
import {
  refill,
  emptyState,
  remaining,
  type CoordinatorState,
} from "../src/lib/request_meter_lease";

const YM = "2026-08";
// Production wires lowWater: 0 (worker/src/index.ts:3322, :3345, :3588) —
// refills fire exactly when the balance empties, the highest-contention moment.
const LOW_WATER = 0;
const REGION = "iad";

/** Spend the shard has served but the coordinator has not yet folded in. */
function unreported(shard: ShardState, coord: CoordinatorState): number {
  return spentTotalOf(shard) - (coord.reconciledSpent?.[REGION] ?? 0);
}

describe("refill protocol — spend accounting across a sync", () => {
  it("does not count the same spend twice when two requests race an empty shard", () => {
    // A shard that just spent its last token: balance 0, 5 spends unreported.
    const shard: ShardState = {
      yearMonth: YM,
      balance: 0,
      spentSinceSync: 5,
      spentTotal: 5,
    };

    // Two concurrent requests both debit BEFORE either applyRefill lands.
    // Neither is served (balance 0) and neither mutates the spend counters, so
    // both snapshot an identical refill payload.
    const a = debit(shard, YM, LOW_WATER);
    const b = debit(shard, YM, LOW_WATER);
    expect(a.needsRefill && b.needsRefill).toBe(true);
    const reqA = prepareRefill(a.state);
    const reqB = prepareRefill(b.state);
    expect(reqB.spentTotal).toBe(reqA.spentTotal);

    // The coordinator serializes them and reconciles each payload in turn.
    let coord = emptyState(YM);
    coord = refill(coord, 1000, REGION, YM, reqA.spentDelta, reqA.reportedBalance, 10, reqA.spentTotal).state;
    coord = refill(coord, 1000, REGION, YM, reqB.spentDelta, reqB.reportedBalance, 10, reqB.spentTotal).state;

    // Only 5 requests were ever served.
    expect(coord.consumed).toBe(5);
  });

  // LATENT in production today: all three call sites wire lowWater: 0, so
  // reportedBalance is always 0 at refill time and no in-flight debit can be
  // served. The parameter exists precisely to be raised ("refill BEFORE the
  // balance is exhausted", request_meter_shard.ts) — this cell pins that
  // raising it stays safe.
  it("does not re-grant tokens already spent between prepareRefill and applyRefill", () => {
    const CAP = 100;
    const BLOCK = 10;
    const EAGER = 2; // refill while the shard still holds 2 tokens
    let coord = emptyState(YM);

    const seed = refill(coord, CAP, REGION, YM, 0, 0, BLOCK, 0);
    coord = seed.state;
    let shard: ShardState = {
      yearMonth: YM,
      balance: seed.newBalance,
      spentSinceSync: 0,
      spentTotal: 0,
    };

    // Spend down to the eager threshold; this debit flags needsRefill.
    while (shard.balance > EAGER) {
      shard = debit(shard, YM, EAGER).state;
    }
    const req = prepareRefill(shard); // reportedBalance = 2, spentTotal = 8

    // Two more requests are served from the remaining balance while the
    // coordinator hop is in flight.
    shard = debit(shard, YM, EAGER).state;
    shard = debit(shard, YM, EAGER).state;

    const grant = refill(coord, CAP, REGION, YM, req.spentDelta, req.reportedBalance, BLOCK, req.spentTotal);
    coord = grant.state;
    shard = applyRefill(shard, YM, grant.newBalance, grant.granted);

    // 10 requests have been served (8 down to the threshold + 2 in flight).
    // Tokens the coordinator has issued but the shard no longer holds must not
    // reappear as spendable balance, and the two in-flight spends must still be
    // owed upstream. Pre-fix this summed to 8: applyRefill zeroed the spend
    // record and handed the two spent tokens back as balance.
    expect(coord.consumed + unreported(shard, coord)).toBe(10);
    expect(coord.consumed + unreported(shard, coord) + shard.balance).toBe(
      CAP - remaining(coord, CAP),
    );
  });
});
