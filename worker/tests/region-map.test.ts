import { describe, it, expect } from "vitest";
import {
  coloForMacro,
  isMacroRegion,
  isProvisionedMacro,
  isLocalIadMacro,
  PROVISIONED_MACROS,
  type MacroRegion,
} from "../src/region-map.js";

// backlog #29 — this is the WORKER half of the cross-language lock test. The
// Rust mirror is `crates/corelink-container/src/storage/region_map.rs`
// (`tests::frozen_macro_to_colo_map_is_exact` + `provisioned_set_is_exact`).
// Both MUST assert the SAME frozen map; if one changes, the other fails.
describe("region-map (FROZEN macro→colo contract)", () => {
  it("maps every macro to its frozen colo (or undefined for afr/unknown)", () => {
    expect(coloForMacro("wnam")).toBe("iad");
    expect(coloForMacro("enam")).toBe("iad");
    expect(coloForMacro("weur")).toBe("lhr");
    expect(coloForMacro("sam")).toBe("sam");
    expect(coloForMacro("apac")).toBe("nrt");
    // afr → undefined (no provisioned colo → reject).
    expect(coloForMacro("afr")).toBeUndefined();
    // unknown → undefined (fail-closed; NEVER a silent IAD fall-through).
    expect(coloForMacro("zzz")).toBeUndefined();
    expect(coloForMacro("")).toBeUndefined();
    expect(coloForMacro("lhr")).toBeUndefined(); // colo, not a macro → reject
  });

  it("recognises exactly the six canonical macro codes", () => {
    for (const m of ["wnam", "enam", "weur", "sam", "apac", "afr"]) {
      expect(isMacroRegion(m)).toBe(true);
    }
    expect(isMacroRegion("iad")).toBe(false);
    expect(isMacroRegion("")).toBe(false);
  });

  it("provisioned set = {wnam, enam, weur, sam}", () => {
    const expected: MacroRegion[] = ["wnam", "enam", "weur", "sam"];
    expect([...PROVISIONED_MACROS].sort()).toEqual([...expected].sort());
    for (const m of expected) expect(isProvisionedMacro(m)).toBe(true);
    expect(isProvisionedMacro("apac")).toBe(false);
    expect(isProvisionedMacro("afr")).toBe(false);
  });

  it("every provisioned macro resolves to a colo (no unservable provisioned region)", () => {
    for (const m of PROVISIONED_MACROS) {
      expect(coloForMacro(m)).toBeDefined();
    }
  });

  it("isLocalIadMacro is true exactly for wnam/enam", () => {
    expect(isLocalIadMacro("wnam")).toBe(true);
    expect(isLocalIadMacro("enam")).toBe(true);
    expect(isLocalIadMacro("weur")).toBe(false);
    expect(isLocalIadMacro("sam")).toBe(false);
    expect(isLocalIadMacro("afr")).toBe(false);
  });
});
