/**
 * Shared Clerk session verification + tenant resolution (dashboard revival WP-1).
 *
 * Extracted VERBATIM from the /v1/onboarding/* arm in index.ts so the
 * customer_v1 dual-auth dispatch and the onboarding arm share ONE pipeline
 * (single source of truth for the M1/M2 hardening):
 *
 *   1. Extract the Clerk session JWT from the Authorization header → 401.
 *   2. Fail-CLOSED when CLERK_SECRET_KEY is unbound → 403 (no verification
 *      possible — never an open gate).
 *   3. verifyToken (@clerk/backend) with secretKey + authorizedParties from
 *      the shared azp allowlist below.
 *   4. M1: post-verify azp presence + allowlist re-assert — the Clerk library
 *      SKIPS the azp check when the claim is absent, so a token from a
 *      different app on the same Clerk instance would otherwise pass.
 *   5. M2: issuer exact-pin when CLERK_ISSUER_URL is set; conservative
 *      shape-check fallback (https + "clerk") otherwise.
 *   6. claims.sub required → 401.
 *   7. Tenant via SELECT tenant_id FROM tenant WHERE clerk_user_id = ?1
 *      (written by the signup-worker at provision, migration 0056) — no
 *      row → 403; D1 error → 500 (fail-closed, never fail-open).
 *
 * On failure the helper returns the EXACT error Response the onboarding arm
 * built before the extraction (same envelope shape, message, and status);
 * the caller applies CORS. The helper never logs token material
 * (INV-NO-PII-IN-LOGS posture preserved).
 */

import { verifyToken } from "@clerk/backend";
import type { Env } from "../index.js";
import { provisionOrLookupGithugrTenant } from "./githugr_provision.js";

/**
 * Shared azp allowlist for Clerk session verification (M1).
 *
 * Both admin-ui hosts are bound to the OpenNext Worker (#219): sessions are
 * minted on corelink-app (the sign-up/user-facing host) AND corelink-admin.
 * A JWT minted on corelink-app carries azp=https://corelink-app.humangr.com —
 * listing only corelink-admin 401-blocked the whole onboarding/checkout funnel.
 *
 * This is THE single allowlist const — formerly the onboarding-arm-local
 * ONBOARDING_AZP_ALLOWLIST — now shared by the onboarding arm and the
 * customer_v1 Clerk bridge. Do NOT duplicate it.
 */
export const CLERK_AZP_ALLOWLIST = [
  "https://corelink-admin.humangr.com",
  "https://corelink-app.humangr.com",
] as const;

/**
 * Authorized parties for the **githugr** Clerk instance (`clerk.githugr.com`).
 *
 * SEPARATE from {@link CLERK_AZP_ALLOWLIST} on purpose: githugr is a DIFFERENT
 * Clerk instance, accepted ONLY on the exchange/token-exchange paths (opt-in via
 * `allowGithugrIssuer`), and ONLY when its issuer + jwtKey + fixed tenant are all
 * configured. Keeping it separate means widening githugr's azp can never widen
 * CoreLink's own dashboard/onboarding azp set. (rt — single-githugr-tenant / Option B.)
 */
export const GITHUGR_AZP_ALLOWLIST = ["https://www.githugr.com"] as const;

/** Result of the shared Clerk-session → tenant resolution pipeline. */
export type ClerkAuthResult =
  | {
      ok: true;
      tenantId: string;
      clerkUserId: string;
      role: string;
      /**
       * Track-B step-up freshness: `fva[0]` (minutes since the first factor was
       * last verified) from the VERIFIED Clerk session JWT — `0` right after a
       * reauth. `undefined` when the token carries no well-formed `fva` claim;
       * consumers MUST treat `undefined` as NOT fresh (fail-closed), never `0`.
       */
      fvaMinutes?: number;
    }
  | { ok: false; response: Response };

/**
 * REAPI error envelope builder — local mirror of index.ts `reapiError`
 * (kept private there; replicated here to avoid a runtime import cycle
 * index.ts ⇄ lib/clerk_auth.ts). Shape is load-bearing:
 * `{ error, message, request_id }` + X-Request-Id header.
 */
