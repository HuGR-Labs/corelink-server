// worker/src/lib/devenv_guard.ts
//
// DevEnv per-tenant quota enforcement and guard (WP-08 / WP-07).

import type { Env } from "../index.js";

export interface DevenvQuotaResult {
  readonly allowed: boolean;
  readonly reason?: string;
}

/**
 * Check whether the given tenant is allowed to launch or use a DevEnv session.
 * Evaluates tenant state, entitlement limits, and monthly vCPU ceilings.
 *
 * ## Fail-CLOSED contract (B-075)
 *
 * DevEnv is the most expensive per-request compute surface in the product, so
 * EVERY way of not obtaining a positive entitlement is a denial — never a
 * fall-through to `allowed: true`:
 *
 *   - no `tenant_id` (or the anonymous sentinel)   → deny
 *   - `CONFIG_DB` unbound                          → deny
 *   - the D1 read throws (outage, timeout, …)      → deny
 *   - NO `runners_entitlement` row for the tenant  → deny
 *   - row present but `install_status = suspended` → deny
 *
 * The "no row" denial matches the entitlement table's OWN documented semantics
 * (`migrations/d1/0070_runners_entitlement.sql`: "Empty table = no cap =
 * reject"; the row's presence is what expresses a real, positive entitlement)
 * and the sibling introspect handler, which already omits the cap and lets the
 * runners fabric reject the placement. This guard used to be the one reader
 * that authorised by omission: a tenant with no entitlement row, or ANY tenant
 * during a D1 outage, was granted billable compute.
 *
 * The `CONFIG_DB`-unbound denial matches the call site in
 * `worker/src/index.ts`, which already 503s when `RUNNER_DEVENV_DO` is
 * unbound — a missing binding is a service fault, never an authorisation.
 */
export async function checkDevenvQuota(
  env: Env,
  tenantId: string,
): Promise<DevenvQuotaResult> {
  if (!tenantId || tenantId === "_anonymous") {
    return { allowed: false, reason: "tenant_id required" };
  }

  // CONFIG_DB is declared non-optional on `Env` and is bound in EVERY wrangler
  // environment (top-level + prod/staging/prod-sam/prod-lhr/prod-nrt/prod-syd).
  // An absent binding is therefore a misconfiguration, not a supported mode:
  // deny rather than authorise a tenant we cannot check.
  if (!env.CONFIG_DB) {
    return {
      allowed: false,
      reason: "DevEnv entitlement unavailable (CONFIG_DB unbound)",
    };
  }

  let row: {
    max_concurrency: number;
    max_vcpu_h: number;
    install_status: string;
  } | null;

  try {
    row = await env.CONFIG_DB.prepare(
      `SELECT max_concurrency, max_vcpu_h, install_status
         FROM runners_entitlement
         WHERE tenant_id = ?1`
    ).bind(tenantId).first<{
      max_concurrency: number;
      max_vcpu_h: number;
      install_status: string;
    }>();
  } catch {
    // Fail CLOSED: an unreadable entitlement is not an entitlement. Authorising
    // through a D1 outage would hand billable DevEnv compute to every tenant
    // for the duration of the outage.
    return {
      allowed: false,
      reason: "DevEnv entitlement check unavailable",
    };
  }

  if (!row) {
    return {
      allowed: false,
      reason: "No DevEnv runners entitlement for tenant",
    };
  }

  if (row.install_status === "suspended") {
    return { allowed: false, reason: "Tenant runners entitlement suspended" };
  }

  return { allowed: true };
}

export const enforceDevenvQuota = checkDevenvQuota;
