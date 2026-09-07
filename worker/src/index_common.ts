/** Shared worker types and routing helpers. */

import type { InternalConsumer } from "./lib/internal_auth.js";
import type { Env } from "./index_env.js";

export type { Env } from "./index_env.js";

/**
 * Cloudflare Durable Object location hints (the `locationHint` option of
 * `DurableObjectNamespace.get`). A DO with NO hint homes at the colo of first
 * access; the hint pins WHERE a brand-new DO — and, for `CoreLinkServer`, its
 * attached Rust container — is created.
 */
export type DoLocationHint =
  | "wnam"
  | "enam"
  | "sam"
  | "weur"
  | "eeur"
  | "apac"
  | "oc"
  | "afr"
  | "me";

/**
 * Map this Worker's own serving region (`R2_CAS_REGION`, a colo code) to the CF
 * DO location hint for the region it serves.
 *
 * WHY THIS EXISTS (multi-region container-serving bug, 2026-08-18): the region
 * fan-out is a CO-LOCATED Service Binding — the regional Worker (`PROD_LHR` /
 * `PROD_NRT` / …) executes in the CALLER's entry colo, not its named region.
 * Combined with a hint-LESS `CORELINK_SERVER.get()`, a regional tenant's DO +
 * its container homed at the entry colo instead of the region. When that colo is
 * not a CF Containers metro (or cannot start the container) the DO's container
 * never became reachable → `container_health_check_failed`. Hinting the DO to
 * this Worker's own region makes placement DETERMINISTIC and in-region.
 *
 * `sam` has NO Cloudflare region (documented platform limit — see
 * region-map.ts) so a sam-serving Worker pins to `enam` (US), matching where
 * sam-labelled data physically lands today. An unknown/unset region returns
 * `undefined` (no hint) → today's exact behaviour, never worse.
 */
export function doLocationHintForRegion(region: string | undefined): DoLocationHint | undefined {
  switch (region) {
    case "iad":
      return "enam";
    case "lhr":
      return "weur";
    case "nrt":
      return "apac";
    case "syd":
      return "oc";
    case "sam":
      // Cloudflare has no SAM region; sam data lands in US R2 today. Pin the DO
      // (and its container) to ENAM so it starts in a supported container metro
      // rather than homing non-deterministically at the caller's entry colo.
      return "enam";
    default:
      return undefined;
  }
}

/**
 * Build the options bag for `CORELINK_SERVER.get(id, opts)`: a region location
 * hint when this Worker's region is known, else `undefined` (bare `.get(id)`).
 * Every `CoreLinkServer` DO owns a container, so ALL `.get()` sites route
 * through this so the container homes in-region deterministically.
 */
export function serverGetOpts(env: Env): { locationHint: DoLocationHint } | undefined {
  const hint = doLocationHintForRegion(env.R2_CAS_REGION);
  return hint ? { locationHint: hint } : undefined;
}

/**
 * Extract the tenant UUID from an OCI bearer for routing only.
 *
 * OCI's bearer is minted by the container and is authenticated there; the
 * Worker deliberately does not treat this structural hint as authorization.
 * It is nevertheless safe to use for residency fan-out because a forged hint
 * can only select a regional service binding, while the receiving container
 * still verifies the complete HMAC before serving or storing any bytes. A
 * malformed/opaque bearer stays on the shared OCI leg and is rejected by the
 * container as usual.
 */
export function ociRoutingTenantId(request: Request): string | undefined {
  const authorization = request.headers.get("authorization");
  if (authorization === null || !authorization.startsWith("Bearer ")) return undefined;
  const token = authorization.slice("Bearer ".length);
  const fields = token.split(".");
  if (fields.length !== 6 || fields[0] !== "corelink-oci") return undefined;
  const tenant = fields[1];
  if (tenant === undefined) return undefined;
  return /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(tenant)
    ? tenant
    : undefined;
}

