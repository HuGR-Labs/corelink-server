import { describe, it, expect } from "vitest";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, resolve } from "node:path";
import {
  coloForMacro,
  isMacroRegion,
  isProvisionedMacro,
  isLocalIadMacro,
  PROVISIONED_MACROS,
  type MacroRegion,
} from "../src/region-map.js";

/** The CANONICAL provisionable macro set — the one truth all three copies obey. */
const CANONICAL_PROVISIONED = ["enam", "weur", "wnam"]; // sorted

const __dirname = dirname(fileURLToPath(import.meta.url));

/**
 * Extract the string literals from a `PROVISIONED_MACROS` definition in a source
 * file (TS `new Set([...])` or Rust `[&str; N] = [...]`), sorted. Used to diff
 * the THREE physical copies of the set against each other and the canonical.
 */
function parseProvisioned(absPath: string): string[] {
  const src = readFileSync(absPath, "utf8");
  // Anchor on the `… = […]` ASSIGNMENT (not a doc-comment mention, and skipping
  // Rust's `[&str; N]` TYPE-annotation bracket which precedes the `=`).
  const block = src.match(/PROVISIONED_MACROS[^=\n]*=[^[]*\[([^\]]*)\]/);
  if (!block) throw new Error(`no PROVISIONED_MACROS array found in ${absPath}`);
  const tokens = [...block[1].matchAll(/"([a-z]+)"/g)].map((m) => m[1]);
  if (tokens.length === 0) {
    throw new Error(`empty PROVISIONED_MACROS array parsed from ${absPath}`);
  }
  return tokens.sort();
}

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

  it("provisioned set = {wnam, enam, weur} (sam removed — see LGPD trap below)", () => {
    const expected: MacroRegion[] = ["wnam", "enam", "weur"];
    expect([...PROVISIONED_MACROS].sort()).toEqual([...expected].sort());
    for (const m of expected) expect(isProvisionedMacro(m)).toBe(true);
    // sam/apac/afr are valid macros but NOT provisionable.
    expect(isProvisionedMacro("sam")).toBe(false);
    expect(isProvisionedMacro("apac")).toBe(false);
    expect(isProvisionedMacro("afr")).toBe(false);
  });

  it("sam stays a recognised, ROUTABLE macro even though it is not provisionable", () => {
    // Routing must still recognise sam (it is a valid D1 macro + has a colo);
    // only PROVISIONING is blocked, so no tenant whose data would mis-land in
    // US R2 (PROD_SAM still uses the default US bucket) can ever be created.
    expect(isMacroRegion("sam")).toBe(true);
    expect(coloForMacro("sam")).toBe("sam");
    expect(isProvisionedMacro("sam")).toBe(false);
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

// backlog #29 / H1 — the 3-WAY drift gate. The provisionable macro set is
// physically duplicated in THREE consumers; parse each copy from source and
// assert they are byte-identical to the canonical {wnam, enam, weur}. This makes
// the clerk drift (the 2-set that masked an LGPD trap) impossible to reintroduce
// silently — any divergence in ANY of the three files fails CI here.
describe("region-map (3-way PROVISIONED_MACROS drift gate)", () => {
  const workerSrc = resolve(__dirname, "../src/region-map.ts");
  const containerSrc = resolve(
    __dirname,
    "../../crates/corelink-container/src/storage/region_map.rs",
  );
  const clerkSrc = resolve(
    __dirname,
    "../../apps/signup-worker/src/webhooks/clerk.ts",
  );

  it("the worker runtime set equals the canonical set", () => {
    expect([...PROVISIONED_MACROS].sort()).toEqual(CANONICAL_PROVISIONED);
  });

  it("all three source copies (worker + container + clerk) match the canonical set", () => {
    const worker = parseProvisioned(workerSrc);
    const container = parseProvisioned(containerSrc);
    const clerk = parseProvisioned(clerkSrc);
    expect(worker).toEqual(CANONICAL_PROVISIONED);
    expect(container).toEqual(CANONICAL_PROVISIONED);
    expect(clerk).toEqual(CANONICAL_PROVISIONED);
    // ...and therefore to each other (explicit cross-diff for a clear failure).
    expect(container).toEqual(worker);
    expect(clerk).toEqual(worker);
  });

  it("none of the three copies provisions `sam` (the LGPD cross-border trap)", () => {
    for (const p of [workerSrc, containerSrc, clerkSrc]) {
      expect(parseProvisioned(p)).not.toContain("sam");
    }
  });
});
