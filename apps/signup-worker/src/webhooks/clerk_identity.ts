/** Signup identity, region, and analytics domain helpers. */
import type { ClerkUserCreatedEvent } from "./clerk.js";

export type MacroRegion = "wnam" | "enam" | "weur" | "sam" | "apac" | "afr";

/**
 * Macro regions backed by a jurisdiction-correct R2 bucket; signup MUST reject
 * the rest with a terminal 422 (never silently mis-land a tenant's data — backlog
 * #29).
 *
 * Provisionable = `{wnam, enam, weur, apac}` — exactly the macros whose serving
 * infra stores data in the CORRECT location/jurisdiction. `wnam`/`enam`→`iad`
 * (US R2); `weur`→`lhr` (EU R2 bucket `corelink-cas-eu` via the eu R2 endpoint —
 * LGPD/GDPR compliant); `apac`→`nrt` (WP4 — APAC-LOCATED bucket `corelink-cas-apac`,
 * Tokyo; a physical location hint, no APAC residency jurisdiction exists in R2).
 *
 * `sam` is DELIBERATELY EXCLUDED even though it is a valid, routable macro:
 * `PROD_SAM` still points at the DEFAULT US R2 endpoint + the shared US bucket
 * (`corelink-cas-prod`), so a `sam`-labelled tenant's data would land in US
 * storage under a FALSE residency label — an LGPD cross-border violation.
 * Cloudflare has NO SAM region (documented platform limit), so `sam` stays
 * non-provisionable EVERYWHERE while remaining routable. `afr` remains
 * unprovisioned (no colo build-out).
 *
 * SINGLE SOURCE OF TRUTH: this set MUST equal `worker/src/region-map.ts`
 * `PROVISIONED_MACROS` and the Rust `region_map.rs` `PROVISIONED_MACROS`. The
 * Three-consumer drift is gated by `worker/tests/region-map.test.ts` (which
 * parses all three copies) and by `tests/clerk.test.ts` here. The provisioned
 * cardinality is four macros: `{wnam, enam, weur, apac}`.
 */
export const PROVISIONED_MACROS: ReadonlySet<MacroRegion> = new Set<MacroRegion>([
  "wnam",
  "enam",
  "weur",
  "apac",
]);

/**
 * Thrown when a tenant's geo-derived macro region is a VALID canonical region
 * but is NOT provisioned (sam is a CF platform limit; afr has no colo — apac
 * was provisioned by WP4). Caught by the webhook
 * handler and mapped to a TERMINAL 422 (no Svix retry) — signup MUST reject the
 * tenant rather than silently downgrade them to a US region (backlog #29).
 */
export class UnprovisionedRegionError extends Error {
  readonly region: string;
  constructor(region: string) {
    super(`data-residency region '${region}' is not provisioned`);
    this.name = "UnprovisionedRegionError";
    this.region = region;
  }
}

/**
 * Map a CF colo code (the closest PoP to the end-user, e.g. "FRA", "GRU",
 * "NRT") to a canonical MACRO residency region. The mapping is geo-coarse:
 * European colos → weur, South-American → sam, Asia-Pacific → apac, everything
 * else (incl. unknown/absent) → enam (the genuine US-east default).
 *
 * NOTE: this returns the macro region the tenant SHOULD be assigned. Whether
 * that region is actually servable is a SEPARATE check (`PROVISIONED_MACROS`) —
 * an unprovisioned macro is REJECTED at signup, never silently downgraded.
 */
export function regionFromColo(colo: string | undefined | null): MacroRegion {
  if (!colo || typeof colo !== "string") return "enam";
  const c = colo.trim().toUpperCase();
  if (c.length === 0) return "enam";
  // Western-European colos → weur (the EU residency region; closing the leak).
  const WEUR = new Set([
    "LHR", "LCY", "MAN", "EDI", // UK + Ireland-adjacent
    "DUB",
    "FRA", "MUC", "DUS", "HAM", "STR", "TXL", "BER", // Germany
    "CDG", "MRS", "LYS", // France
    "AMS", "BRU", "ARN", "CPH", "HEL", "OSL", "VIE", "ZRH", "GVA",
    "MAD", "BCD", "BCN", "LIS", "MXP", "FCO", "PMO", "WAW", "PRG", "BUD",
  ]);
  // South-American colos → sam.
  const SAM = new Set([
    "GRU", "GIG", "BSB", "POA", "FOR", "REC", "CWB", "CNF", // Brazil
    "EZE", "SCL", "BOG", "LIM", "UIO", "MDE", "MVD", "ASU",
  ]);
  // Asia-Pacific + Oceania colos → apac (provisioned by WP4 → Tokyo/nrt,
  // physical bucket corelink-cas-apac; Oceania is served from Tokyo until an OC
  // region exists).
  const APAC = new Set([
    "NRT", "KIX", "ITM", "HND", // Japan
    "ICN", "TPE", "HKG", "SIN", "KUL", "BKK", "CGK", "MNL",
    "BOM", "DEL", "MAA", "BLR", "HYD", "CCU",
    "SYD", "MEL", "PER", "BNE", "AKL", // Oceania
  ]);
  // AFR has no provisioned jurisdiction-correct bucket in Phase 1. Returning
  // `afr` is load-bearing: the caller's provisioning gate rejects it before
  // createTenant; mapping it to `enam` would mis-land African data in US R2.
  const AFR = new Set([
    "JNB", "CPT", "DUR", "LOS", "ACC", "ABV", "NBO", "DAR", "KGL",
    "MRU", "TNR", "ALG", "TUN", "CAI", "CMN", "LAD", "WDH", "HRE",
  ]);
  if (WEUR.has(c)) return "weur";
  if (SAM.has(c)) return "sam";
  if (APAC.has(c)) return "apac";
  if (AFR.has(c)) return "afr";
  // North-American + Middle-Eastern + anything unknown → enam
  // (the genuine US-east default for unspecified/unmapped geos).
  return "enam";
}

