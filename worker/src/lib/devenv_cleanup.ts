/** Durable DevEnv credential obligations. Raw token plaintext never enters this module. */
import type { DurableObjectStorage } from "@cloudflare/workers-types";

export const PREPARE_MS = 90_000;
const PREFIX = "devenv-cleanup/";
const NOW = "CAST(strftime('%s','now') AS INTEGER) * 1000";
const GENERATION = "length(lifecycle_generation) BETWEEN 1 AND 19 AND lifecycle_generation NOT GLOB '*[^0-9]*' AND (length(lifecycle_generation) = 1 OR substr(lifecycle_generation, 1, 1) <> '0') AND (length(lifecycle_generation) < 19 OR lifecycle_generation <= '9223372036854775807')";
const UUID = /^(?!00000000-0000-0000-0000-000000000000$)[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
interface Marker { schema_version: 1; operationId: string; tenantId: string; lifecycleGeneration: string; deadline: number; due: number; attempts: number }
interface Row { state: string; token_id: string | null; pat_id?: string | null }
const validGeneration = (v: unknown): v is string => typeof v === "string" && /^(0|[1-9][0-9]*)$/.test(v) && v.length <= 19 && (v.length < 19 || v <= "9223372036854775807");
const validMarker = (v: unknown, key: string): v is Marker => {
  if (!v || typeof v !== "object") return false; const m = v as Partial<Marker>;
  return m.schema_version === 1 && typeof m.operationId === "string" && PREFIX + encodeURIComponent(m.operationId) === key && UUID.test(m.operationId) && UUID.test(m.tenantId ?? "") && validGeneration(m.lifecycleGeneration) && [m.deadline, m.due, m.attempts].every(n => typeof n === "number" && Number.isSafeInteger(n)) && m.deadline! > 0 && m.due! >= 0 && m.attempts! >= 0;
};
export async function bounded<T>(work: Promise<T>, ms = 5000): Promise<T> { let timer: ReturnType<typeof setTimeout> | undefined; try { return await Promise.race([work, new Promise<never>((_, reject) => { timer = setTimeout(() => reject(new Error("DevEnv dependency timeout")), ms); })]); } finally { if (timer) clearTimeout(timer); } }

export async function prepareDevenvOperation(storage: DurableObjectStorage, db: D1Database, operationId: string, tenantId: string, now: number, generation: string): Promise<boolean> {
  if (!UUID.test(operationId) || !UUID.test(tenantId) || !validGeneration(generation) || !Number.isSafeInteger(now) || now < 0 || now > Number.MAX_SAFE_INTEGER - PREPARE_MS) return false;
  const deadline = now + PREPARE_MS, key = PREFIX + encodeURIComponent(operationId);
  const retained = await bounded(storage.transaction(async txn => {
    const old = await txn.get(key) as Marker | undefined;
    if (old !== undefined) { if (!validMarker(old, key) || old.tenantId !== tenantId || old.lifecycleGeneration !== generation) throw new Error("malformed DevEnv cleanup marker"); if (old.deadline <= now) return null; }
    else { if ((await txn.list({ prefix: PREFIX, limit: 65 })).size >= 64) throw new Error("DevEnv cleanup capacity exhausted"); await txn.put(key, { schema_version: 1, operationId, tenantId, lifecycleGeneration: generation, deadline, due: deadline + 2000, attempts: 0 } satisfies Marker); }
    const due = old?.due ?? deadline + 2000, alarm = await txn.getAlarm(); if (alarm === null || alarm > due) await txn.setAlarm(due); return old?.deadline ?? deadline;
  }));
  if (retained === null) return false;
  const result = await bounded(db.prepare(`INSERT INTO devenv_credential_obligation (operation_id, tenant_id, state, deadline_ms, lifecycle_generation) SELECT ?1, ?2, 'prepared', ?3, ?4 WHERE ?3 > ${NOW} AND NOT EXISTS (SELECT 1 FROM tenant_credential_revocation_floor WHERE tenant_id = ?2 AND (length(revoked_through) > length(?4) OR (length(revoked_through) = length(?4) AND revoked_through >= ?4))) ON CONFLICT(operation_id) DO NOTHING`).bind(operationId, tenantId, retained, generation).run());
  if (result.meta.changes === 1) return true;
  const row = await bounded(db.prepare("SELECT tenant_id, lifecycle_generation, state, deadline_ms FROM devenv_credential_obligation WHERE operation_id = ?1").bind(operationId).first<{tenant_id:string; lifecycle_generation:string; state:string; deadline_ms:number}>());
  return row?.tenant_id === tenantId && row.lifecycle_generation === generation && row.state === "prepared" && row.deadline_ms === retained;
}

export async function activateDevenvPat(db: D1Database, operationId: string, tenantId: string, minted: { pat_id: string; token_id: string; hash: string; expires_ms: number }, scope: string, generation: string): Promise<boolean> {
  if (!UUID.test(operationId) || !UUID.test(tenantId) || !validGeneration(generation) || !minted.pat_id || !minted.token_id || !minted.hash) return false;
  const results = await bounded(db.batch([
    db.prepare(`INSERT INTO pat (pat_id, tenant_id, pat_hash, scope, expires_ms, token_id, shown_once_token, shown_once_consumed, created_ms, lifecycle_generation) SELECT ?1, ?2, ?3, ?4, ?5, ?6, ?6, 1, ${NOW}, lifecycle_generation FROM devenv_credential_obligation WHERE operation_id = ?7 AND tenant_id = ?2 AND lifecycle_generation = ?8 AND state = 'prepared' AND deadline_ms > ${NOW} AND ${GENERATION} AND NOT EXISTS (SELECT 1 FROM tenant_credential_revocation_floor f WHERE f.tenant_id = ?2 AND (length(f.revoked_through) > length(lifecycle_generation) OR (length(f.revoked_through) = length(lifecycle_generation) AND f.revoked_through >= lifecycle_generation)))`).bind(minted.pat_id, tenantId, minted.hash, scope, minted.expires_ms, minted.token_id, operationId, generation),
    db.prepare(`UPDATE devenv_credential_obligation SET state = 'issued', pat_id = ?1, token_id = ?2 WHERE operation_id = ?3 AND tenant_id = ?4 AND lifecycle_generation = ?5 AND state = 'prepared' AND EXISTS (SELECT 1 FROM pat WHERE pat_id = ?1 AND tenant_id = ?4 AND token_id = ?2 AND lifecycle_generation = ?5)`).bind(minted.pat_id, minted.token_id, operationId, tenantId, generation),
  ]));
  return results.length === 2 && results.every(r => r.success && r.meta.changes === 1);
}
export async function adoptDevenvOperation(db: D1Database, operationId: string, tenantId: string, patId: string): Promise<boolean> {
  if (!UUID.test(operationId) || !UUID.test(tenantId) || !patId) return false;
  const result = await bounded(db.prepare(`UPDATE devenv_credential_obligation SET state = 'adopted' WHERE operation_id = ?1 AND tenant_id = ?2 AND pat_id = ?3 AND state = 'issued' AND deadline_ms > ${NOW} AND ${GENERATION} AND NOT EXISTS (SELECT 1 FROM tenant_credential_revocation_floor f WHERE f.tenant_id = ?2 AND (length(f.revoked_through) > length(lifecycle_generation) OR (length(f.revoked_through) = length(lifecycle_generation) AND f.revoked_through >= lifecycle_generation)))`).bind(operationId, tenantId, patId).run());
  return result.meta.changes === 1;
}
export async function revokeDevenvOperation(db: D1Database, kv: { delete(key: string): Promise<void> } | undefined, operationId: string, tenantId: string): Promise<boolean> {
  if (!UUID.test(operationId) || !UUID.test(tenantId)) return false;
  const results = await bounded(db.batch([db.prepare(`INSERT OR IGNORE INTO devenv_credential_obligation (operation_id, tenant_id, state, deadline_ms) VALUES (?1, ?2, 'revoked', ${NOW})`).bind(operationId, tenantId), db.prepare("UPDATE devenv_credential_obligation SET state = 'revoking' WHERE operation_id = ?1 AND tenant_id = ?2 AND state IN ('prepared','issued','revoking')").bind(operationId, tenantId), db.prepare(`UPDATE pat SET revoked_at_ms = COALESCE(revoked_at_ms, ${NOW}) WHERE pat_id = (SELECT pat_id FROM devenv_credential_obligation WHERE operation_id = ?1 AND tenant_id = ?2 AND state = 'revoking') AND tenant_id = ?2`).bind(operationId, tenantId)]));
  if (results.length !== 3 || results.some(r => !r.success)) throw new Error("DevEnv revocation not confirmed");
  const row = await bounded(db.prepare("SELECT state, token_id FROM devenv_credential_obligation WHERE operation_id = ?1 AND tenant_id = ?2").bind(operationId, tenantId).first<Row>()); if (!row) return false; if (row.state === "adopted" || row.state === "revoked") return true; if (row.state !== "revoking") return false;
  if (row.token_id) { if (!kv) throw new Error("DevEnv revocation cache unavailable"); await bounded(kv.delete(`patrow:${row.token_id}`)); }
  return (await bounded(db.prepare("UPDATE devenv_credential_obligation SET state = 'revoked' WHERE operation_id = ?1 AND tenant_id = ?2 AND state = 'revoking'").bind(operationId, tenantId).run())).meta.changes === 1;
}
export async function drainDevenvOperations(storage: DurableObjectStorage, db: D1Database, kv: { delete(key: string): Promise<void> } | undefined, now: number): Promise<boolean> {
  const entries = await bounded(storage.list<unknown>({ prefix: PREFIX, limit: 64 })); const due: [string, Marker][] = [];
  for (const [key, value] of entries) { if (!validMarker(value, key)) { console.error("malformed DevEnv cleanup marker retained"); continue; } if (value.due <= now) due.push([key, value]); }
  due.sort((a,b) => a[1].due - b[1].due).splice(4);
  for (const [key, marker] of due) try { if (await revokeDevenvOperation(db, kv, marker.operationId, marker.tenantId)) await bounded(storage.delete(key)); else throw new Error("obligation not confirmed"); } catch { const attempts = Math.min(marker.attempts + 1, 9); await bounded(storage.put(key, { ...marker, attempts, due: now + Math.min(300000, 1000 * 2 ** attempts) } satisfies Marker)); }
  return (await bounded(storage.list({ prefix: PREFIX, limit: 1 }))).size > 0;
}
