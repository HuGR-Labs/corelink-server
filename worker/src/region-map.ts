/**
 * CoreLink data-residency region map — SINGLE SOURCE OF TRUTH (worker side).
 *
 * MIRRORED in `crates/corelink-container/src/storage/region_map.rs`. The two
 * MUST agree byte-for-byte on the macro→colo mapping and the provisioned set;
 * the worker routing-test + the container lock-consistency test both pin this.
 *
 * Background (backlog #29 — a live Schrems II leak): EU tenants' data was
 * landing in the US region because the edge fan-out routed by literal colo
 * codes ("lhr"/"nrt"/"syd") while D1's `tenant.primary_region` only ever holds
 * the MACRO codes (wnam/enam/weur/sam/apac/afr). `weur` therefore never matched
 * "lhr" and the EU tenant stayed on IAD (US storage). This map removes the
 * vocabulary mismatch: routing keys off the MACRO code → colo deterministically.
 *
 * FROZEN macro→colo map (lead's decision — do NOT change without a residency
 * ADR + the mirror in region_map.rs):
 *   wnam → iad   (US west traffic served from IAD today)
 *   enam → iad   (US east — IAD)
 *   weur → lhr   (EU — London; the leak this fix closes)
 *   sam  → sam   (South America)
 *   apac → nrt   (Asia-Pacific — Tokyo; NOT provisioned in Phase 1)
 *   afr  → REJECT (no African colo provisioned; must be rejected at signup)
 *
 * Provisioned Phase-1 = { wnam, enam, weur, sam }. apac/afr are valid macro
 * codes (the D1 CHECK accepts them) but are NOT provisioned, so signup MUST
 * reject them rather than silently downgrade to a US region.
 */

/** The canonical CoreLink data-residency MACRO region codes (D1 CHECK set). */
export type MacroRegion = "wnam" | "enam" | "weur" | "sam" | "apac" | "afr";

/** Cloudflare colo codes CoreLink routes residency traffic to. */
export type Colo = "iad" | "lhr" | "sam" | "nrt";

/**
 * FROZEN macro→colo map. `afr` is intentionally ABSENT (no provisioned colo →
 * reject at signup). `coloForMacro` returns `undefined` for `afr` and for any
 * unknown string so callers must handle the reject branch explicitly.
 */
const MACRO_TO_COLO: Readonly<Record<MacroRegion, Colo | undefined>> = {
  wnam: "iad",
  enam: "iad",
  weur: "lhr",
  sam: "sam",
  apac: "nrt",
  afr: undefined,
};

/**
 * Macro regions provisioned in Phase 1. Signup MUST reject any macro NOT in
 * this set (apac/afr today) rather than route/store the tenant anywhere.
 */
export const PROVISIONED_MACROS: ReadonlySet<MacroRegion> = new Set<MacroRegion>([
  "wnam",
  "enam",
  "weur",
  "sam",
]);

/** Type guard: is `s` one of the six canonical macro region codes? */
export function isMacroRegion(s: string): s is MacroRegion {
  return (
    s === "wnam" ||
    s === "enam" ||
    s === "weur" ||
    s === "sam" ||
    s === "apac" ||
    s === "afr"
  );
}

/**
 * Map a macro region code to its serving colo, or `undefined` when the macro is
 * `afr` (no colo) or the input is not a recognised macro code. Callers in the
 * residency path MUST treat `undefined` as a hard reject / fail-closed, NEVER as
 * "fall through to IAD" (that is the cross-border leak this fix closes).
 */
export function coloForMacro(macro: string): Colo | undefined {
  if (!isMacroRegion(macro)) return undefined;
  return MACRO_TO_COLO[macro];
}

/** Is the macro a region whose serving colo is IAD (the local US path)? */
export function isLocalIadMacro(macro: string): boolean {
  return coloForMacro(macro) === "iad";
}

/** True iff the macro is provisioned in Phase 1 (signup-acceptable). */
export function isProvisionedMacro(s: string): s is MacroRegion {
  return isMacroRegion(s) && PROVISIONED_MACROS.has(s);
}