/** True iff the macro region is provisioned in Phase 1 (signup-acceptable). */
export function isProvisionedMacro(region: string): region is MacroRegion {
  return (PROVISIONED_MACROS as ReadonlySet<string>).has(region);
}

/**
 * Derive a tenant slug from a Clerk user payload.
 *
 *   1. Prefer GitHub external-account username (`@github` provider).
 *   2. Else use the local-part of the primary email.
 *   3. Append `-default` and lower-case the whole thing.
 *   4. Strip anything not [a-z0-9-] to satisfy the tenant validator.
 */
export function tenantSlugFor(user: ClerkUserCreatedEvent["data"]): string {
  const github = user.external_accounts?.find(
    (e) => e.provider === "oauth_github" || e.provider === "github",
  );
  const primary = user.primary_email_address_id
    ? user.email_addresses.find((e) => e.id === user.primary_email_address_id)
    : user.email_addresses[0];
  const emailLocal = primary?.email_address.split("@")[0];
  const seed = github?.username ?? user.username ?? emailLocal ?? user.id;
  const cleaned = seed
    .toLowerCase()
    .replace(/[^a-z0-9-]+/g, "-")
    .replace(/-+/g, "-")
    .replace(/^-|-$/g, "");
  // Tenant name validator (admin-ui `validators.ts`) requires 3–64 chars.
  const safe = cleaned.length >= 3 ? cleaned : `user-${user.id.slice(0, 12)}`;
  return `${safe}-default`.slice(0, 64);
}

/**
 * The `tenant_org_map` key for this provisioning event — the Clerk principal
 * identifier githugr scopes a token to (A1 auto-provision, frozen decision).
 *
 * Prefer the Clerk **org id** (`organization_id`, else `org_id`) when the event
 * carries one; otherwise fall back to the user `id` (`sub`). Individual pilot
 * users have no org, so the sub fallback guarantees EVERY principal maps to its
 * isolated tenant — which is exactly what `resolve-tenant` looks up (a missing
 * row is the `org_not_mapped` lockout A1 closes).
 *
 * NOTE (githugr contract): "org_id else sub" is a githugr token-scoping
 * contract; confirm it matches what githugr's Option-B token exchange scopes to.
 */
export function orgMapKeyFor(user: ClerkUserCreatedEvent["data"]): string {
  const org = user.organization_id ?? user.org_id;
  if (typeof org === "string" && org.length > 0) return org;
  return user.id;
}

/**
 * The new user's primary email address (the one a team invitation was sent to),
 * or null when the payload carries no usable address. Mirrors `tenantSlugFor`'s
 * primary-address selection: prefer `primary_email_address_id`, else the first.
 */
export function primaryEmailOf(
  user: ClerkUserCreatedEvent["data"],
): string | null {
  const primary = user.primary_email_address_id
    ? user.email_addresses.find((e) => e.id === user.primary_email_address_id)
    : user.email_addresses[0];
  return primary?.email_address ?? null;
}

/**
 * Canonical `team_member.email_hash` pseudonym for `email`, normalized (trim +
 * lower-case) — the CROSS-LANG twin of the container's `email_hash::hash_email`
 * (Rust). CTRL-PRIV-001: the raw email is never stored.
 *
 * - `salt` set + non-empty → `hex(HMAC-SHA256(key=salt, msg=normalized))`
 * - `salt` unset / empty   → legacy `hex(SHA-256(normalized))` (byte-identical
 *   to the pre-salt scheme → zero regression until the salt is registered)
 *
 * MUST match the normalization + scheme the container's `invite()` write uses,
 * or an accepted user will never match their seat. `salt` is the same server
 * secret (`EMAIL_HASH_SALT`) the container reads — pass `env.EMAIL_HASH_SALT`.
 */