/**
 * Route prefix of the DO D1-placement probe: `/_internal/do-d1-probe/{tenant_id}`.
 *
 * A DIAGNOSTIC, not a product surface. It forwards to the tenant's own
 * `CoreLinkServer` DO `/_do/health`, whose body carries two timed `SELECT 1`
 * reads (D1 primary vs nearest replica) measured from INSIDE the DO. That is
 * the one unmeasured fact the "route the container's D1 reads through the DO"
 * proposal hinges on: a DO-issued primary read ≤25 ms means the DO is
 * co-located with the ENAM primary (the proposal wins ~60-75 ms); ≥100 ms means
 * it is far (the proposal is a regression).
 *
 * The tenant id is REQUIRED and caller-supplied because DO placement is
 * per-DO-id: `idFromName(tenant)` — the number is only comparable to a measured
 * container `opat` if it comes from the SAME DO that serves that tenant.
 */
export const INTERNAL_DO_D1_PROBE_PREFIX = "/_internal/do-d1-probe/";

/**
 * Map a `/_internal/*` path to its auth CONSUMER (red-team #3 key split).
 *
 * The container exposes three internal surfaces with distinct blast radii:
 *   - `/_internal/pat/mint`  → `pat_mint` (CORELINK_PAT_MINT_AUTH_KEY)
 *   - `/_internal/admin/*`   → `admin`    (CORELINK_ADMIN_AUTH_KEY)
 *   - `/_internal/dsr/*`     → `erase`    (CORELINK_ERASE_AUTH_KEY)
 *
 * Anything else under `/_internal/*` (e.g. `/_internal/cas/*`) defaults to the
 * most-privileged data-plane consumer, `erase` — its key (and, via fallback,
 * the shared key) gates the CAS delete surface used by the DSR/erasure path.
 * Every consumer falls back to the shared CORELINK_INTERNAL_AUTH_KEY when its
 * dedicated key is unset (see resolveConsumerKey), so this never widens access.
 *
 * `pathSuffix` is the server-derived route path (NOT client-suppliable beyond
 * the URL itself, which already selected the `internal` routeKind).
 */
export function internalConsumerForPath(pathSuffix: string): InternalConsumer {
  if (pathSuffix === "/_internal/pat/mint") {
    return "pat_mint";
  }
  if (pathSuffix.startsWith("/_internal/admin/")) {
    return "admin";
  }
  // The per-user DSR legitimacy ANCHOR (`/_internal/dsr/anchor`, #634) is a
  // SEPARATE authority from the eraser: githugr holds `CORELINK_DSR_ANCHOR_AUTH_KEY`
  // (distinct from the eraser's ERASE key — the anti-forge two-authority split that
  // gates the irreversible physical-erase cascade). It MUST be matched before the
  // `/_internal/dsr/*` erase catch-all below, or the anchor caller is gated on the
  // wrong (erase) key and always 401s (the go-live blocker: the worker front-gate
  // rejected githugr's anchor key before it ever reached the container's own anchor
  // gate, so binding/forwarding the anchor key alone could never help).
  if (pathSuffix === "/_internal/dsr/anchor") {
    return "dsr_anchor";
  }
  // Read-only tenant-quota lookup (`/_internal/tenant/{tenant_id}/quota`, #quota-read):
  // a DEDICATED read consumer (`CORELINK_QUOTA_READ_AUTH_KEY`, shared-key fallback)
  // so a leak of this low-privilege read secret cannot mint, erase, or admin. Matched
  // BEFORE the erase catch-all below.
  if (pathSuffix.startsWith("/_internal/tenant/") && pathSuffix.endsWith("/quota")) {
    return "quota_read";
  }
  // Edge-probe audit emit (`/_internal/audit/cas-attempted`): its own consumer,
  // NOT the `erase` catch-all below. The container gates it on the dedicated
  // `CORELINK_AUDIT_ATTEMPTED_AUTH_KEY` with no shared fallback, so falling into
  // the catch-all would have the edge demand the ERASE key while the container
  // demands the audit key — a mismatch that 401s at the edge and leaves the
  // endpoint unreachable no matter which secret the operator binds.
  if (pathSuffix === "/_internal/audit/cas-attempted") {
    return "audit_attempted";
  }
  // Read-only DO D1-placement probe (`/_internal/do-d1-probe/{tenant_id}`) — the
  // diagnostic instrument that answers "where does this tenant's DO sit relative
  // to the ENAM D1 primary?". It mutates nothing (two `SELECT 1`s), so it gates
  // on the SAME low-privilege READ consumer as the quota lookup rather than on
  // the erase catch-all: an operator reading a latency number must not need the
  // key that can delete a tenant's bytes. No new key is introduced.
  if (pathSuffix.startsWith(INTERNAL_DO_D1_PROBE_PREFIX)) {
    return "quota_read";
  }
  // DSR erase surface (`/_internal/dsr/*`) and any other internal data-plane
  // route (`/_internal/cas/*`, …) gate on the erase consumer key.
  return "erase";
}

