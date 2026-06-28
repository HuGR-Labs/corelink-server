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
 *   weur → lhr   (EU — London; jurisdiction-correct EU R2 bucket — the leak this fix closes)
 *   sam  → sam   (South America — RECOGNISED/routable, but NOT provisionable: PROD_SAM
 *                 still points at the DEFAULT US R2 endpoint + shared US bucket, so a
 *                 `sam`-labelled tenant would mis-land in US storage — an LGPD cross-border
 *                 violation. Re-add to PROVISIONED_MACROS only once PROD_SAM has a real
 *                 SAM-jurisdiction bucket/endpoint.)
 *   apac → nrt   (Asia-Pacific — Tokyo; NOT provisioned in Phase 1)
 *   afr  → REJECT (no African colo provisioned; must be rejected at signup)
 *
 * Provisioned = { wnam, enam, weur } — exactly the macros backed by a
 * jurisdiction-correct R2 bucket today. sam/apac/afr are valid macro codes (the
 * D1 CHECK accepts them and routing still recognises them) but are NOT
 * provisioned, so signup MUST reject them rather than silently mis-land the
 * tenant's data in the wrong jurisdiction.
 *
 * This provisioned set is the SINGLE SOURCE OF TRUTH shared by THREE consumers
 * (this file, the Rust `region_map.rs`, and signup `clerk.ts`); all three are
 * pinned together by `worker/tests/region-map.test.ts` (the 3-way drift gate).
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
 * Macro regions provisioned today — exactly those with a jurisdiction-correct R2
 * bucket. Signup MUST reject any macro NOT in this set (sam/apac/afr today)
 * rather than route/store the tenant anywhere. `sam` is deliberately EXCLUDED:
 * it stays routable (`MACRO_TO_COLO`/`coloForMacro` still recognise it) but is
 * NOT provisionable until PROD_SAM has a real SAM-jurisdiction bucket — provisioning
 * it today would mis-land data in US R2 under a false residency label (LGPD).
 */
export const PROVISIONED_MACROS: ReadonlySet<MacroRegion> = new Set<MacroRegion>([
  "wnam",
  "enam",
  "weur",
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
