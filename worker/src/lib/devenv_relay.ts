import { bounded } from "./devenv_cleanup.js";
import type { AuthorizedDevenvInput, AuthorizedDevenvAck, PreparedDevenvCompute } from "../types/devenv_rpc.js";

export const DEVENV_MAX_TTL_SECONDS = 8 * 60 * 60;
const UUID = /^(?!00000000-0000-0000-0000-000000000000$)[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const BASES = ["/v1/customer/devenv", "/v1/devenv"];
const MAX_START_BODY_BYTES = 4 * 1024;
const GENERATION = /^(0|[1-9][0-9]*)$/;
const MAX_GENERATION = 9223372036854775807n;
export function devenvPath(path: string): string | null { for (const base of BASES) if (path === base || path.startsWith(`${base}/`)) return path.slice(base.length).replace(/\/+$/, ""); return null; }
export function isDevenvStart(method: string, path: string): boolean { return method === "POST" && devenvPath(path) === ""; }
export function mayAccessDevenv(request: Request, scope: string): boolean {
  if (scope === "admin" || scope === "read-write") return true;
  if (scope !== "read-only" || request.headers.get("upgrade")?.toLowerCase() === "websocket") return false;
  const path = devenvPath(new URL(request.url).pathname);
  return (request.method === "GET" || request.method === "HEAD") && (path === "" || path === "/status");
}
interface RelayDeps {
  lifecycleGeneration: string;
  prepare(operationId: string, tenantId: string, lifecycleGeneration: string): Promise<boolean>;
  prepareCompute(sessionUuid: string, tier: string): Promise<PreparedDevenvCompute | null>;
  abandonCompute(reservationId: string): Promise<void>;
  mint(operationId: string): Promise<Response>;
  revoke(operationId: string): Promise<boolean>;
  adopt(operationId: string, patId: string): Promise<boolean>;
  start(input: AuthorizedDevenvInput): Promise<AuthorizedDevenvAck>;
  stop(input: { tenantId: string; sessionUuid: string }): Promise<{ sessionUuid: string; status: "stopped" | "already_stopped" | "not_current" }>;
  now(): number;
  sessionId(): string;
  cleanupFailed(): void;
}
function failure(status: number): Response { return Response.json({ error: status === 400 ? "BAD_REQUEST" : "SERVICE_UNAVAILABLE", message: status === 400 ? "invalid DevEnv start configuration" : "DevEnv start unavailable" }, { status }); }
function record(value: unknown): value is Record<string, unknown> { return value !== null && typeof value === "object" && !Array.isArray(value); }
function name(value: unknown): value is string { return typeof value === "string" && /^[a-zA-Z0-9_-]{1,128}$/.test(value); }

/** Called only after verified identity, write authorization and start quota. */
export async function relayAuthorizedDevenvStart(request: Request, tenantId: string, deps: RelayDeps): Promise<Response> {
  const contentLength = request.headers.get("content-length");
  if (contentLength !== null && (!/^(0|[1-9][0-9]*)$/.test(contentLength) || Number(contentLength) > MAX_START_BODY_BYTES)) return failure(400);
  if (!GENERATION.test(deps.lifecycleGeneration) || deps.lifecycleGeneration.length > 19 || BigInt(deps.lifecycleGeneration) > MAX_GENERATION) return failure(503);
  let body: unknown;
  try {
    const reader = request.body?.getReader();
    if (!reader) throw new Error("missing body");
    const chunks: Uint8Array[] = []; let bytes = 0;
    try {
      for (;;) {
        const part = await reader.read();
        if (part.done) break;
        bytes += part.value.byteLength;
        if (bytes > MAX_START_BODY_BYTES) throw new Error("body too large");
        chunks.push(part.value);
      }
    } finally { reader.releaseLock(); }
    const raw = new Uint8Array(bytes); let offset = 0;
    for (const chunk of chunks) { raw.set(chunk, offset); offset += chunk.byteLength; }
    body = JSON.parse(new TextDecoder().decode(raw));
  } catch { return failure(400); }
  if (!record(body) || Object.keys(body).some(key => !["workspace_name", "profile_name", "tier"].includes(key))) return failure(400);
  const workspaceName = body["workspace_name"], profileName = body["profile_name"] ?? "default", tier = body["tier"] ?? "standard-4";
  if (!name(workspaceName) || !name(profileName) || typeof tier !== "string" || !["standard-2", "standard-4", "power-8", "ultra-16"].includes(tier)) return failure(400);
  const deadline = deps.now() + DEVENV_MAX_TTL_SECONDS * 1000, sessionUuid = deps.sessionId();
  if (!UUID.test(sessionUuid) || !UUID.test(tenantId)) return failure(503);
  let patId: string | null = null, armed = false, accepted = false, startAttempted = false, compute: PreparedDevenvCompute | null = null;
  try {
    // The obligation is armed before minting; compute is staged only after the token exists.
    armed = await bounded(deps.prepare(sessionUuid, tenantId, deps.lifecycleGeneration));
    if (!armed) return failure(503);
    const minted = await bounded(deps.mint(sessionUuid), 25_000);
    if (!minted.ok) return failure(503);
    const issued: unknown = await bounded(minted.json()); if (!record(issued)) return failure(503);
    const id = issued["pat_id"], token = issued["token_plaintext"], expires = issued["expires_ms"];
    if (typeof id === "string" && id.trim().length > 0 && id.length <= 256) patId = id;
    const now = deps.now();
    if (patId === null || !UUID.test(patId) || typeof token !== "string" || token.trim().length === 0 || token.length > 4096 || issued["tenant"] !== tenantId || typeof expires !== "number" || !Number.isSafeInteger(expires) || expires <= now || expires > now + DEVENV_MAX_TTL_SECONDS * 1000 || deadline <= now) return failure(503);
    compute = await bounded(deps.prepareCompute(sessionUuid, tier));
    if (compute !== null && (compute.reservationId !== sessionUuid || !Number.isSafeInteger(compute.maximumWallMs) || compute.maximumWallMs < 1 || compute.maximumWallMs > DEVENV_MAX_TTL_SECONDS * 1000)) return failure(503);
    const relayNow = deps.now();
    const expiresAtMs = compute === null ? Math.min(expires, deadline) : Math.min(expires, relayNow + compute.maximumWallMs);
    if (!Number.isSafeInteger(relayNow) || !Number.isSafeInteger(expiresAtMs) || expiresAtMs <= relayNow) return failure(503);
    startAttempted = true;
    const ack = await bounded(deps.start({ config: { workspaceName, profileName, tier }, grant: { tenantId, sessionUuid, casPat: token, patId, expiresAtMs, lifecycleGeneration: deps.lifecycleGeneration, ...(compute !== null ? { computeReservationId: compute.reservationId, maximumWallMs: compute.maximumWallMs } : {}) } }), 20_000);
    if (!ack || ack.sessionUuid !== sessionUuid || !["starting", "running"].includes(ack.status)) return failure(503);
    accepted = await bounded(deps.adopt(sessionUuid, patId)); if (!accepted) return failure(503);
    return Response.json({ sessionUuid, status: ack.status, lifecycle_generation: deps.lifecycleGeneration }, { status: 201 });
  } catch { return failure(503); }
  finally {
    if (startAttempted && !accepted) {
      try {
        const stopped = await bounded(deps.stop({ tenantId, sessionUuid }), 20_000);
        if (!stopped || stopped.sessionUuid !== sessionUuid || !["stopped", "already_stopped", "not_current"].includes(stopped.status)) throw new Error("invalid stop acknowledgement");
      } catch { console.error("devenv_cleanup_pending"); }
    }
    if (!accepted && compute !== null) { try { await bounded(deps.abandonCompute(compute.reservationId)); } catch {} }
    if (armed && !accepted) { let revoked = false; for (let attempt = 0; attempt < 2 && !revoked; attempt++) { try { revoked = await bounded(deps.revoke(sessionUuid)); } catch {} } if (!revoked) deps.cleanupFailed(); }
  }
}