export async function emailHashFor(
  email: string,
  salt?: string,
): Promise<string> {
  const normalized = new TextEncoder().encode(email.trim().toLowerCase());
  if (salt && salt.length > 0) {
    const key = await crypto.subtle.importKey(
      "raw",
      new TextEncoder().encode(salt),
      { name: "HMAC", hash: "SHA-256" },
      false,
      ["sign"],
    );
    return bytesToHex(await crypto.subtle.sign("HMAC", key, normalized));
  }
  return bytesToHex(await crypto.subtle.digest("SHA-256", normalized));
}

/**
 * The **legacy, always-unsalted** `email_hash` for `email` — `hex(SHA-256(trim +
 * lowercase))`, byte-identical to the pre-salt scheme regardless of the salt.
 * Cross-lang twin of the container's `email_hash::hash_email_legacy`.
 *
 * NOT a write helper — it exists so a LOOKUP can also match rows written BEFORE
 * `EMAIL_HASH_SALT` was registered (the 5 pending team-invites + 123 legacy
 * tenant rows). See {@link emailHashCandidates}.
 */
export async function emailHashLegacy(email: string): Promise<string> {
  return emailHashFor(email, undefined);
}

/**
 * The set of `email_hash` values a LOOKUP for `email` must match against — the
 * salted candidate (current WRITE scheme) plus the legacy unsalted candidate,
 * **deduplicated**. Cross-lang twin of `email_hash::email_hash_candidates`.
 *
 * - salt UNSET → both candidates are identical → returns a SINGLE value → the
 *   lookup is behaviorally identical to today (zero regression).
 * - salt SET   → returns `[salted, legacy]` → the lookup finds BOTH a row
 *   written under the new salted scheme AND a legacy pre-salt row (no
 *   false-negative on the 5-invite / 123-legacy rows).
 *
 * WRITES never call this — they stay on {@link emailHashFor} (salted-if-set).
 */
export async function emailHashCandidates(
  email: string,
  salt?: string,
): Promise<string[]> {
  const salted = await emailHashFor(email, salt);
  const legacy = await emailHashLegacy(email);
  return salted === legacy ? [salted] : [salted, legacy];
}

export interface ApiClient {
  /**
   * Insert the tenant row with its data-residency MACRO region (backlog #29).
   * `region` is the geo-derived, provisioned macro (wnam/enam/weur/apac) — the
   * caller has already rejected unprovisioned macros. The region is persisted as
   * `tenant.primary_region` (NO LONGER hardcoded to 'enam').
   */
  createTenant(
    name: string,
    ownerUserId: string,
    region: MacroRegion,
  ): Promise<{ id: string }>;
  configureTenant(
    tenantId: string,
    region: string,
    plan: "free",
  ): Promise<void>;
  issuePat(
    tenantId: string,
    scope: "read-write",
  ): Promise<{ id: string; plaintext: string }>;
  publishUserMetadata(
    userId: string,
    publicMetadata: Record<string, unknown>,
    privateMetadata: Record<string, unknown>,
  ): Promise<void>;
}

export interface AnalyticsEmitter {
  emit(
    eventName: string,
    tenantId: string | null,
    userId: string | null,
    properties: Record<string, unknown>,
  ): Promise<void>;
}

function bytesToHex(buf: ArrayBuffer): string {
  return Array.from(new Uint8Array(buf))
    .map((b) => b.toString(16).padStart(2, "0"))
    .join("");
}

/**
 * Default analytics emitter backed by the D1 binding owned by the
 * Phase-0 analytics-worker. No-op when the binding is absent so the
 * provisioning path stays alive in environments where agent G's worker
 * hasn't shipped yet.
 */
export function d1AnalyticsEmitter(
  db: D1Database | undefined,
): AnalyticsEmitter {
  return {
    async emit(
      eventName: string,
      tenantId: string | null,
      userId: string | null,
      properties: Record<string, unknown>,
    ): Promise<void> {
      if (!db) return;
      const id = crypto.randomUUID();
      await db
        .prepare(
          "INSERT INTO analytics_events " +
            "(id, event_name, tenant_id, user_id, session_id, properties, created_at) " +
            "VALUES (?1, ?2, ?3, ?4, NULL, ?5, ?6)",
        )
        .bind(
          id,
          eventName,
          tenantId,
          userId,
          JSON.stringify(properties),
          new Date().toISOString(),
        )
        .run();
    },
  };
}

/**
 * Pure provisioning orchestration. Tests inject `api` + `analytics` so the
 * unit tests run without HTTP or D1.
 */