// ──────────────────────────────────────────────────────────────────────────────
// Error envelope builders (REAPI error shapes)
// ──────────────────────────────────────────────────────────────────────────────

interface ReapiErrorEnvelope {
  readonly error: string;
  readonly message: string;
  readonly request_id: string;
}

export function reapiError(error: string, message: string, status: number, requestId: string): Response {
  const body: ReapiErrorEnvelope = { error, message, request_id: requestId };
  return new Response(JSON.stringify(body), {
    status,
    headers: {
      "Content-Type": "application/json",
      "X-Request-Id": requestId,
    },
  });
}

// ──────────────────────────────────────────────────────────────────────────────
// Worker state (module-level, reset per isolate cold start)
// ──────────────────────────────────────────────────────────────────────────────

/**
 * One-time server nonce for timing-pad seed mixing.
 * Initialized lazily from crypto.getRandomValues on first request.
 * Module-scoped: stable within a single isolate lifetime.
 */
let serverNonce: number | null = null;

export function getServerNonce(): number {
  if (serverNonce === null) {
    const buf = new Uint32Array(1);
    crypto.getRandomValues(buf);
    serverNonce = buf[0] ?? 0;
  }
  return serverNonce;
}

/** The sole declared cron in wrangler.toml and its internal drill kind. */
const SCHEDULED_DRILL_BY_CRON: Readonly<Record<string, "synthetic_page">> = {
  "0 14 * * 1": "synthetic_page",
};

export const SYNTHETIC_PAGE_CONTRACT = {
  service: "synthetic-drill",
  event_action: "trigger",
  severity: "info",
  synthetic_severity: "sev2_synthetic",
} as const;

/** Stable week number used by the four-week synthetic page rotation. */
export function scheduledWeekNumber(scheduledTime: number): number {
  // 1970-01-05 was a Monday; all declared crons fire on Mondays. Floor
  // instead of rounding so a malformed pre-epoch test timestamp cannot move
  // into the following week.
  return Math.floor((scheduledTime - Date.UTC(1970, 0, 5)) / (7 * 24 * 60 * 60 * 1_000));
}

export function syntheticRegionForWeek(week: number): "americas" | "emea" | "apac" | "boundary_handoff" {
  switch (((week % 4) + 4) % 4) {
    case 0:
      return "americas";
    case 1:
      return "emea";
    case 2:
      return "apac";
    default:
      return "boundary_handoff";
  }
}

/**
 * The Monday cron is not the boundary-handoff emit window. For rotation week 3
 * the receiver must defer the real page to the following Sunday at 23:59 UTC.
 */
export function syntheticEmitAtMs(scheduledTime: number, week: number): number {
  if (((week % 4) + 4) % 4 !== 3) return scheduledTime;
  const scheduled = new Date(scheduledTime);
  const sunday = Date.UTC(
    scheduled.getUTCFullYear(),
    scheduled.getUTCMonth(),
    scheduled.getUTCDate() + (7 - scheduled.getUTCDay()),
    23,
    59,
    0,
    0,
  );
  return sunday;
}

export function scheduledDrillForCron(cron: string): "synthetic_page" | undefined {
  return SCHEDULED_DRILL_BY_CRON[cron];
}
