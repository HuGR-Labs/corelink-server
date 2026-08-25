import { describe, expect, it } from "vitest";
import { quotaPathFor } from "../src/index.js";

// The composition rule between the two edge quota optimizations. The regression
// this file exists to prevent shipped once: `EDGE_DO_METER="serve"` (live in
// every prod region) disabled `EDGE_ASYNC_METER="on"` for every capped tier,
// so every warm authed READ paid a synchronous D1 storage SUM — measured at
// 35-51 ms from colo=IAD and 115-124 ms from São Paulo.
describe("quotaPathFor", () => {
  const eligible = { asyncMeterEligible: true } as const;

  it("composes the DO serve verdict with the async fast path", () => {
    expect(
      quotaPathFor({ serveHandled: true, asyncMeterMode: "on", ...eligible }),
    ).toBe("serve-fast");
  });

  it("keeps the plain fast path when the DO did not meter", () => {
    expect(
      quotaPathFor({ serveHandled: false, asyncMeterMode: "on", ...eligible }),
    ).toBe("fast");
  });

  it("never leaves the exact path for an ineligible request, whatever the flags", () => {
    for (const mode of [undefined, "off", "shadow", "on"]) {
      for (const serveHandled of [true, false]) {
        expect(
          quotaPathFor({ serveHandled, asyncMeterMode: mode, asyncMeterEligible: false }),
        ).toBe("exact");
      }
    }
  });

  it("canaries on shadow and serves the exact path", () => {
    expect(
      quotaPathFor({ serveHandled: false, asyncMeterMode: "shadow", ...eligible }),
    ).toBe("shadow");
    expect(
      quotaPathFor({ serveHandled: true, asyncMeterMode: "shadow", ...eligible }),
    ).toBe("shadow");
  });

  it("falls back to the exact path when the flag is unset or unknown", () => {
    expect(quotaPathFor({ serveHandled: true, asyncMeterMode: undefined, ...eligible })).toBe(
      "exact",
    );
    expect(quotaPathFor({ serveHandled: false, asyncMeterMode: "1", ...eligible })).toBe("exact");
  });
});
