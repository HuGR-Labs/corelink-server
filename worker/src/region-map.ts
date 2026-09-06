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
 *   apac → nrt   (Asia-Pacific — Tokyo; provisioned via the APAC R2 bucket)
 *   afr  → REJECT (no African colo provisioned; must be rejected at signup)
 *
 * Provisioned = { wnam, enam, weur, apac } — the four macros backed by the
 * currently provisioned storage paths. `apac` resolves to Tokyo (`nrt`) and
 * uses the APAC bucket; `sam` and `afr` remain valid macro codes (the D1 CHECK
 * accepts them and routing recognises `sam`) but are NOT provisioned, so signup
 * MUST reject them rather than silently mis-land the tenant's data.
 *
 * This provisioned set is the SINGLE SOURCE OF TRUTH shared by three consumers
 * (this file, the Rust `region_map.rs`, and signup `clerk.ts`); all three are
 * pinned together by `worker/tests/region-map.test.ts` (the three-consumer drift
 * gate). The provisioned cardinality is four macros.
 */

/** The canonical CoreLink data-residency MACRO region codes (D1 CHECK set). */
export type MacroRegion = "wnam" | "enam" | "weur" | "sam" | "apac" | "afr";

/** Cloudflare colo codes CoreLink routes residency traffic to. */
export type Colo = "iad" | "lhr" | "sam" | "nrt";

// Keep the macro vocabulary and the edge-colo vocabulary distinct at the
// comparison boundary. Both happen to be encoded as `sam` today, but callers
// must compare through `coloMatchesMacro`, not by treating a macro as a colo.
const COLO_SAM: Colo = "sam";

/**
 * FROZEN macro→colo map. `afr` is intentionally ABSENT (no provisioned colo →
 * reject at signup). `coloForMacro` returns `undefined` for `afr` and for any
 * unknown string so callers must handle the reject branch explicitly.
 */
const MACRO_TO_COLO: Readonly<Record<MacroRegion, Colo | undefined>> = {
  wnam: "iad",
  enam: "iad",
  weur: "lhr",
  sam: COLO_SAM,
  apac: "nrt",
  afr: undefined,
};

/**
 * Macro regions provisioned today — exactly those with a location/jurisdiction-
 * correct R2 bucket. Signup MUST reject any macro NOT in this set (sam/afr today)
 * rather than route/store the tenant anywhere. `apac` is now provisioned: WP4
 * (2026-08-17) created the APAC-LOCATED bucket `corelink-cas-apac` (Tokyo/nrt) and
 * pointed prod-nrt at it, so apac CAS bytes store + serve in-region (apac is a
 * physical LOCATION hint — R2 has no APAC data-residency jurisdiction, unlike EU —
 * which suffices for latency-locality). `sam` is deliberately EXCLUDED: it stays
 * routable (`MACRO_TO_COLO`/`coloForMacro` still recognise it) but is NOT
 * provisionable — Cloudflare has no SAM region (documented platform limit), so its
 * data would mis-land in US R2 under a false residency label.
 */
export const PROVISIONED_MACROS: ReadonlySet<MacroRegion> = new Set<MacroRegion>([
  "wnam",
  "enam",
  "weur",
  "apac",
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

/** Compare a trusted macro claim with the serving colo without stringly
 * equating the two different vocabularies at call sites. */
export function coloMatchesMacro(macro: string, servingColo: string): boolean {
  const expected = coloForMacro(macro);
  return expected !== undefined && expected === servingColo;
}

/** Is the macro a region whose serving colo is IAD (the local US path)? */
export function isLocalIadMacro(macro: string): boolean {
  return coloForMacro(macro) === "iad";
}

/** True iff the macro is provisioned in Phase 1 (signup-acceptable). */
export function isProvisionedMacro(s: string): s is MacroRegion {
  return isMacroRegion(s) && PROVISIONED_MACROS.has(s);
}
