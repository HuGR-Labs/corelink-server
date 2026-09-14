import type { D1Database } from "@cloudflare/workers-types";

const I64_MAX = 9_223_372_036_854_775_807n;
const UUID = /^(?!00000000-0000-0000-0000-000000000000$)[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;

export function validateLifecycleGeneration(value: unknown): string {
  if (typeof value !== "string" || value.length > 19 || !/^(0|[1-9][0-9]*)$/.test(value)) throw new Error("invalid lifecycle generation");
  let number: bigint;
  try { number = BigInt(value); } catch { throw new Error("invalid lifecycle generation"); }
  if (number > I64_MAX) throw new Error("invalid lifecycle generation");
  return value;
}

function validIdentity(value: unknown): value is string {
  return typeof value === "string" && value.length > 0 && value === value.trim();
}

async function bounded<T>(work: Promise<T>, ms = 1_000): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([work, new Promise<never>((_, reject) => {
      timer = setTimeout(() => reject(new Error("credential invalidation timeout")), ms);
    })]);
  } finally {
    if (timer !== undefined) clearTimeout(timer);
  }
}

export async function closeCredentialGeneration(
  db: D1Database,
  tenantId: string,
  throughGeneration: string,
  refreshPatRevocations = true,
  ensureFloor = true,
  verifyIdentity = true,
): Promise<void> {
  const generation = validateLifecycleGeneration(throughGeneration);
  if (!validIdentity(tenantId)) throw new Error("invalid tenant identity");
  const conflicts = verifyIdentity ? await db.prepare(`SELECT 1 FROM credential_generation_revocation q
    JOIN pat p ON p.pat_id = q.pat_id
    WHERE q.tenant_id = ?1 AND (q.token_id <> p.token_id OR q.lifecycle_generation <> p.lifecycle_generation)
    LIMIT 1`).bind(tenantId).all() : { results: [] };
  if ((conflicts.results ?? []).length !== 0) throw new Error("credential revocation identity conflict");
  const statements = [
    ensureFloor ? db.prepare(`INSERT INTO tenant_credential_revocation_floor (tenant_id, revoked_through)
      VALUES (?1, ?2)
      ON CONFLICT(tenant_id) DO UPDATE SET revoked_through = CASE
        WHEN CAST(tenant_credential_revocation_floor.revoked_through AS INTEGER) < CAST(excluded.revoked_through AS INTEGER)
        THEN excluded.revoked_through ELSE tenant_credential_revocation_floor.revoked_through END`).bind(tenantId, generation) : undefined,
    db.prepare(`INSERT OR IGNORE INTO credential_generation_revocation (pat_id, token_id, tenant_id, lifecycle_generation)
      SELECT p.pat_id, p.token_id, p.tenant_id, p.lifecycle_generation
      FROM pat p
      WHERE p.tenant_id = ?1 AND p.lifecycle_generation IS NOT NULL
        AND p.token_id IS NOT NULL
        AND CAST(p.lifecycle_generation AS INTEGER) <= CAST((SELECT revoked_through FROM tenant_credential_revocation_floor WHERE tenant_id = ?1) AS INTEGER)
        AND NOT EXISTS (SELECT 1 FROM credential_generation_revocation q
          WHERE q.pat_id = p.pat_id AND q.token_id = p.token_id
            AND q.tenant_id = p.tenant_id AND q.lifecycle_generation = p.lifecycle_generation)
      ORDER BY p.lifecycle_generation, p.pat_id LIMIT 256`).bind(tenantId),
    refreshPatRevocations ? db.prepare(`UPDATE pat SET revoked_at_ms = COALESCE(revoked_at_ms, CAST(strftime('%s','now') AS INTEGER) * 1000)
      WHERE tenant_id = ?1 AND lifecycle_generation IS NOT NULL
        AND CAST(lifecycle_generation AS INTEGER) <= CAST((SELECT revoked_through FROM tenant_credential_revocation_floor WHERE tenant_id = ?1) AS INTEGER)
        AND revoked_at_ms IS NULL`).bind(tenantId) : undefined,
  ].filter((statement): statement is NonNullable<typeof statement> => statement !== undefined);
  const results = await db.batch(statements);
  if (results.length !== statements.length || results.some((result) => !result.success)) throw new Error("credential generation close not confirmed");
}

interface PendingRevocation { pat_id: string; token_id: string; tenant_id: string; lifecycle_generation: string }

export async function drainCredentialGenerationRevocations(
  db: D1Database,
  kv: { delete(key: string): Promise<void> } | undefined,
  tenantId: string,
  throughGeneration: string,
): Promise<{ complete: boolean; revoked: number }> {
  const generation = validateLifecycleGeneration(throughGeneration);
  if (!validIdentity(tenantId)) throw new Error("invalid tenant identity");
  if (kv === undefined) throw new Error("credential metadata KV is unavailable");
  const rows = await db.prepare(`SELECT pat_id, token_id, tenant_id, lifecycle_generation
      FROM credential_generation_revocation
    WHERE tenant_id = ?1 AND state = 'pending' AND CAST(lifecycle_generation AS INTEGER) <= CAST(?2 AS INTEGER)
    ORDER BY lifecycle_generation, pat_id LIMIT 256`).bind(tenantId, generation).all<PendingRevocation>();
  const pending = rows.results ?? [];
  let revoked = 0;
  let processed = 0;
  let failed = false;
  for (const row of pending) {
    if (processed >= 4) break;
    if (!UUID.test(row.pat_id) || !validIdentity(row.token_id) || row.tenant_id !== tenantId) continue;
    try { validateLifecycleGeneration(row.lifecycle_generation); } catch { continue; }
    processed++;
    try {
      await bounded(kv.delete(`patrow:${row.token_id}`));
      const acknowledged = await db.prepare(`UPDATE credential_generation_revocation SET state = 'revoked'
        WHERE pat_id = ?1 AND token_id = ?2 AND tenant_id = ?3 AND lifecycle_generation = ?4 AND state = 'pending'`).bind(
        row.pat_id, row.token_id, row.tenant_id, row.lifecycle_generation,
      ).run();
      if (acknowledged.meta.changes === 1) revoked++;
      else failed = true;
    } catch {
      failed = true;
    }
  }
  const remaining = await db.prepare(`SELECT 1 FROM credential_generation_revocation
    WHERE tenant_id = ?1 AND state = 'pending' AND CAST(lifecycle_generation AS INTEGER) <= CAST(?2 AS INTEGER) LIMIT 1`).bind(tenantId, generation).first();
  if (failed || remaining !== null) return { complete: false, revoked };
  const unqueued = await db.prepare(`SELECT 1 FROM pat p
    WHERE p.tenant_id = ?1 AND p.lifecycle_generation IS NOT NULL AND p.token_id IS NOT NULL
      AND CAST(p.lifecycle_generation AS INTEGER) <= CAST(?2 AS INTEGER)
      AND NOT EXISTS (SELECT 1 FROM credential_generation_revocation q
        WHERE q.pat_id = p.pat_id AND q.token_id = p.token_id
          AND q.tenant_id = p.tenant_id AND q.lifecycle_generation = p.lifecycle_generation)
    LIMIT 1`).bind(tenantId, generation).first();
  return { complete: unqueued === null, revoked };
}
