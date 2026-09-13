/** Durable DevEnv credential obligations. No plaintext token is stored here. */
export const PREPARE_MS = 90_000;
const PREFIX = "devenv-cleanup/";
const SQL_NOW = "CAST(strftime('%s','now') AS INTEGER) * 1000";
const SQL_VALID_GENERATION = "length(lifecycle_generation) BETWEEN 1 AND 19 AND lifecycle_generation NOT GLOB '*[^0-9]*' AND (length(lifecycle_generation) = 1 OR substr(lifecycle_generation, 1, 1) <> '0') AND (length(lifecycle_generation) < 19 OR lifecycle_generation <= '9223372036854775807')";
const UUID = /^(?!00000000-0000-0000-0000-000000000000$)[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
interface Marker { due: number; attempts: number; tenantId: string; lifecycleGeneration?: string }
interface Intent { state: string; deadline_ms: number; token_id: string | null; lifecycle_generation?: string }
const I64_MAX = "9223372036854775807";
function validGeneration(value: unknown): value is string {
  return typeof value === "string" && /^(0|[1-9][0-9]*)$/.test(value) && value.length <= 19 && (value.length < 19 || value <= I64_MAX);
}

/** Timeout bounds the caller. It does not claim to cancel a remote mutation. */
export async function bounded<T>(work: Promise<T>, ms = 5000): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try {
    return await Promise.race([work, new Promise<never>((_, reject) => {
      timer = setTimeout(() => reject(new Error("DevEnv dependency timeout")), ms);
    })]);
  } finally { if (timer !== undefined) clearTimeout(timer); }
}

export async function prepareDevenvOperation(storage: DurableObjectStorage, db: D1Database,
  operationId: string, tenantId: string, now: number, lifecycleGeneration: string): Promise<boolean> {
  if (!UUID.test(operationId) || !UUID.test(tenantId) || !validGeneration(lifecycleGeneration)) return false;
  await bounded(db.prepare("SELECT operation_id FROM devenv_credential_obligation LIMIT 1").all());
  const deadline = now + PREPARE_MS;
  // Marker + alarm precede intent creation. Cleanup retains a terminal D1
  // tombstone, so even a very late prepare cannot recreate an untracked intent.
  await bounded(storage.transaction(async (txn) => {
    if ((await txn.list({ prefix: PREFIX, limit: 64 })).size >= 64) throw new Error("DevEnv cleanup capacity exhausted");
    const existing = await txn.get<Marker>(PREFIX + operationId);
    if (existing) {
      if ((existing.lifecycleGeneration ?? "0") !== lifecycleGeneration || existing.tenantId !== tenantId) throw new Error("DevEnv cleanup obligation conflict");
    } else await txn.put(PREFIX + operationId, { due: deadline + 2000, attempts: 0, tenantId, lifecycleGeneration } satisfies Marker);
    const alarm = await txn.getAlarm();
    if (alarm === null || alarm > deadline + 2000) await txn.setAlarm(deadline + 2000);
  }));
  const result = await bounded(db.prepare(
    `INSERT INTO devenv_credential_obligation (operation_id, tenant_id, state, deadline_ms, lifecycle_generation) SELECT ?1, ?2, 'prepared', ?3, ?4 WHERE ?3 > ${SQL_NOW} AND NOT EXISTS (SELECT 1 FROM tenant_credential_revocation_floor WHERE tenant_id = ?2 AND (length(revoked_through) > length(?4) OR (length(revoked_through) = length(?4) AND revoked_through >= ?4))) ON CONFLICT(operation_id) DO NOTHING`,
  ).bind(operationId, tenantId, deadline, lifecycleGeneration).run());
  return result.meta.changes === 1;
}

/** Atomic activation: no authenticatable PAT may exist without its obligation. */
export async function activateDevenvPat(db: D1Database, operationId: string, tenantId: string,
  minted: { pat_id: string; token_id: string; hash: string; expires_ms: number }, scope: string,
  lifecycleGeneration: string): Promise<boolean> {
  const results = await bounded(db.batch([
    db.prepare(`INSERT INTO pat (pat_id, tenant_id, pat_hash, scope, expires_ms, token_id, shown_once_token, shown_once_consumed, created_ms, lifecycle_generation)
      SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?6, 1, ${SQL_NOW}, lifecycle_generation FROM devenv_credential_obligation
      WHERE operation_id = ?7 AND tenant_id = ?2 AND lifecycle_generation = ?8 AND state = 'prepared' AND deadline_ms > ${SQL_NOW}
      AND ${SQL_VALID_GENERATION}
      AND NOT EXISTS (SELECT 1 FROM tenant_credential_revocation_floor f WHERE f.tenant_id = ?2 AND (length(f.revoked_through) > length(lifecycle_generation) OR (length(f.revoked_through) = length(lifecycle_generation) AND f.revoked_through >= lifecycle_generation)))`)
      .bind(minted.pat_id, tenantId, minted.hash, scope, minted.expires_ms, minted.token_id, operationId, lifecycleGeneration),
    db.prepare(`UPDATE devenv_credential_obligation SET state = 'issued', pat_id = ?1, token_id = ?2
      WHERE operation_id = ?3 AND tenant_id = ?4 AND lifecycle_generation = ?5 AND state = 'prepared'
      AND EXISTS (SELECT 1 FROM pat WHERE pat_id = ?1 AND tenant_id = ?4 AND lifecycle_generation = ?5)`)
      .bind(minted.pat_id, minted.token_id, operationId, tenantId, lifecycleGeneration),
  ]));
  return results.length === 2 && results.every((result) => result.success && result.meta.changes === 1);
}

