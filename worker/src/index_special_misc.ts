/** Session, token, runner, rotate, and not-found special routes. */
import type { Env } from "./index_common.js";
import { applyCors, applyTimingPad } from "./index_auth.js";
import type { RouteMatch } from "./route_match.js";
import { reapiError, getServerNonce } from "./index_common.js";
import { handleSessionExchange, handleTokenExchange } from "./lib/session_exchange.js";
import { handleRunnerMint, handleRunnerRevoke } from "./lib/runner_mint.js";
import { handleRunnerAuthorize } from "./lib/runner_authorization.js";
import { handleRunnerAdopt } from "./lib/runner_credential_routes.js";
import { handleRunnerCloseGeneration } from "./lib/runner_credential_generation_routes.js";
import { handleAuthRotate } from "./lib/auth_rotate.js";
import { handleTenantLookup } from "./lib/tenant_lookup.js";

export async function handleSpecialMiscRoute(
  request: Request,
  env: Env,
  requestId: string,
  requestStart: number,
  requestCounter: number,
  route: RouteMatch,
): Promise<Response | null> {
    // Session→token exchange — POST /v1/session/exchange (hugit-P2 WP-C, seam C).
    // The caller presents a Clerk SESSION JWT; the edge verifies it (shared
    // pipeline) and exchanges it for a short-lived tenant-scoped CoreLink PAT,
    // REUSING the container's audited /_internal/pat/mint via the _system DO.
    // The handler is fully fail-CLOSED (missing secret → 403, bad/expired
    // session → 401, no tenant → 403, upstream fault → 500) and never forwards
    // the session token past the edge. CORS is applied here, mirroring the
    // onboarding/customer arms.
    if (route.routeKind === "session_exchange") {
      const sessResp = await handleSessionExchange(request, env, requestId);
      return applyCors(sessResp, request);
    }

    // githugr authz #3 — tenant lookup (POST /internal/v1/auth/tenant/lookup).
    // Internal-auth gated; parameterized D1 read; fail-CLOSED 404 when no tenant
    // maps to the subject. CORS applied here, mirroring the arms above.
    if (route.routeKind === "tenant_lookup") {
      const lookupResp = await handleTenantLookup(request, env, requestId);
      return applyCors(lookupResp, request);
    }

    // githugr authz #1 — token exchange (POST /internal/v1/auth/token-exchange).
    // Internal-auth + Clerk-session gated; 403 on session.tenant ≠ audience (the
    // cross-tenant-write rejection); mints a ~300s tenant-scoped PAT via the
    // container. Fully fail-CLOSED; never forwards the session token past the edge.
    if (route.routeKind === "token_exchange") {
      const xchgResp = await handleTokenExchange(request, env, requestId);
      return applyCors(xchgResp, request);
    }

    // corelink-runners D-9 — runner PAT mint (POST /internal/v1/runner/mint).
    // Internal-auth gated (pat_mint consumer key + shared fallback) + runners-
    // entitlement checked; mints a job-bounded tenant-scoped PAT via the
    // container's single mint authority. Handled AT the Worker: the handler
    // builds a FRESH server-trusted request to the _system DO (it never forwards
    // the inbound request), so client trust headers can never reach the mint
    // route — the same posture as token-exchange. CORS applied here.
    if (route.routeKind === "runner_mint") {
      const mintResp = await handleRunnerMint(request, env, requestId);
      return applyCors(mintResp, request);
    }

    if (route.routeKind === "runner_authorize") {
      const authorizeResp = await handleRunnerAuthorize(request, env, requestId);
      return applyCors(authorizeResp, request);
    }

    if (route.routeKind === "runner_adopt") {
      const adoptResp = await handleRunnerAdopt(request, env, requestId);
      return applyCors(adoptResp, request);
    }

    if (route.routeKind === "runner_close_generation") {
      const closeResp = await handleRunnerCloseGeneration(request, env, requestId);
      return applyCors(closeResp, request);
    }

    // corelink-runners D-9 — runner PAT revoke (POST /internal/v1/runner/revoke).
    // Internal-auth gated; revokes by pat_id via the existing
    // `UPDATE pat SET revoked_at_ms` surface (INV-PAT-REVOKE-PROPAGATION). The
    // handler touches CONFIG_DB directly — no inbound headers are forwarded. CORS
    // applied here.
    if (route.routeKind === "runner_revoke") {
      const revokeResp = await handleRunnerRevoke(request, env, requestId);
      return applyCors(revokeResp, request);
    }

    // clw `auth rotate` (POST /internal/v1/auth/rotate). Internal-auth gated
    // (pat_mint consumer key + shared fallback); reads the old PAT's tenant +
    // scope, mints an equivalent new PAT via the container's single mint authority
    // (FRESH server-trusted request to the _system DO — no inbound headers
    // forwarded), then revokes the old pat_id ONLY after the mint succeeds (no
    // zero-valid-PAT window). Fully fail-CLOSED. CORS applied here.
    if (route.routeKind === "auth_rotate") {
      const rotateResp = await handleAuthRotate(request, env, requestId);
      return applyCors(rotateResp, request);
    }

    // Not found — timing-padded to prevent cross-tenant enumeration
    if (route.routeKind === "not_found") {
      await applyTimingPad(
        requestStart,
        requestId,
        requestCounter,
        getServerNonce(),
      );
      const resp = reapiError("NOT_FOUND", "The requested resource does not exist.", 404, requestId);
      return applyCors(resp, request);
    }

  return null;
}