function reapiError(error: string, message: string, status: number, requestId: string): Response {
  const body = { error, message, request_id: requestId };
  return new Response(JSON.stringify(body), {
    status,
    headers: {
      "Content-Type": "application/json",
      "X-Request-Id": requestId,
    },
  });
}

/**
 * Verify the Clerk session JWT on `request` and resolve the CoreLink tenant.
 *
 * Behavior is the onboarding arm's pipeline, verbatim (see module doc).
 * The ONLY ordering difference vs the old inline arm: the CLERK_SECRET_KEY
 * fail-closed guard runs AFTER bearer extraction so a request with NO token
 * is always a 401 (unauthenticated), not a 403 — the onboarding arm keeps
 * its own arm-level secrets guard BEFORE calling this helper, so onboarding
 * behavior is unchanged (its combined 403 still fires first).
 */
export async function verifyClerkSessionAndResolveTenant(
  request: Request,
  env: Env,
  requestId: string,
  opts?: { allowGithugrIssuer?: boolean },
): Promise<ClerkAuthResult> {
  // Extract the Clerk session token from the Authorization header.
  const authz = request.headers.get("authorization") ?? "";
  const bearerMatch = /^Bearer\s+(.+)$/i.exec(authz);
  const sessionToken = bearerMatch?.[1];
  if (!sessionToken) {
    return {
      ok: false,
      response: reapiError("UNAUTHORIZED", "clerk session required", 401, requestId),
    };
  }

  // ── Multi-issuer (opt-in, exchange paths only) ──────────────────────────────
  // The exchange/token-exchange seams accept sessions from the SEPARATE githugr
  // Clerk instance (`clerk.githugr.com`) IN ADDITION to CoreLink's. We route by
  // the UNVERIFIED issuer (peeked for routing only — the real signature verify
  // happens inside `verifyGithugrSession`), and ONLY when the caller opted in AND
  // both real githugr settings (issuer + public jwtKey) are configured. This
  // NEVER touches the shared `CLERK_ISSUER_URL` path below (dashboard/onboarding
  // stay CoreLink-only), so it cannot break CoreLink's own login.
  //
  // PILOT ISOLATION FIX: a githugr session no longer resolves to a single fixed
  // tenant. `verifyGithugrSession` now PROVISIONS-OR-LOOKS-UP a per-user isolated
  // tenant keyed on the verified Clerk `sub` (githugr runs its own Clerk, so the
  // CoreLink signup-worker auto-provision never fires for these users). The
  // routing gate therefore no longer requires `GITHUGR_TENANT_ID` — that legacy
  // shared-tenant secret is dead (see `verifyGithugrSession`).
  if (
    opts?.allowGithugrIssuer &&
    env.GITHUGR_CLERK_ISSUER_URL &&
    env.GITHUGR_CLERK_ISSUER_URL.length > 0 &&
    env.GITHUGR_CLERK_JWT_KEY &&
    env.GITHUGR_CLERK_JWT_KEY.length > 0 &&
    peekUnverifiedIssuer(sessionToken) === env.GITHUGR_CLERK_ISSUER_URL
  ) {
    return verifyGithugrSession(sessionToken, env, requestId);
  }

  // Fail-CLOSED when the Clerk verification secret is unbound — no JWKS-backed
  // verification is possible without it, so deny (never an open gate).
  const clerkSecretKey = env.CLERK_SECRET_KEY;
  if (!clerkSecretKey || clerkSecretKey.length === 0) {
    return {
      ok: false,
      response: reapiError("FORBIDDEN", "clerk verification unavailable", 403, requestId),
    };
  }

  // Verify the Clerk JWT at the edge. `verifyToken` throws on an invalid /
  // expired / wrong-azp token; `authorizedParties` pins it to our app origin.
  //
  // SECURITY FIX (M1): Clerk's `authorizedParties` check is skipped by the
  // library when the token omits `azp` entirely.  We therefore re-assert `azp`
  // presence and allowlist membership after a successful verify, and also
  // assert `iss` presence + expected shape so issuer trust is explicit rather
  // than implicit from the JWKS endpoint.
  //
  // SECURITY FIX (M2 — issuer pin): When CLERK_ISSUER_URL is set, we re-assert
  // exact equality post-verify.  Without the secret the shape-check fallback
  // (https + "clerk") is preserved; set the secret to activate the exact pin.
  const clerkIssuerUrl = env.CLERK_ISSUER_URL && env.CLERK_ISSUER_URL.length > 0
    ? env.CLERK_ISSUER_URL
    : undefined;
  let clerkUserId: string;
  // Track-B: the Clerk step-up freshness signal, captured from the verified JWT
  // below. Stays `undefined` (fail-closed = not fresh) unless a well-formed
  // `fva[0]` is present.
  let fvaMinutes: number | undefined;
  try {
    // Note: `verifyToken` has no `issuer` option (Clerk verifies the issuer
    // implicitly via the instance-scoped JWKS); the exact issuer pin is the
    // post-verify `iss === CLERK_ISSUER_URL` assertion below.
    const claims = await verifyToken(sessionToken, {
      secretKey: clerkSecretKey,
      authorizedParties: [...CLERK_AZP_ALLOWLIST],
    });

    // M1-FIX-1: Explicitly assert azp is present AND in the allowlist.
    // The Clerk library skips the azp check when the claim is absent, so a
    // token from a different app on the same Clerk instance (no azp) would
    // otherwise pass verifyToken() undetected.
    const azp = (claims as Record<string, unknown>)["azp"];
    if (
      typeof azp !== "string" ||
      azp.length === 0 ||
      !(CLERK_AZP_ALLOWLIST as readonly string[]).includes(azp)
    ) {
      return {
        ok: false,
        response: reapiError("UNAUTHORIZED", "clerk session azp invalid", 401, requestId),
      };
    }

    // M2-FIX: Assert issuer.  When CLERK_ISSUER_URL is set (exact-pin mode),
    // require strict equality against the configured value.  Without it, fall
    // back to the M1 shape-check (https scheme + hostname contains "clerk") so
    // this is a safe no-op until the owner runs:
    //   wrangler secret put CLERK_ISSUER_URL --env prod
    const iss = (claims as Record<string, unknown>)["iss"];
    if (clerkIssuerUrl) {
      // Exact-pin mode: CLERK_ISSUER_URL is set — require exact equality.
      if (iss !== clerkIssuerUrl) {
        return {
          ok: false,
          response: reapiError("UNAUTHORIZED", "clerk session issuer invalid", 401, requestId),
        };
      }
    } else if (env.ENVIRONMENT === "production") {
      // CAA-360 #30: in PRODUCTION the issuer MUST be exact-pinned. With
      // CLERK_ISSUER_URL unset we fail CLOSED rather than fall back to the weak
      // shape-check — otherwise a misconfigured prod deploy would accept tokens
      // from ANY https issuer whose host merely contains "clerk" (e.g. another
      // Clerk instance). Fail-closed forces the operator to provision the pin.
      // OPERATOR (launch step): `wrangler secret put CLERK_ISSUER_URL --env prod`
      // BEFORE deploying this Worker, or all Clerk session auth will 401.
      console.error(
        `[${requestId}] CLERK_ISSUER_URL unset in production — failing CLOSED (issuer pin required)`,
      );
      return {
        ok: false,
        response: reapiError("UNAUTHORIZED", "clerk session issuer invalid", 401, requestId),
      };
    } else {
      // Non-production shape-check fallback: block non-Clerk issuers
      // conservatively. (Prod requires the exact pin above.)
      if (
        typeof iss !== "string" ||
        iss.length === 0 ||
        !iss.startsWith("https://") ||
        !iss.includes("clerk")
      ) {
        return {
          ok: false,
          response: reapiError("UNAUTHORIZED", "clerk session issuer invalid", 401, requestId),
        };
      }
    }

    if (!claims.sub) {
      return {
        ok: false,
        response: reapiError("UNAUTHORIZED", "clerk session missing subject", 401, requestId),
      };
    }
    clerkUserId = claims.sub;
    // Track-B: capture the step-up freshness signal from the VERIFIED JWT. `fva`
    // is a Clerk built-in top-level array claim `[first-factor-age, second-factor-age]`
    // in minutes; `fva[0] = 0` means the user reauthed within the last minute.
    // Fail-CLOSED: only a well-formed, non-negative integer `fva[0]` is captured —
    // an absent/malformed claim leaves `fvaMinutes` undefined (⇒ NOT fresh), never 0.
    const fva = (claims as Record<string, unknown>)["fva"];
    if (
      Array.isArray(fva) &&
      typeof fva[0] === "number" &&
      Number.isInteger(fva[0]) &&
      fva[0] >= 0
    ) {
      fvaMinutes = fva[0];
    }
  } catch {
    // Never surface the verification error detail to the client.
    return {
      ok: false,
      response: reapiError("UNAUTHORIZED", "invalid clerk session", 401, requestId),
    };
  }

  // Resolve the CoreLink tenant from the verified Clerk user id. The
  // signup-worker writes tenant.clerk_user_id at provision (migration 0056).
  let tenantId: string;
  // RBAC: the resolved role gates the scope the caller receives (owner/admin/member
  // → read-write; viewer → read-only). Fail-safe default is the LEAST privilege.
  let role = "viewer";
  try {
    const row = await env.CONFIG_DB
      .prepare("SELECT tenant_id FROM tenant WHERE clerk_user_id = ?1 LIMIT 1")
      .bind(clerkUserId)
      .first<{ tenant_id: string }>();
    if (row && row.tenant_id) {
      // OWNER path (UNCHANGED): this Clerk user provisioned the tenant → owner role.
      tenantId = row.tenant_id;
      role = "owner";
    } else {
      // ADDITIVE team-member fallback (C-RESOLVE, WP-T4). Only when the OWNER
      // lookup above returns no row do we fall back to the team_member table
      // (migration 0074): a second-seat user resolves to the OWNING tenant.
      // The `status = 'active'` filter is MANDATORY — an 'invited' or 'removed'
      // member MUST NOT resolve (a removed seat is denied 403). Index on
      // (user_id, status) backs this lookup. Still 403 if neither matches.
      const memberRow = await env.CONFIG_DB
        .prepare("SELECT tenant_id, role FROM team_member WHERE user_id = ?1 AND status = 'active' LIMIT 1")
        .bind(clerkUserId)
        .first<{ tenant_id: string; role: string }>();
      if (!memberRow || !memberRow.tenant_id) {
        return {
          ok: false,
          response: reapiError("FORBIDDEN", "no tenant for this session", 403, requestId),
        };
      }
      tenantId = memberRow.tenant_id;
      // RBAC (0074 roles: owner|admin|member|viewer). A missing/unexpected role
      // fails safe to the least-privilege 'viewer' (read-only) rather than granting.
      role = memberRow.role || "viewer";
    }
  } catch (err: unknown) {
    const message = err instanceof Error ? err.message : "unknown error";
    console.error(`[${requestId}] clerk tenant lookup failed: ${message.slice(0, 80)}`);
    return {
      ok: false,
      response: reapiError("INTERNAL_ERROR", "tenant resolution error", 500, requestId),
    };
  }

  return {
    ok: true,
    tenantId,
    clerkUserId,
    role,
    // Omit the key entirely when there is no fresh signal (exactOptionalPropertyTypes
    // + fail-closed: absence ⇒ the consumer treats the session as NOT fresh).
    ...(fvaMinutes !== undefined ? { fvaMinutes } : {}),
  };
}

