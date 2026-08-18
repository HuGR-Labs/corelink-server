// Unit test for the DO location-hint mapping (multi-region container-serving
// bug, 2026-08-18). A hint-less `CORELINK_SERVER.get()` homes a brand-new DO —
// and its attached Rust container — at the caller's entry colo. Because the
// region fan-out is a CO-LOCATED Service Binding (the regional Worker runs in
// the caller's colo, not its named region), that left EU/APAC tenant containers
// homing outside their region and failing to start. `doLocationHintForRegion`
// pins each Worker's DOs to its own region's CF location.
import { describe, it, expect } from "vitest";
import { doLocationHintForRegion } from "../src/index.js";

describe("doLocationHintForRegion", () => {
  it("maps each serving colo to its CF DO location hint", () => {
    expect(doLocationHintForRegion("iad")).toBe("enam");
    expect(doLocationHintForRegion("lhr")).toBe("weur");
    expect(doLocationHintForRegion("nrt")).toBe("apac");
    expect(doLocationHintForRegion("syd")).toBe("oc");
  });

  it("pins sam to enam — Cloudflare has no SAM region (platform limit)", () => {
    // sam-labelled data physically lands in US R2 today; the DO+container must
    // start in a supported container metro, not home non-deterministically at
    // the caller's entry colo.
    expect(doLocationHintForRegion("sam")).toBe("enam");
  });

  it("returns undefined for unknown/unset region → bare .get(id), no worse than today", () => {
    expect(doLocationHintForRegion(undefined)).toBeUndefined();
    expect(doLocationHintForRegion("")).toBeUndefined();
    expect(doLocationHintForRegion("gru")).toBeUndefined();
  });
});
