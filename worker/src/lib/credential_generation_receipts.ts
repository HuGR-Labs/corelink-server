import type { D1Database } from "@cloudflare/workers-types";
import { closeCredentialGeneration, drainCredentialGenerationRevocations, validateLifecycleGeneration } from "./credential_generation.js";

export interface CloseGenerationInput {
  event_id: string;
  tenant_id: string;
  lifecycle_generation: string;
}

export interface CredentialGenerationReceipt extends CloseGenerationInput {
  state: "requested" | "complete";
}

export class CredentialGenerationEventError extends Error {
  constructor(readonly code: "conflict" | "unavailable" | "invalid", message: string) { super(message); }
}

function validate(input: CloseGenerationInput): void {
  if (!input || typeof input.event_id !== "string" || input.event_id.length === 0 || new TextEncoder().encode(input.event_id).byteLength > 256 || input.event_id !== input.event_id.trim() ||
    typeof input.tenant_id !== "string" || input.tenant_id.length === 0 ||
    typeof input.lifecycle_generation !== "string") throw new CredentialGenerationEventError("invalid", "invalid suspension event");
  try { validateLifecycleGeneration(input.lifecycle_generation); } catch { throw new CredentialGenerationEventError("invalid", "invalid suspension event"); }
}

function same(a: CloseGenerationInput, b: CloseGenerationInput): boolean {
  return a.event_id === b.event_id && a.tenant_id === b.tenant_id && a.lifecycle_generation === b.lifecycle_generation;
}

async function receipt(db: D1Database, eventId: string): Promise<CredentialGenerationReceipt | null> {
  const row = await db.prepare("SELECT event_id, tenant_id, lifecycle_generation, state FROM credential_generation_event_receipts WHERE event_id = ?1").bind(eventId).first<unknown>();
  if (row === null || row === undefined) return null;
  if (typeof row !== "object" || Array.isArray(row)) throw new CredentialGenerationEventError("unavailable", "credential generation receipt is corrupt");
  const value = row as Record<string, unknown>;
  if (typeof value["event_id"] !== "string" || typeof value["tenant_id"] !== "string" || typeof value["lifecycle_generation"] !== "string" || (value["state"] !== "requested" && value["state"] !== "complete")) {
    throw new CredentialGenerationEventError("unavailable", "credential generation receipt is corrupt");
  }
  try { validateLifecycleGeneration(value["lifecycle_generation"]); } catch { throw new CredentialGenerationEventError("unavailable", "credential generation receipt is corrupt"); }
  return { event_id: value["event_id"], tenant_id: value["tenant_id"], lifecycle_generation: value["lifecycle_generation"], state: value["state"] };
}

async function floorCovers(db: D1Database, input: CloseGenerationInput): Promise<boolean> {
  const row = await db.prepare("SELECT revoked_through FROM tenant_credential_revocation_floor WHERE tenant_id = ?1").bind(input.tenant_id).first<{ revoked_through: string }>();
  if (!row) return false;
  try { validateLifecycleGeneration(row.revoked_through); } catch { return false; }
  return BigInt(row.revoked_through) >= BigInt(input.lifecycle_generation);
}

export async function closeGenerationEvent(
  db: D1Database,
  kv: { delete(key: string): Promise<void> } | undefined,
  input: CloseGenerationInput,
  budget: number,
): Promise<{ complete: boolean }> {
  validate(input);
  if (!Number.isSafeInteger(budget) || budget < 1 || budget > 256) throw new CredentialGenerationEventError("invalid", "invalid generation event budget");
  let prior: CredentialGenerationReceipt | null;
  try { prior = await receipt(db, input.event_id); } catch { throw new CredentialGenerationEventError("unavailable", "credential generation unavailable"); }
  if (prior) {
    if (!same(prior, input)) throw new CredentialGenerationEventError("conflict", "suspension event identity conflict");
    if (prior.state === "complete") {
      let covered = false;
      try { covered = await floorCovers(db, input); } catch { throw new CredentialGenerationEventError("unavailable", "credential generation receipt is corrupt"); }
      if (!covered) throw new CredentialGenerationEventError("unavailable", "credential generation receipt is corrupt");
      const check = await drainCredentialGenerationRevocations(db, kv, input.tenant_id, input.lifecycle_generation).catch(() => ({ complete: false, revoked: 0 }));
      if (!check.complete) throw new CredentialGenerationEventError("unavailable", "credential generation receipt is incomplete");
      return { complete: true };
    }
  } else {
    try {
      const result = await db.prepare(`INSERT INTO credential_generation_event_receipts
        (event_id, tenant_id, lifecycle_generation, state) VALUES (?1, ?2, ?3, 'requested')`)
        .bind(input.event_id, input.tenant_id, input.lifecycle_generation).run();
      if (!result.success) throw new Error("receipt insert failed");
    } catch (error) {
      const existing = await receipt(db, input.event_id).catch(() => null);
      if (!existing) throw new CredentialGenerationEventError("unavailable", "credential generation unavailable");
      if (!same(existing, input)) throw new CredentialGenerationEventError("conflict", "suspension event identity conflict");
      prior = existing;
    }
  }
  try {
    for (let page = 0; page < budget; page++) {
      await closeCredentialGeneration(db, input.tenant_id, input.lifecycle_generation);
      const drained = await drainCredentialGenerationRevocations(db, kv, input.tenant_id, input.lifecycle_generation);
      if (!drained.complete) continue;
      const updated = await db.prepare(`UPDATE credential_generation_event_receipts SET state = 'complete'
        WHERE event_id = ?1 AND tenant_id = ?2 AND lifecycle_generation = ?3 AND state = 'requested'`)
        .bind(input.event_id, input.tenant_id, input.lifecycle_generation).run();
      if (!updated.success || updated.meta.changes !== 1) throw new Error("receipt completion failed");
      return { complete: true };
    }
    return { complete: false };
  } catch (error) {
    if (error instanceof CredentialGenerationEventError) throw error;
    if (error instanceof Error && error.message.includes("identity conflict")) throw new CredentialGenerationEventError("conflict", "suspension event identity conflict");
    throw new CredentialGenerationEventError("unavailable", "credential generation unavailable");
  }
}
