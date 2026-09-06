/** Authentication and tenant policy stage. */
import type { ExecutionContext } from "@cloudflare/workers-types";
import type { Env } from "./index_common.js";
import { applyCors, extractAuth, type AuthResult } from "./index_auth.js";
import type { RouteMatch } from "./route_match.js";
import { emitFirstCliAuthed } from "./lib/onboarding_events.js";
import { reapiError } from "./index_common.js";

type AuthOk = Extract<AuthResult, { ok: true }>;
export interface AuthStageResult {
  readonly auth: AuthOk;
  readonly resolvedTenantId: string;
  readonly stAuthStart: number;
  readonly stAuthEnd: number;
  readonly stPatSource: "l1" | "kv" | "d1" | undefined;
}

export async function authenticateRequest(
  request: Request,
  env: Env,
  ctx: ExecutionContext,
  requestId: string,
  route: RouteMatch,
): Promise<AuthStageResult | Response> {
  const url = new URL(request.url);
  let stAuthStart = 0;
  let stAuthEnd = 0;
  let stPatSource: "l1" | "kv" | "d1" | undefined;
    let auth: AuthOk;
    if (route.routeKind === "signup") {
      // Signup is pre-tenant: the path :token IS the auth artifact, not a PAT,
      // so there is no D1-resolved scope — forward an empty scope (H1).
      // WP5a: signup is pre-tenant and never a runner-job PAT → runnerJobAcKey null.
      auth = { ok: true, tenantId: "_anonymous", tokenPrefix: "signup", scope: "", runnerJobAcKey: null };
    } else {
      // Scope guard (security): Basic auth is accepted ONLY on the `pip` adapter
      // route. pip/uv can emit nothing but URL-embedded Basic; every other
      // surface (native CAS/AC, npm `_authToken` Bearer, cargo/sccache Bearer,
      // browser) still rejects non-Bearer schemes with `invalid_scheme`.
      stAuthStart = Date.now();
      const result = await extractAuth(
        request,
        env,
        route.routeKind === "pip",
        ctx.waitUntil.bind(ctx),
      );
      stAuthEnd = Date.now();
      if (result.ok) stPatSource = result.patSource;
      if (!result.ok) {
        // OCI (oci_v2 / oci_token) never reaches here — it is handled by the
        // dedicated pass-through branch ABOVE (which forwards to the container
        // for its own two-leg auth), so this PAT-gate path only sees PAT routes.
        //
        // F18: signing_key_not_configured means PAT_SIGNING_KEY is absent or
        // too short — the operator MUST be alerted via 503 (not 401, which would
        // silently look like a bad client credential). The structured error log
        // is emitted inside extractAuth; here we map to 503 Service Unavailable.
        //
        // H1: d1_lookup_error is a TRANSIENT D1 infra fault (network partition /
        // DB unavailable) raised by the PAT D1 lookup — NOT a bad credential. It
        // MUST map to 503 (retryable) too, otherwise a D1 hiccup makes every
        // client see "bad credentials" → CI failures + spurious PAT rotation +
        // on-call chasing the wrong thing. Genuine bad/unknown PATs
        // (pat_not_found / pat_expired / invalid_*) still fall through to 401.
        if (
          result.reason === "signing_key_not_configured" ||
          result.reason === "d1_lookup_error"
        ) {
          return applyCors(
            reapiError("SERVICE_UNAVAILABLE", "authentication service unavailable", 503, requestId),
            request,
          );
        }
        // G4: the PAT is valid but its tenant is suspended/erased
        // (tenant_offboarding_state.state ∈ {suspended, erased}). This is an
        // authorization denial, NOT a bad credential — map to 403 fail-closed
        // (distinct from the 401 unknown/expired/malformed-PAT arms), so a
        // suspended tenant is fast-denied on the customer CAS/AC hot path
        // without waiting for every PAT to be individually revoked.
        if (result.reason === "tenant_suspended") {
          return applyCors(
            reapiError("FORBIDDEN", "tenant suspended", 403, requestId),
            request,
          );
        }
        return applyCors(
          reapiError("UNAUTHORIZED", "authentication required", 401, requestId),
          request,
        );
      }
      auth = result;
    }

    // ── Tenant routing (WP-T1) ────────────────────────────────────────────────
    // auth.tenantId is the PAT-resolved tenant produced by WP-A1's D1 lookup.
    // We use it exclusively for DO routing — never the raw URL path segment.
    //
    // WP-A1 contract: resolves real tenant → stores in auth.tenantId.
    // Until WP-A1 lands the value is "_pending" (WP-A1 stub in extractAuth).
    // Once WP-A1 is merged, auth.tenantId will carry the real tenant ID.
    const resolvedTenantId = auth.tenantId;

    // Path-spoof defence (P1-2): if the URL path carries a tenant namespace
    // AND the PAT has been resolved to a real tenant (not the "_pending" stub),
    // the URL tenant MUST match the PAT tenant. Mismatch → 403.
    //
    // The guard `resolvedTenantId !== "_pending"` is the WP-A1 activation gate:
    //   - "_pending" = WP-A1 not yet merged → skip spoof check (transitional)
    //   - Any other value = WP-A1 resolved → enforce tenant match
    const urlTenant = route.tenantId;
    const isRealTenant = resolvedTenantId !== "_pending";

    if (isRealTenant && urlTenant !== "_anonymous" && urlTenant !== resolvedTenantId) {
      // PAT tenant ≠ URL path tenant — potential path-spoof.
      // Return 403 without leaking which side mismatched. (OCI never reaches
      // this PAT-gate path — see the dedicated pass-through branch above.)
      return applyCors(
        reapiError("FORBIDDEN", "tenant mismatch", 403, requestId),
        request,
      );
    }

    // ── Onboarding funnel: `first_cli_authed` producer (PLG §7.1) ─────────────
    // The CLI's FIRST authenticated call is `GET /v1/users/me` (`corelink
    // whoami` — tools/cli/src/client.rs:138 — and the doctor auth probe —
    // tools/cli/src/doctor.rs:37). This is the earliest point where BOTH facts
    // the event asserts are established: the PAT verified (extractAuth returned
    // ok, above) and the tenant is a real, non-spoofed tenant (the path-spoof
    // guard immediately above has just cleared). It is deliberately BEFORE the
    // quota/residency/DO legs so a later 429/503 can never suppress a signal
    // that is about AUTH, not about the response body.
    //
    // Fire-and-forget: `emitFirstCliAuthed` returns void (impossible to await
    // inline), swallows all its own errors, and its promise is handed to
    // ctx.waitUntil so it survives the response (#859 — a bare floating promise
    // is cancelled on return, see lib/tenant_suspend_gate.ts:115).
    // Dedup is the deterministic id `first_cli_authed:<tenant_id>` against the
    // analytics_events PRIMARY KEY + ingest's INSERT OR IGNORE — see
    // lib/onboarding_events.ts.
    if (
      route.routeKind === "reapi_v1" &&
      request.method === "GET" &&
      url.pathname === "/v1/users/me"
    ) {
      emitFirstCliAuthed(env, resolvedTenantId, { waitUntil: ctx.waitUntil.bind(ctx) });
    }

    // ── Per-tier quota enforcement ────────────────────────────────────────────
    // Quota checks run AFTER auth and BEFORE forwarding to the DO.
    // Skip for system/anonymous tenants (no billing record exists for them).
    //
    // Storage quota: enforced from SUM(tenant_storage_state.bytes_used).
    // Request quota: enforced BY DEFAULT via the monthly_request_counts atomic
    // counter (fail-CLOSED) — disabled only by REQUEST_QUOTA_DISABLED="true"
    // (dev/test). See worker/src/lib/quota.ts.
    //
    // D1-error posture is verb-aware (CAA-360 #25): reads fail OPEN for
    // availability, byte-adding writes (PUT/POST) fail CLOSED so an outage
    // cannot be used to write past the cap. The DO's CAS quota enforcement
    // (quota_fsm_state) provides the deeper safety net on mutations.
    //
    // The resolved per-tier storage cap is also forwarded to the container as
    // the server-trusted STORAGE_QUOTA_HEADER so the container's byte-accounting
    // reservation seeds a FRESH tenant_storage_state row with the REAL cap
    // (not the legacy uncapped `0`). It is set ONLY for a real tenant with a
    // confirmed tier; `null` (system/anon/pending, or a D1-error tier) ⇒ the
    // header is omitted and the container fails closed on an unseeded tenant.

  return { auth, resolvedTenantId, stAuthStart, stAuthEnd, stPatSource };
}
