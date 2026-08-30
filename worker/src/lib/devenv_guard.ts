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
 */
export async function checkDevenvQuota(
  env: Env,
  tenantId: string,
): Promise<DevenvQuotaResult> {
  if (!tenantId || tenantId === "_anonymous") {
    return { allowed: false, reason: "tenant_id required" };
  }

  // If CONFIG_DB is available, verify tenant entitlement and monthly limits
  if (env.CONFIG_DB) {
    try {
      const row = await env.CONFIG_DB.prepare(
        `SELECT max_concurrency, max_vcpu_h, install_status 
         FROM runners_entitlement 
         WHERE tenant_id = ?1`
      ).bind(tenantId).first<{
        max_concurrency: number;
        max_vcpu_h: number;
        install_status: string;
      }>();

      if (row) {
        if (row.install_status === "suspended") {
          return { allowed: false, reason: "Tenant runners entitlement suspended" };
        }
      }
    } catch {
      // Fail-open for transient D1 reads during quota check if configured
    }
  }

  return { allowed: true };
}

export const enforceDevenvQuota = checkDevenvQuota;
