import { describe, it, expect } from "vitest";
import {
  emptyState,
  refill,
  reclaimIdle,
  remaining,
  totalOutstanding,
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

/** The load-bearing safety property: over-serve = 0. */
function assertNeverOverCap(state: CoordinatorState, cap: number): void {
  expect(state.consumed + totalOutstanding(state)).toBeLessThanOrEqual(cap);
}

describe("request_meter_lease — pure token-lease accounting", () => {
  it("single region: grants the block until the cap, then grants exactly 0", () => {
    const cap = 25_000;
    const block = 10_000;
    let s = emptyState(YM);
    let bal = 0;

    // 1st refill: full block.
    let r = refill(s, cap, "iad", YM, 0, bal, block);
    expect(r.granted).toBe(10_000);
    (s = r.state), (bal = r.newBalance);
    assertNeverOverCap(s, cap);

    // 2nd: full block (20k ≤ 25k).
    r = refill(s, cap, "iad", YM, 0, bal, block);
    expect(r.granted).toBe(10_000);
    (s = r.state), (bal = r.newBalance);

    // 3rd: only 5k left (cap 25k − 20k held).
    r = refill(s, cap, "iad", YM, 0, bal, block);
    expect(r.granted).toBe(5_000);
    (s = r.state), (bal = r.newBalance);
    expect(bal).toBe(25_000);

    // 4th: cap reached → 0.
    r = refill(s, cap, "iad", YM, 0, bal, block);
    expect(r.granted).toBe(0);
    assertNeverOverCap(r.state, cap);
  });

  it("spentDelta moves outstanding → consumed and frees budget for the same region", () => {
    const cap = 20_000;
    let s = emptyState(YM);
    // Hold a full 20k block.
    let r = refill(s, cap, "iad", YM, 0, 0, 20_000);
    expect(r.granted).toBe(20_000);
    s = r.state;
    expect(remaining(s, cap)).toBe(0);

    // Spend 15k, report holding 5k, ask for more: consumed=15k, remaining=0 still
    // (15k consumed + 5k held = 20k), so no new grant.
    r = refill(s, cap, "iad", YM, 15_000, 5_000, 10_000);
    expect(r.state.consumed).toBe(15_000);
    expect(r.granted).toBe(0);
    assertNeverOverCap(r.state, cap);

    // Spend the last 5k, hold 0 → remaining 0 (all 20k consumed) → still 0.
    r = refill(r.state, cap, "iad", YM, 5_000, 0, 10_000);
    expect(r.state.consumed).toBe(20_000);
    expect(r.granted).toBe(0);
  });

  it("two regions cannot collectively lease past the cap", () => {
    const cap = 15_000;
    const block = 10_000;
    let s = emptyState(YM);

    let a = refill(s, cap, "iad", YM, 0, 0, block); // 10k to iad
    expect(a.granted).toBe(10_000);
    s = a.state;

    let b = refill(s, cap, "nrt", YM, 0, 0, block); // only 5k left for nrt
    expect(b.granted).toBe(5_000);
    s = b.state;
    assertNeverOverCap(s, cap);
    expect(totalOutstanding(s)).toBe(15_000);

    // A third region gets nothing.
    const c = refill(s, cap, "lhr", YM, 0, 0, block);
    expect(c.granted).toBe(0);
    assertNeverOverCap(c.state, cap);
  });

  it("PROPERTY (over-serve = 0): random refills across regions never exceed the cap", () => {
    const regions = ["iad", "nrt", "lhr", "sam", "syd"];
    for (let seed = 1; seed <= 40; seed++) {
      const rnd = prng(seed);
      const cap = 1 + Math.floor(rnd() * 500_000);
      const block = 1 + Math.floor(rnd() * 20_000);
      let s = emptyState(YM);
      const held: Record<string, number> = {};
      for (const r of regions) held[r] = 0;

      for (let step = 0; step < 300; step++) {
        const region = regions[Math.floor(rnd() * regions.length)]!;
        // Spend a random slice of what this region holds, report the rest.
        const spend = Math.floor(rnd() * (held[region]! + 1));
        const reported = held[region]! - spend;
        const r = refill(s, cap, region, YM, spend, reported, block);
        s = r.state;
        held[region] = r.newBalance;
        // The invariant must hold after EVERY step.
        assertNeverOverCap(s, cap);
        // A shard's held balance is always non-negative and ≤ cap.
        expect(held[region]!).toBeGreaterThanOrEqual(0);
        expect(held[region]!).toBeLessThanOrEqual(cap);
      }
    }
  });

  it("month rollover: a new yearMonth resets consumed + all outstanding", () => {
    const cap = 20_000;
    let r = refill(emptyState(YM), cap, "iad", YM, 5_000, 0, 10_000);
    r = refill(r.state, cap, "iad", YM, 0, r.newBalance, 0); // just to carry state
    expect(r.state.consumed).toBe(5_000);
    expect(totalOutstanding(r.state)).toBeGreaterThan(0);

    // First refill of the NEXT month → fresh budget.
    const next = refill(r.state, cap, "iad", YM2, 0, 0, 10_000);
    expect(next.state.yearMonth).toBe(YM2);
    expect(next.state.consumed).toBe(0);
    expect(next.granted).toBe(10_000); // full block available again
    assertNeverOverCap(next.state, cap);
  });

  it("uncapped tier (cap = MAX_SAFE_INTEGER) always grants the full block", () => {
    const cap = Number.MAX_SAFE_INTEGER;
    let s = emptyState(YM);
    for (let i = 0; i < 50; i++) {
      const r = refill(s, cap, "iad", YM, 0, 0, 100_000);
      expect(r.granted).toBe(100_000);
      s = r.state;
    }
    assertNeverOverCap(s, cap);
  });

  it("under-serve is BOUNDED by idle outstanding; reclaimIdle frees it (no consumed change)", () => {
    const cap = 15_000;
    const block = 10_000;
    // iad leases 10k and sits idle (spends nothing); nrt then gets only 5k.
    let s = refill(emptyState(YM), cap, "iad", YM, 0, 0, block).state;
    let nrt = refill(s, cap, "nrt", YM, 0, 0, block);
    expect(nrt.granted).toBe(5_000); // nrt is under-served: 5k stranded idle in iad
    s = nrt.state;
    const consumedBefore = s.consumed;

    // Reclaim iad's idle balance → nrt can now lease the freed 10k.
    s = reclaimIdle(s, "iad", YM, 0);
    expect(s.consumed).toBe(consumedBefore); // reclaim never spends
    // nrt holds 5k, iad reclaimed to 0 → Σoutstanding 5k, consumed 0 → remaining 10k.
    expect(remaining(s, cap)).toBe(10_000);

    const more = refill(s, cap, "nrt", YM, 0, 5_000, block);
    expect(more.granted).toBe(10_000); // now fully served
    assertNeverOverCap(more.state, cap);
  });

  it("clamps hostile inputs (negative spend/balance/block) and never over-grants", () => {
    const cap = 10_000;
    const r = refill(emptyState(YM), cap, "iad", YM, -999, -50, -100);
    expect(r.granted).toBe(0);
    expect(r.newBalance).toBe(0);
    assertNeverOverCap(r.state, cap);
  });
});
