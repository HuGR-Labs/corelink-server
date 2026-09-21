import { isCanonicalTenantUuid } from "./tenant_uuid.js";

/** Runner mint handoff: issuer owns cleanup until the Worker durably adopts. */
export const RUNNER_PREPARE_MS = 90_000;
export interface RunnerCredentialOperation {
  operationId: string;
  tenantId: string;
  jobId: string;
  repo: string;
  lifecycleGeneration?: string;
}
export interface RunnerIssuedPat {
  pat_id: string;
  token_id: string;
  hash: string;
  expires_ms: number;
}

const PREFIX = "runner-credential/";
const UUID = /^(?!00000000-0000-0000-0000-000000000000$)[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
interface Marker extends RunnerCredentialOperation { schema_version: 1; deadline_ms: number; due: number; attempts: number }
interface Row extends RunnerCredentialOperation { state: string; deadline_ms: number; token_id: string | null; pat_id: string | null; lifecycle_generation: string }

function validOperation(o: RunnerCredentialOperation): boolean {
  return UUID.test(o.operationId) && isCanonicalTenantUuid(o.tenantId) &&
    typeof o.jobId === "string" && o.jobId.trim() === o.jobId && o.jobId.trim() !== "" &&
    typeof o.repo === "string" && o.repo.trim() === o.repo && o.repo.trim() !== "";
}
const MAX_GENERATION = 0x7fff_ffff_ffff_ffffn;
const DECIMAL_GENERATION = /^(0|[1-9][0-9]*)$/;
function validGeneration(value: unknown): value is string {
  if (typeof value !== "string" || value.length > 19 || !DECIMAL_GENERATION.test(value)) return false;
  try { return BigInt(value) <= MAX_GENERATION; } catch { return false; }
}
function validModernOperation(o: RunnerCredentialOperation): boolean {
  return validOperation(o) && validGeneration(o.lifecycleGeneration);
}
function generationForCleanup(o: RunnerCredentialOperation): string | null {
  if (!validOperation(o) || (o.lifecycleGeneration !== undefined && !validGeneration(o.lifecycleGeneration))) return null;
  return o.lifecycleGeneration ?? "0";
}
function generationSql(column: string): string {
  return `length(${column}) BETWEEN 1 AND 19 AND ${column} NOT GLOB '*[^0-9]*' AND (length(${column}) = 1 OR substr(${column}, 1, 1) <> '0') AND NOT (length(${column}) = 19 AND ${column} > '9223372036854775807')`;
}

function markerKey(operationId: string): string { return PREFIX + encodeURIComponent(operationId); }
function validMarker(value: unknown, key: string): value is Marker {
  if (typeof value !== "object" || value === null) return false;
  const marker = value as Partial<Marker>;
  return marker.schema_version === 1 && typeof marker.operationId === "string" &&
    markerKey(marker.operationId) === key && validOperation(marker as RunnerCredentialOperation) &&
    (marker.lifecycleGeneration === undefined || validGeneration(marker.lifecycleGeneration)) &&
    typeof marker.deadline_ms === "number" && Number.isSafeInteger(marker.deadline_ms) && marker.deadline_ms > 0 &&
    typeof marker.due === "number" && Number.isSafeInteger(marker.due) && marker.due >= 0 &&
    typeof marker.attempts === "number" && Number.isSafeInteger(marker.attempts) && marker.attempts >= 0;
}
async function bounded<T>(work: Promise<T>, ms = 5000): Promise<T> {
  let timer: ReturnType<typeof setTimeout> | undefined;
  try { return await Promise.race([work, new Promise<never>((_, reject) => { timer = setTimeout(() => reject(new Error("runner credential dependency timeout")), ms); })]); }
  finally { if (timer !== undefined) clearTimeout(timer); }
}

// Frozen implementation anchors: no authenticatable PAT without a prepared
// obligation; permanent revocation tombstones; no adoption after the deadline.
export async function prepareRunnerOperation(storage: DurableObjectStorage, db: D1Database,
  operation: RunnerCredentialOperation, now: number): Promise<boolean> {
  if (!validModernOperation(operation) || !Number.isSafeInteger(now) || now < 0 || now > Number.MAX_SAFE_INTEGER - RUNNER_PREPARE_MS) return false;
  const initialDeadline = now + RUNNER_PREPARE_MS;
  const deadline = await bounded(storage.transaction(async (txn) => {
    const key = markerKey(operation.operationId);
    const existing = await txn.get(key) as Marker | undefined;
    if (existing !== undefined) {
      if (!validMarker(existing, key) || existing.lifecycleGeneration !== operation.lifecycleGeneration || existing.operationId !== operation.operationId || existing.tenantId !== operation.tenantId || existing.jobId !== operation.jobId || existing.repo !== operation.repo) throw new Error("malformed runner credential marker");
      if (existing.deadline_ms <= now) return null;
    } else {
      if ((await txn.list({ prefix: PREFIX, limit: 501 })).size >= 500) throw new Error("runner credential capacity exhausted");
      await txn.put(key, { schema_version: 1, ...operation, deadline_ms: initialDeadline, due: initialDeadline, attempts: 0 } satisfies Marker);
    }
    const alarm = await txn.getAlarm();
    const due = existing?.due ?? initialDeadline;
    if (alarm === null || alarm > due) await txn.setAlarm(due);
    return existing?.deadline_ms ?? initialDeadline;
  }));
  if (deadline === null) return false;
  let inserted: { meta: { changes: number } };
  try {
    inserted = await bounded(db.prepare(`INSERT INTO runner_credential_obligation (operation_id, tenant_id, job_id, repo, state, deadline_ms, lifecycle_generation)
      SELECT ?1, ?2, ?3, ?4, 'prepared', ?5, ?6 WHERE ?5 > CAST(strftime('%s','now') AS INTEGER) * 1000
      AND ${generationSql("?6")} AND NOT EXISTS (SELECT 1 FROM tenant_credential_revocation_floor WHERE tenant_id = ?2 AND (NOT ${generationSql("revoked_through")} OR CAST(revoked_through AS INTEGER) >= CAST(?6 AS INTEGER)))`).bind(operation.operationId, operation.tenantId, operation.jobId, operation.repo, deadline, operation.lifecycleGeneration).run());
  } catch {
    inserted = { meta: { changes: 0 } };
  }
  if (inserted.meta.changes === 1) return true;
  try {
    const row = await bounded(db.prepare(`SELECT tenant_id, job_id, repo, state, deadline_ms
      FROM runner_credential_obligation WHERE operation_id = ?1 AND lifecycle_generation = ?2
      AND deadline_ms > CAST(strftime('%s','now') AS INTEGER) * 1000
      AND ${generationSql("lifecycle_generation")} AND NOT EXISTS (SELECT 1 FROM tenant_credential_revocation_floor WHERE tenant_id = ?3 AND (NOT ${generationSql("revoked_through")} OR CAST(revoked_through AS INTEGER) >= CAST(?2 AS INTEGER)))`).bind(operation.operationId, operation.lifecycleGeneration, operation.tenantId).first<{ tenant_id: string; job_id: string; repo: string; state: string; deadline_ms: number }>());
    return row !== null && row.tenant_id === operation.tenantId && row.job_id === operation.jobId && row.repo === operation.repo && row.state === "prepared" && row.deadline_ms === deadline;
  } catch {
    return false;
  }
}
export async function activateRunnerPat(db: D1Database, operation: RunnerCredentialOperation,
  minted: RunnerIssuedPat, scope: string, runnerJobAcKey: string): Promise<boolean> {
  if (!validModernOperation(operation) || !runnerJobAcKey || !minted.pat_id || !minted.token_id || !minted.hash || !Number.isSafeInteger(minted.expires_ms)) return false;
  const results = await bounded(db.batch([
    db.prepare(`INSERT INTO pat (pat_id, tenant_id, pat_hash, scope, expires_ms, token_id, shown_once_token, shown_once_consumed, created_ms, runner_job_ac_key, lifecycle_generation)
      SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?6, 1, CAST(strftime('%s','now') AS INTEGER) * 1000, ?7, ?8 FROM runner_credential_obligation
      WHERE operation_id = ?9 AND tenant_id = ?2 AND job_id = ?10 AND repo = ?11 AND lifecycle_generation = ?8 AND state = 'prepared' AND deadline_ms > CAST(strftime('%s','now') AS INTEGER) * 1000
      AND ${generationSql("lifecycle_generation")} AND NOT EXISTS (SELECT 1 FROM tenant_credential_revocation_floor WHERE tenant_id = ?2 AND (NOT ${generationSql("revoked_through")} OR CAST(revoked_through AS INTEGER) >= CAST(?8 AS INTEGER)))`)
      .bind(minted.pat_id, operation.tenantId, minted.hash, scope, minted.expires_ms, minted.token_id, runnerJobAcKey, operation.lifecycleGeneration, operation.operationId, operation.jobId, operation.repo),
    db.prepare(`UPDATE runner_credential_obligation SET state = 'issued', pat_id = ?1, token_id = ?2
      WHERE operation_id = ?3 AND tenant_id = ?4 AND job_id = ?5 AND repo = ?6 AND lifecycle_generation = ?7 AND state = 'prepared'
      AND ${generationSql("lifecycle_generation")} AND NOT EXISTS (SELECT 1 FROM tenant_credential_revocation_floor WHERE tenant_id = ?4 AND (NOT ${generationSql("revoked_through")} OR CAST(revoked_through AS INTEGER) >= CAST(?7 AS INTEGER)))
      AND EXISTS (SELECT 1 FROM pat WHERE pat_id = ?1 AND tenant_id = ?4 AND token_id = ?2 AND lifecycle_generation = ?7)`)
      .bind(minted.pat_id, minted.token_id, operation.operationId, operation.tenantId, operation.jobId, operation.repo, operation.lifecycleGeneration),
  ]));
  return results.length === 2 && results.every((r) => r.success && r.meta.changes === 1);
}
export async function adoptRunnerOperation(db: D1Database, operationId: string, patId: string): Promise<boolean> {
  if (!UUID.test(operationId) || !patId) return false;
  const result = await bounded(db.prepare(`UPDATE runner_credential_obligation SET state = 'adopted'
    WHERE operation_id = ?1 AND pat_id = ?2 AND state = 'issued' AND deadline_ms > CAST(strftime('%s','now') AS INTEGER) * 1000
    AND ${generationSql("runner_credential_obligation.lifecycle_generation")} AND NOT EXISTS (SELECT 1 FROM tenant_credential_revocation_floor f WHERE f.tenant_id = runner_credential_obligation.tenant_id AND (NOT ${generationSql("f.revoked_through")} OR CAST(f.revoked_through AS INTEGER) >= CAST(runner_credential_obligation.lifecycle_generation AS INTEGER)))`).bind(operationId, patId).run());
  if (result.meta.changes === 1) return true;
  const row = await bounded(db.prepare(`SELECT state, pat_id FROM runner_credential_obligation
    WHERE operation_id = ?1 AND state = 'adopted' AND pat_id = ?2 AND deadline_ms > CAST(strftime('%s','now') AS INTEGER) * 1000
    AND ${generationSql("runner_credential_obligation.lifecycle_generation")} AND NOT EXISTS (SELECT 1 FROM tenant_credential_revocation_floor f WHERE f.tenant_id = runner_credential_obligation.tenant_id AND (NOT ${generationSql("f.revoked_through")} OR CAST(f.revoked_through AS INTEGER) >= CAST(runner_credential_obligation.lifecycle_generation AS INTEGER)))`).bind(operationId, patId).first<{ state: string; pat_id: string | null }>());
  return row?.state === "adopted" && row.pat_id === patId;
}
export async function revokeRunnerOperation(db: D1Database,
  kv: { delete(key: string): Promise<void> } | undefined, operation: RunnerCredentialOperation): Promise<boolean> {
  const lifecycleGeneration = generationForCleanup(operation);
  if (lifecycleGeneration === null) return false;
  const results = await bounded(db.batch([
    db.prepare(`INSERT OR IGNORE INTO runner_credential_obligation (operation_id, tenant_id, job_id, repo, state, deadline_ms, lifecycle_generation)
      VALUES (?1, ?2, ?3, ?4, 'revoked', CAST(strftime('%s','now') AS INTEGER) * 1000, ?5)`).bind(operation.operationId, operation.tenantId, operation.jobId, operation.repo, lifecycleGeneration),
    db.prepare(`UPDATE runner_credential_obligation SET state = 'revoking' WHERE operation_id = ?1 AND tenant_id = ?2 AND job_id = ?3 AND repo = ?4 AND lifecycle_generation = ?5 AND state IN ('prepared','issued','revoking')`).bind(operation.operationId, operation.tenantId, operation.jobId, operation.repo, lifecycleGeneration),
    db.prepare(`UPDATE pat SET revoked_at_ms = COALESCE(revoked_at_ms, CAST(strftime('%s','now') AS INTEGER) * 1000)
      WHERE pat_id = (SELECT pat_id FROM runner_credential_obligation WHERE operation_id = ?1 AND tenant_id = ?2 AND job_id = ?3 AND repo = ?4 AND lifecycle_generation = ?5 AND state = 'revoking') AND tenant_id = ?2 AND lifecycle_generation = ?5`).bind(operation.operationId, operation.tenantId, operation.jobId, operation.repo, lifecycleGeneration),
  ]));
  if (results.length !== 3 || results.some((r) => !r.success)) throw new Error("runner credential revocation not confirmed");
  const row = await bounded(db.prepare("SELECT operation_id AS operationId, tenant_id AS tenantId, job_id AS jobId, repo, state, deadline_ms, token_id, pat_id, lifecycle_generation FROM runner_credential_obligation WHERE operation_id = ?1 AND tenant_id = ?2 AND job_id = ?3 AND repo = ?4 AND lifecycle_generation = ?5").bind(operation.operationId, operation.tenantId, operation.jobId, operation.repo, lifecycleGeneration).first<Row>());
  if (!row || row.state === "revoked" || row.state === "adopted") return row !== null;
  if (row.state !== "revoking" || row.operationId !== operation.operationId || row.tenantId !== operation.tenantId || row.jobId !== operation.jobId || row.repo !== operation.repo) return false;
  if (row.token_id) {
    if (!kv) throw new Error("runner credential metadata KV is unavailable");
    await bounded(kv.delete(`patrow:${row.token_id}`));
  }
  const done = await bounded(db.prepare("UPDATE runner_credential_obligation SET state = 'revoked' WHERE operation_id = ?1 AND tenant_id = ?2 AND job_id = ?3 AND repo = ?4 AND lifecycle_generation = ?5 AND state = 'revoking'").bind(operation.operationId, operation.tenantId, operation.jobId, operation.repo, lifecycleGeneration).run());
  return done.meta.changes === 1;
}
export async function drainRunnerOperations(storage: DurableObjectStorage, db: D1Database,
  kv: { delete(key: string): Promise<void> } | undefined, now: number): Promise<boolean> {
  const markers = await bounded(storage.list<unknown>({ prefix: PREFIX, limit: 500 }));
  const due: [string, Marker][] = [];
  for (const [key, marker] of markers) {
    if (!validMarker(marker, key)) {
      console.error("malformed runner credential marker retained", { key });
      continue;
    }
    if (marker.due <= now) due.push([key, marker]);
  }
  due.sort((a, b) => a[1].due - b[1].due).splice(4);
  for (const [key, marker] of due) {
    try {
      if (await revokeRunnerOperation(db, kv, { ...marker, lifecycleGeneration: marker.lifecycleGeneration ?? "0" })) await bounded(storage.delete(key));
      else throw new Error("runner credential obligation not confirmed");
    } catch {
      const attempts = Math.min(marker.attempts + 1, 9);
      await bounded(storage.put(key, { ...marker, attempts, due: now + Math.min(300_000, 1000 * 2 ** attempts) } satisfies Marker));
    }
  }
  return (await bounded(storage.list({ prefix: PREFIX, limit: 1 }))).size > 0;
}
