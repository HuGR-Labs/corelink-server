import type { D1Database } from "@cloudflare/workers-types";
import { validateLifecycleGeneration } from "./credential_generation.js";

export interface LegacyCredentialCoverage { complete: boolean }
const UUID = /^(?!00000000-0000-0000-0000-000000000000$)[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

/** Read-only, fail-closed proof that the requested floor covers legacy PATs. */
export async function readLegacyCredentialCoverage(db: D1Database, tenantId: string, throughGeneration: string): Promise<LegacyCredentialCoverage> {
  if (typeof tenantId !== "string" || !UUID.test(tenantId) || tenantId !== tenantId.toLowerCase()) throw new Error("invalid coverage identity");
  validateLifecycleGeneration(throughGeneration);
  const row = await db.prepare(`WITH floor AS (SELECT revoked_through FROM tenant_credential_revocation_floor WHERE tenant_id = ?1), classified AS (
    SELECT p.pat_id,
      (p.revoked_at_ms IS NULL AND typeof(p.expires_ms) = 'integer' AND p.expires_ms > CAST(strftime('%s','now') AS INTEGER) * 1000) AS active,
      ((typeof(p.runner_job_ac_key) = 'text' AND (p.runner_job_ac_key = '*' OR (length(p.runner_job_ac_key) = 64 AND p.runner_job_ac_key NOT GLOB '*[^0-9a-f]*'))) OR EXISTS (SELECT 1 FROM runner_credential_obligation o WHERE o.pat_id = p.pat_id AND o.tenant_id = p.tenant_id) OR EXISTS (SELECT 1 FROM devenv_credential_obligation o WHERE o.pat_id = p.pat_id AND o.tenant_id = p.tenant_id)) AS known_runner,
      EXISTS (SELECT 1 FROM customer_audit_events a WHERE a.tenant_id = p.tenant_id AND a.event_type = 'pat.created' AND a.target = p.pat_id) AS known_customer,
      (p.pat_id IS NULL OR typeof(p.pat_id) <> 'text' OR length(p.pat_id) = 0 OR p.tenant_id IS NULL OR typeof(p.tenant_id) <> 'text' OR p.tenant_id <> ?1 OR typeof(p.token_id) <> 'text' OR length(p.token_id) = 0 OR typeof(p.expires_ms) <> 'integer' OR p.expires_ms < 0 OR (p.revoked_at_ms IS NOT NULL AND (typeof(p.revoked_at_ms) <> 'integer' OR p.revoked_at_ms < 0)) OR (p.runner_job_ac_key IS NOT NULL AND NOT (typeof(p.runner_job_ac_key) = 'text' AND (p.runner_job_ac_key = '*' OR (length(p.runner_job_ac_key) = 64 AND p.runner_job_ac_key NOT GLOB '*[^0-9a-f]*'))))) AS malformed,
      (p.lifecycle_generation IS NULL OR p.lifecycle_generation = '0' OR (length(p.lifecycle_generation) < length(?2) OR (length(p.lifecycle_generation) = length(?2) AND p.lifecycle_generation <= ?2))) AS within_floor
    FROM pat p WHERE p.tenant_id = ?1)
    SELECT (SELECT revoked_through FROM floor) AS floor_value, COALESCE(SUM(CASE WHEN malformed THEN 1 ELSE 0 END), 0) AS malformed,
      COALESCE(SUM(CASE WHEN known_runner AND within_floor THEN 1 ELSE 0 END), 0) AS known_inventory,
      COALESCE(SUM(CASE WHEN known_customer AND within_floor THEN 1 ELSE 0 END), 0) AS known_customer,
      COALESCE(SUM(CASE WHEN active AND known_runner AND within_floor THEN 1 ELSE 0 END), 0) AS active_known_runner,
      COALESCE(SUM(CASE WHEN active AND NOT known_runner AND NOT known_customer AND within_floor THEN 1 ELSE 0 END), 0) AS active_unknown FROM classified`).bind(tenantId, throughGeneration).first<unknown>();
  if (!row || typeof row !== "object") throw new Error("coverage unavailable");
  const value = row as Record<string, unknown>;
  if (typeof value["floor_value"] !== "string" || (value["known_inventory"] === 0 && value["known_customer"] === 0) || value["malformed"] !== 0 || value["active_known_runner"] !== 0 || value["active_unknown"] !== 0) return { complete: false };
  try { validateLifecycleGeneration(value["floor_value"]); return { complete: BigInt(value["floor_value"] as string) >= BigInt(throughGeneration) }; } catch { return { complete: false }; }
}
