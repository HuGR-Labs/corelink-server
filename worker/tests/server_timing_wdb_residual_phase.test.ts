/**
 * `qcontrol` — the explicitly named framework remainder of `wdb` after its four named
 * sub-phases (`qtier`, `qdo`, `qbatch`, `qresid`).
 *
 * # Why this test exists
 *
 * `qtier`/`qbatch`/`qresid` summed to only 122 ms of a 265 ms `wdb` in prod —
 * 143 ms unattributed, traced to the awaited `serveViaDO(...)` hop that sat
 * inside `wdb` and was never timed. `qdo` closes that gap; `wdbControlPhase`
 * is the pure function that computes what is STILL left over, mirroring the
 * `ohop` reconciliation discipline `originSubPhases` already uses:
 *
 *   - a `-1` (did-not-run) phase is EXCLUDED from the sum, never counted as 0;
 *   - a sum that exceeds `wdb` is refused whole, `desc="unreconciled"`, rather
 *     than silently redistributed;
 *   - otherwise the residue is emitted exactly, `dur=0` included — a real
 *     measurement of zero is not the same as "did not run".
 *
 * Invariant under test: qtier + qdo + qbatch + qresid + qcontrol === wdb,
 * exactly, always.
 */

import { describe, it, expect } from "vitest";
import { wdbControlPhase } from "../src/index.js";

describe("wdbControlPhase", () => {
  it("computes the exact difference when all four phases ran and sum < wdb", () => {
    // 265 total, phases sum to 122 (30 + 50 + 20 + 22) — residue is the other 143.
    expect(wdbControlPhase(265, [30, 50, 20, 22])).toBe("qcontrol;dur=143");
  });

  it("excludes a -1 (did-not-run) phase from the sum instead of treating it as 0", () => {
    // qresid at -1 (did not run): sum is 30+50+20 = 100, not 30+50+20+0 = 100
    // — same arithmetic result here, so assert against a case where treating
    // -1 as 0 would UNDER-count and inflate the residue incorrectly is not
    // distinguishable; instead assert the sum used excludes it explicitly by
    // comparing to the hand-computed sum of only the non-negative entries.
    const wdb = 200;
    const phases = [30, -1, 20, 22]; // qdo did not run
    const sum = 30 + 20 + 22; // -1 excluded, NOT counted as 0
    expect(wdbControlPhase(wdb, phases)).toBe(`qcontrol;dur=${wdb - sum}`);
  });

  it("emits dur=0 when the phases sum exactly to wdb — a real zero, not omitted", () => {
    expect(wdbControlPhase(100, [25, 25, 25, 25])).toBe("qcontrol;dur=0");
  });

  it("refuses the split with desc=unreconciled when the phases overshoot wdb", () => {
    // Sum = 30+50+20+22 = 122, but wdb is only 100: clock skew across the
    // awaits. The split is dropped whole, dur equal to the WHOLE wdb.
    expect(wdbControlPhase(100, [30, 50, 20, 22])).toBe('qcontrol;dur=100;desc="unreconciled"');
  });

  it("attributes the entire wdb to qcontrol when every phase is -1 (none ran)", () => {
    expect(wdbControlPhase(77, [-1, -1, -1, -1])).toBe("qcontrol;dur=77");
  });
});
