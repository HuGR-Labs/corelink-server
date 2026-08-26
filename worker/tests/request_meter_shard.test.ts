import { describe, it, expect } from "vitest";
import {
  emptyShard,
  debit,
  prepareRefill,
  applyRefill,
  type ShardState,
} from "../src/lib/request_meter_shard.js";
import {
  emptyState,
  refill,
  type CoordinatorState,
} from "../src/lib/request_meter_lease.js";

const YM = "2026-08";
const YM2 = "2026-09";

/** Deterministic PRNG (splitmix32) so the property test is reproducible. */
function prng(seed: number): () => number {
  let s = seed >>> 0;
  return () => {
    s = (s + 0x9e3779b9) >>> 0;
    let z = s;
    z = Math.imul(z ^ (z >>> 16), 0x21f0aaad) >>> 0;
    z = Math.imul(z ^ (z >>> 15), 0x735a2d97) >>> 0;
    return ((z ^ (z >>> 15)) >>> 0) / 0x100000000;
  };
}

describe("request_meter_shard — pure local shard accounting", () => {
  it("serves from balance and stops exactly at 0 (local over-serve = 0)", () => {
    let s = applyRefill(emptyShard(YM), YM, 3);
    let served = 0;
    for (let i = 0; i < 10; i++) {
      const r = debit(s, YM, 0);
      s = r.state;
      if (r.served) served++;
    }
    expect(served).toBe(3); // never serves past the leased 3 tokens
    expect(s.balance).toBe(0);
  });

  it("accumulates spentSinceSync per served request, zeroed only by refill", () => {
    let s = applyRefill(emptyShard(YM), YM, 5);
    s = debit(s, YM, 0).state;
    s = debit(s, YM, 0).state;
    expect(s.spentSinceSync).toBe(2);
    expect(s.balance).toBe(3);
    // prepareRefill reports what to send upstream, without mutating.
    const req = prepareRefill(s);
    expect(req).toEqual({ spentDelta: 2, reportedBalance: 3, spentTotal: 2 });
    expect(s.spentSinceSync).toBe(2); // unchanged by prepareRefill
    // Legacy call (no `granted`): adopts the coordinator balance and zeroes the
    // delta counter, as before. The month's cumulative total is NOT reset — the
    // coordinator dedupes against it, so resetting would let the same requests
    // be charged again on the next refill.
    s = applyRefill(s, YM, 13);
    expect(s.balance).toBe(13);
    expect(s.spentSinceSync).toBe(0);
    expect(s.spentTotal).toBe(2);
  });

  it("applyRefill ADDS the grant, so debits served mid-sync are not re-granted", () => {
    // The shard reported a balance of 3 one hop ago; the coordinator granted 10
    // against that figure (newBalance 13). Two requests were served in between.
    let s: ShardState = { yearMonth: YM, balance: 3, spentSinceSync: 2, spentTotal: 2 };
    s = debit(s, YM, 2).state;
    s = debit(s, YM, 2).state;
    expect(s.balance).toBe(1);

    s = applyRefill(s, YM, 13, 10);
    // 1 held + 10 granted — NOT the stale absolute 13, which would hand back the
    // two tokens already spent.
    expect(s.balance).toBe(11);
    expect(s.spentTotal).toBe(4);
  });

  it("a refill that crosses a month boundary starts the new month's spend at zero", () => {
    const s = applyRefill(
      { yearMonth: "2026-08", balance: 4, spentSinceSync: 6, spentTotal: 6 },
      "2026-09",
      99,
      7,
    );
    expect(s.yearMonth).toBe("2026-09");
    expect(s.balance).toBe(7); // last month's balance does not carry over
    expect(s.spentTotal).toBe(0);
  });

  it("denies and flags needsRefill when the balance is empty", () => {
    const s = emptyShard(YM);
    const r = debit(s, YM, 0);
    expect(r.served).toBe(false);
    expect(r.needsRefill).toBe(true);
    expect(r.state.balance).toBe(0);
  });

  it("flags needsRefill at the low-water mark while still serving", () => {
    let s = applyRefill(emptyShard(YM), YM, 3);
    // lowWater=1: served with post-debit balance 2 → no refill yet.
    let r = debit(s, YM, 1);
    expect(r.served).toBe(true);
    expect(r.needsRefill).toBe(false);
    s = r.state;
    // post-debit balance 1 ≤ lowWater 1 → refill flagged, still served.
    r = debit(s, YM, 1);
    expect(r.served).toBe(true);
    expect(r.needsRefill).toBe(true);
  });

  it("drops a stale-month balance to 0 and forces a refill (month isolation)", () => {
    let s = applyRefill(emptyShard(YM), YM, 100);
    expect(s.balance).toBe(100);
    const r = debit(s, YM2, 0); // new month arrives
    expect(r.served).toBe(false);
    expect(r.needsRefill).toBe(true);
    expect(r.state).toEqual(emptyShard(YM2)); // last month's 100 do NOT carry
  });

  it("applyRefill clamps a negative balance to 0", () => {
    const s = applyRefill(emptyShard(YM), YM, -5);
    expect(s.balance).toBe(0);
  });

  // The load-bearing END-TO-END property: N shards driving one coordinator can
  // never collectively SERVE past the monthly cap. This composes the shard's
  // local over-serve=0 with the coordinator's consumed+Σoutstanding≤cap.
  it("composed: no set of regions serves past the cap (40 seeds × 400 steps)", () => {
    const regions = ["iad", "lhr", "nrt", "syd"];
    for (let seed = 1; seed <= 40; seed++) {
      const rnd = prng(seed);
      const cap = 200 + Math.floor(rnd() * 4_800); // 200..5000
      const block = 10 + Math.floor(rnd() * 200); // 10..210
      const lowWater = Math.floor(rnd() * 5); // 0..4

      let coord: CoordinatorState = emptyState(YM);
      const shards: Record<string, ShardState> = {};
      for (const r of regions) shards[r] = emptyShard(YM);
      let servedTotal = 0;

      for (let step = 0; step < 400; step++) {
        const region = regions[Math.floor(rnd() * regions.length)];
        // Try to serve one request from this region's shard.
        const d = debit(shards[region], YM, lowWater);
        shards[region] = d.state;
        if (d.served) servedTotal++;

        // Refill on demand (empty) or eagerly at low-water.
        if (d.needsRefill) {
          const req = prepareRefill(shards[region]);
          const rr = refill(
            coord,
            cap,
            region,
            YM,
            req.spentDelta,
            req.reportedBalance,
            block,
          );
          coord = rr.state;
          shards[region] = applyRefill(shards[region], YM, rr.newBalance);
        }

        // The invariant, after every single step: total ACTUALLY SERVED
        // requests never exceeds the monthly cap.
        expect(servedTotal).toBeLessThanOrEqual(cap);
      }
    }
  });
});
