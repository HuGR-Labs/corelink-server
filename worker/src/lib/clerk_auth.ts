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

/** Result of the shared Clerk-session → tenant resolution pipeline. */
export type ClerkAuthResult =
  | { ok: true; tenantId: string; clerkUserId: string }
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
    } else {
      // Shape-check fallback: block non-Clerk issuers conservatively.
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
  try {
    const row = await env.CONFIG_DB
      .prepare("SELECT tenant_id FROM tenant WHERE clerk_user_id = ?1 LIMIT 1")
      .bind(clerkUserId)
      .first<{ tenant_id: string }>();
    if (!row || !row.tenant_id) {
      return {
        ok: false,
        response: reapiError("FORBIDDEN", "no tenant for this session", 403, requestId),
      };
    }
    tenantId = row.tenant_id;
  } catch (err: unknown) {
    const message = err instanceof Error ? err.message : "unknown error";
    console.error(`[${requestId}] clerk tenant lookup failed: ${message.slice(0, 80)}`);
    return {
      ok: false,
      response: reapiError("INTERNAL_ERROR", "tenant resolution error", 500, requestId),
    };
  }

  return { ok: true, tenantId, clerkUserId };
}
