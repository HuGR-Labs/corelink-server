import type { Env } from "../index.js";
import { requireConsumerAuth } from "./internal_auth.js";
import { closeGenerationEvent, CredentialGenerationEventError, type CloseGenerationInput } from "./credential_generation_receipts.js";
import { validateLifecycleGeneration } from "./credential_generation.js";
import { readLegacyCredentialCoverage } from "./credential_legacy_coverage.js";
import { isCanonicalTenantUuid } from "./tenant_uuid.js";

export interface CloseGenerationRequest extends CloseGenerationInput {}
const MAX_BODY_BYTES = 16 * 1024;
const CLOSE_BUDGET = 256;

function reply(status: number, requestId: string, body?: unknown, coverage = "unknown"): Response {
  return new Response(body === undefined ? null : JSON.stringify(body), { status, headers: {
    "Content-Type": "application/json", "X-Request-Id": requestId, "x-corelink-legacy-coverage": coverage,
  } });
}

function parseBody(value: unknown): CloseGenerationRequest {
  if (value === null || typeof value !== "object" || Array.isArray(value)) throw new CredentialGenerationEventError("invalid", "invalid suspension event");
  const object = value as Record<string, unknown>;
  const keys = Object.keys(object).sort();
  if (keys.join(",") !== "event_id,lifecycle_generation,tenant_id") throw new CredentialGenerationEventError("invalid", "invalid suspension event");
  const eventId = object["event_id"];
  const tenantId = object["tenant_id"];
  const generation = object["lifecycle_generation"];
  if (typeof eventId !== "string" || eventId.length === 0 || new TextEncoder().encode(eventId).byteLength > 256 || eventId !== eventId.trim() || /[\u0000-\u001f\u007f]/u.test(eventId)) throw new CredentialGenerationEventError("invalid", "invalid suspension event");
  if (!isCanonicalTenantUuid(tenantId)) throw new CredentialGenerationEventError("invalid", "invalid suspension event");
  if (typeof generation !== "string") throw new CredentialGenerationEventError("invalid", "invalid suspension event");
  try { validateLifecycleGeneration(generation); } catch { throw new CredentialGenerationEventError("invalid", "invalid suspension event"); }
  return { event_id: eventId, tenant_id: tenantId, lifecycle_generation: generation };
}

async function readBoundedBody(request: Request): Promise<string> {
  const reader = request.body?.getReader();
  if (!reader) return "";
  const chunks: Uint8Array[] = [];
  let size = 0;
  try {
    for (;;) {
      const part = await reader.read();
      if (part.done) break;
      size += part.value.byteLength;
      if (size > MAX_BODY_BYTES) { await reader.cancel("request body too large"); throw new Error("oversize"); }
      chunks.push(part.value);
    }
  } finally { reader.releaseLock(); }
  const bytes = new Uint8Array(size);
  let offset = 0;
  for (const chunk of chunks) { bytes.set(chunk, offset); offset += chunk.byteLength; }
  return new TextDecoder().decode(bytes);
}

export async function handleRunnerCloseGeneration(request: Request, env: Env, requestId: string): Promise<Response> {
  if (typeof env.CORELINK_RUNNER_MINT_AUTH_KEY !== "string" || env.CORELINK_RUNNER_MINT_AUTH_KEY.length < 32) return reply(503, requestId, { error: "credential_generation_unavailable" });
  const denied = requireConsumerAuth(request, env, "runner_mint", requestId);
  if (denied) return denied;
  if (request.method !== "POST") return reply(405, requestId);
  let input: CloseGenerationRequest;
  try {
    const raw = await readBoundedBody(request);
    input = parseBody(JSON.parse(raw));
  } catch { return reply(400, requestId, { error: "invalid_suspension_event" }); }
  try {
    const metadataKv = (env as unknown as { METADATA_KV?: { delete(key: string): Promise<void> } }).METADATA_KV;
    const result = await closeGenerationEvent(env.CONFIG_DB, metadataKv, input, CLOSE_BUDGET);
    let coverage = "unknown";
    try { if ((await readLegacyCredentialCoverage(env.CONFIG_DB, input.tenant_id, input.lifecycle_generation)).complete) coverage = "verified"; } catch { /* fail closed */ }
    return reply(result.complete ? 200 : 202, requestId, { ...input, complete: result.complete }, coverage);
  } catch (error) {
    const code = error instanceof CredentialGenerationEventError ? error.code : "unavailable";
    if (code === "conflict") return reply(409, requestId, { error: "suspension_event_identity_conflict" });
    if (code === "invalid") return reply(400, requestId, { error: "invalid_suspension_event" });
    return reply(503, requestId, { error: "credential_generation_unavailable" });
  }
}