/**
 * Peek the UNVERIFIED `iss` claim of a JWT — for ROUTING ONLY (deciding which
 * Clerk instance minted it). The signature is NOT checked here; the caller MUST
 * verify the token authoritatively before trusting any claim. Malformed input →
 * `undefined`, so the caller falls through to the default (CoreLink) issuer path,
 * which then rejects it.
 */
function peekUnverifiedIssuer(token: string): string | undefined {
  try {
    const parts = token.split(".");
    if (parts.length !== 3) return undefined;
    const payloadB64 = parts[1];
    if (!payloadB64) return undefined;
    // base64url → base64 (+ pad) → JSON. Clerk payloads are ASCII JSON.
    const b64 = payloadB64.replace(/-/g, "+").replace(/_/g, "/");
    const padded = b64.padEnd(Math.ceil(b64.length / 4) * 4, "=");
    const claims = JSON.parse(atob(padded)) as Record<string, unknown>;
    const iss = claims["iss"];
    return typeof iss === "string" ? iss : undefined;
  } catch {
    return undefined;
  }
}

/**
 * Verify a session minted by the **githugr** Clerk instance and resolve it to a
 * PER-USER ISOLATED CoreLink tenant.
 *
 * PILOT ISOLATION FIX (was: fixed `GITHUGR_TENANT_ID` for every githugr user).
 * githugr runs its OWN Clerk instance, so the CoreLink signup-worker's
 * `user.created` auto-provision never fires for these users — the old code
 * therefore mapped EVERY githugr session to one shared tenant, giving NO
 * isolation between githugr users. This now makes the exchange itself the
 * provisioning authority: it derives a DETERMINISTIC tenant_id from the verified
 * Clerk `sub` and PROVISIONS-OR-LOOKS-UP that isolated tenant (with its full
 * 5-family row-set) on `CONFIG_DB`, idempotently.
 *
 * Networkless verification via githugr's PUBLIC `jwtKey` — no githugr secret
 * crosses into CoreLink. The caller has already confirmed the two githugr
 * settings (issuer + jwtKey) are present and that the unverified issuer matches;
 * this re-verifies signature + azp + issuer authoritatively (fail-CLOSED on any
 * miss).
 *
 * FAIL-CLOSED (isolation-critical): ANY D1 error during provision/lookup returns
 * a 500 — NEVER a fall-back to a shared tenant (that fall-back is the exact
 * isolation break this fixes).
 */
