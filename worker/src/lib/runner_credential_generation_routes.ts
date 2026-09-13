import type { Env } from "../index.js";
import { requireConsumerAuth } from "./internal_auth.js";
import {
  closeGenerationEvent,
  CredentialGenerationEventError,
  type CloseGenerationInput,
} from "./credential_generation_receipts.js";
import { validateLifecycleGeneration } from "./credential_generation.js";

export interface CloseGenerationRequest extends CloseGenerationInput {}

const TENANT_ID = /^(?!00000000-0000-0000-0000-000000000000$)[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;
const MAX_BODY_BYTES = 16 * 1024;
const CLOSE_BUDGET = 256;

function reply(status: number, requestId: string, body?: unknown): Response {
  return new Response(body === undefined ? null : JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json", "X-Request-Id": requestId },
  });
}

function parseBody(value: unknown): CloseGenerationRequest {
  if (value === null || typeof value !== "object" || Array.isArray(value)) throw new CredentialGenerationEventError("invalid", "invalid suspension event");
  const object = value as Record<string, unknown>;
  const keys = Object.keys(object).sort();
  if (keys.length !== 3 || keys[0] !== "event_id" || keys[1] !== "lifecycle_generation" || keys[2] !== "tenant_id") throw new CredentialGenerationEventError("invalid", "invalid suspension event");
  const eventId = object["event_id"];
  const tenantId = object["tenant_id"];
  const generation = object["lifecycle_generation"];
  if (typeof eventId !== "string" || eventId.length === 0 || eventId.length > 256 || eventId !== eventId.trim() || /[\u0000-\u001f\u007f]/u.test(eventId)) throw new CredentialGenerationEventError("invalid", "invalid suspension event");
  if (typeof tenantId !== "string" || !TENANT_ID.test(tenantId) || tenantId !== tenantId.toLowerCase()) throw new CredentialGenerationEventError("invalid", "invalid suspension event");
  if (typeof generation !== "string") throw new CredentialGenerationEventError("invalid", "invalid suspension event");
  try { validateLifecycleGeneration(generation); } catch { throw new CredentialGenerationEventError("invalid", "invalid suspension event"); }
  return { event_id: eventId, tenant_id: tenantId, lifecycle_generation: generation };
}

export async function handleRunnerCloseGeneration(request: Request, env: Env, requestId: string): Promise<Response> {
  // This endpoint is runner-dispatcher-only.  Do not let the helper's
  // compatibility fallback widen this closure authority to the shared key.
  if (typeof env.CORELINK_RUNNER_MINT_AUTH_KEY !== "string" || env.CORELINK_RUNNER_MINT_AUTH_KEY.length < 32) {
    return reply(503, requestId, { error: "credential_generation_unavailable" });
  }
  const denied = requireConsumerAuth(request, env, "runner_mint", requestId);
  if (denied) return denied;
  if (request.method !== "POST") return reply(405, requestId);
  let input: CloseGenerationRequest;
  try {
    const raw = await request.text();
    if (new TextEncoder().encode(raw).byteLength > MAX_BODY_BYTES) throw new Error("oversize");
    input = parseBody(JSON.parse(raw));
  } catch (error) {
    if (error instanceof CredentialGenerationEventError && error.code !== "invalid") throw error;
    return reply(400, requestId, { error: "invalid_suspension_event" });
  }
  try {
    const result = await closeGenerationEvent(env.CONFIG_DB, env.METADATA_KV, input, CLOSE_BUDGET);
    return reply(result.complete ? 200 : 202, requestId, { ...input, complete: result.complete });
  } catch (error) {
    const code = error instanceof CredentialGenerationEventError ? error.code : "unavailable";
    if (code === "conflict") return reply(409, requestId, { error: "suspension_event_identity_conflict" });
    if (code === "invalid") return reply(400, requestId, { error: "invalid_suspension_event" });
    return reply(503, requestId, { error: "credential_generation_unavailable" });
  }
}