/** Only a matching, validated RPC acknowledgement transfers cleanup ownership. */
export async function adoptDevenvOperation(db: D1Database, operationId: string, tenantId: string, patId: string): Promise<boolean> {
  const result = await bounded(db.prepare(`UPDATE devenv_credential_obligation SET state = 'adopted' WHERE operation_id = ?1 AND tenant_id = ?2 AND pat_id = ?3 AND state = 'issued' AND deadline_ms > ${SQL_NOW} AND ${SQL_VALID_GENERATION} AND NOT EXISTS (SELECT 1 FROM tenant_credential_revocation_floor f WHERE f.tenant_id = ?2 AND (length(f.revoked_through) > length(lifecycle_generation) OR (length(f.revoked_through) = length(lifecycle_generation) AND f.revoked_through >= lifecycle_generation)))`)
    .bind(operationId, tenantId, patId).run());
  return result.meta.changes === 1;
}

/** Fencing and revocation are one transaction; a late mint can never reactivate. */
export async function revokeDevenvOperation(db: D1Database,
  kv: { delete(key: string): Promise<void> } | undefined, operationId: string, tenantId: string): Promise<boolean> {
  const fenced = await bounded(db.batch([
    db.prepare(`INSERT OR IGNORE INTO devenv_credential_obligation (operation_id, tenant_id, state, deadline_ms)
      VALUES (?1, ?2, 'revoked', ${SQL_NOW})`).bind(operationId, tenantId),
    db.prepare(`UPDATE devenv_credential_obligation SET state = 'revoking' WHERE operation_id = ?1 AND tenant_id = ?2 AND state IN ('prepared', 'issued', 'revoking') AND ${SQL_VALID_GENERATION}`).bind(operationId, tenantId),
    db.prepare(`UPDATE pat SET revoked_at_ms = COALESCE(revoked_at_ms, ${SQL_NOW})
      WHERE pat_id = (SELECT pat_id FROM devenv_credential_obligation WHERE operation_id = ?1 AND tenant_id = ?2 AND state = 'revoking')
      AND tenant_id = ?2`).bind(operationId, tenantId),
  ]));
  if (fenced.length !== 3 || fenced.some((result) => !result.success)) throw new Error("DevEnv revocation not confirmed");
  const row = await bounded(db.prepare("SELECT state, deadline_ms, token_id FROM devenv_credential_obligation WHERE operation_id = ?1 AND tenant_id = ?2")
    .bind(operationId, tenantId).first<Intent>());
  if (row === null) return false;
  if (row.state === "adopted" || row.state === "revoked") return true;
  if (row.state !== "revoking") return false;
  // Match the existing runner revoke surface's patrow KV cache invalidation.
  // Unlike its TTL-only fallback, preserve this obligation until KV succeeds.
  if (row.token_id !== null) {
    if (kv === undefined) throw new Error("DevEnv revocation cache unavailable");
    await bounded(kv.delete(`patrow:${row.token_id}`));
  }
  await bounded(db.prepare("UPDATE devenv_credential_obligation SET state = 'revoked' WHERE operation_id = ?1 AND tenant_id = ?2 AND state = 'revoking'").bind(operationId, tenantId).run());
  return true;
}

/** Existing DO alarm driver; each invocation processes at most four markers. */
export async function drainDevenvOperations(storage: DurableObjectStorage, db: D1Database,
  kv: { delete(key: string): Promise<void> } | undefined, now: number): Promise<boolean> {
  const markers = await bounded(storage.list<Marker>({ prefix: PREFIX, limit: 64 }));
  const due = [...markers].filter(([, marker]) => marker.due <= now).sort((a, b) => a[1].due - b[1].due).slice(0, 4);
  for (const [key, marker] of due) {
    try {
      const done = await revokeDevenvOperation(db, kv, key.slice(PREFIX.length), marker.tenantId);
      if (done) await bounded(storage.delete(key));
    } catch {
      // A bounded attempt failed; the durable marker remains the source of work.
      const attempts = Math.min(marker.attempts + 1, 9);
      await bounded(storage.put(key, { attempts, due: now + Math.min(300_000, 1000 * 2 ** attempts), tenantId: marker.tenantId } satisfies Marker));
    }
  }
  // A new prepare can interleave here. It sets its own alarm, which this driver
  // never deletes, so observing an empty set cannot strand a later insert.
  return (await bounded(storage.list({ prefix: PREFIX, limit: 1 }))).size > 0;
}