async function verifyGithugrSession(
  sessionToken: string,
  env: Env,
  requestId: string,
): Promise<ClerkAuthResult> {
  let clerkUserId: string;
  try {
    const claims = await verifyToken(sessionToken, {
      jwtKey: env.GITHUGR_CLERK_JWT_KEY,
      authorizedParties: [...GITHUGR_AZP_ALLOWLIST],
    });

    // azp MUST be present AND in the githugr allowlist (the library skips the
    // authorizedParties check when azp is absent — re-assert it ourselves).
    const azp = (claims as Record<string, unknown>)["azp"];
    if (
      typeof azp !== "string" ||
      azp.length === 0 ||
      !(GITHUGR_AZP_ALLOWLIST as readonly string[]).includes(azp)
    ) {
      return {
        ok: false,
        response: reapiError("UNAUTHORIZED", "clerk session azp invalid", 401, requestId),
      };
    }

    // Exact issuer pin to the githugr instance (authoritative — the peek was
    // routing-only).
    const iss = (claims as Record<string, unknown>)["iss"];
    if (iss !== env.GITHUGR_CLERK_ISSUER_URL) {
      return {
        ok: false,
        response: reapiError("UNAUTHORIZED", "clerk session issuer invalid", 401, requestId),
      };
    }

    if (!claims.sub) {
      return {
        ok: false,
        response: reapiError("UNAUTHORIZED", "clerk session missing subject", 401, requestId),
      };
    }
    clerkUserId = claims.sub;
  } catch {
    // A verification failure (bad signature / expired / wrong azp / wrong issuer)
    // is a 401 — never surface the detail.
    return {
      ok: false,
      response: reapiError("UNAUTHORIZED", "invalid clerk session", 401, requestId),
    };
  }

  // PROVISION-OR-LOOKUP the per-user isolated tenant, keyed on the verified `sub`.
  // Deterministic tenant_id ⇒ idempotent re-login (same sub → same tenant); the
  // read-back of `tenant_org_map` is authoritative (a prior login's row wins).
  //
  // FAIL-CLOSED: a D1 fault here is a 500 — we do NOT fall back to any shared
  // tenant (that would re-open the exact multi-user isolation break this fixes).
  try {
    const tenantId = await provisionOrLookupGithugrTenant(env.CONFIG_DB, clerkUserId);
    // githugr is a federated-login owner of its own per-user isolated tenant (not a
    // team_member seat), so it maps to the 'owner' role → read-write (unchanged).
    return { ok: true, tenantId, clerkUserId, role: "owner" };
  } catch (err: unknown) {
    const message = err instanceof Error ? err.message : "unknown error";
    console.error(`[${requestId}] githugr tenant provision/lookup failed: ${message.slice(0, 80)}`);
    return {
      ok: false,
      response: reapiError("INTERNAL_ERROR", "tenant resolution error", 500, requestId),
    };
  }
}
