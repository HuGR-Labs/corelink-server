import { bounded } from "./devenv_cleanup.js";
import type { AuthorizedDevenvInput, AuthorizedDevenvAck } from "../types/devenv_rpc.js";

export const DEVENV_MAX_TTL_SECONDS = 8 * 60 * 60;
const UUID = /^(?!00000000-0000-0000-0000-000000000000$)[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/i;
const BASES = ["/v1/customer/devenv", "/v1/devenv"];
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
  prepareCompute(sessionUuid: string, tier: string): Promise<string | null>;
  abandonCompute(reservationId: string): Promise<void>;
  mint(operationId: string): Promise<Response>;
  revoke(operationId: string): Promise<boolean>;
  adopt(operationId: string, patId: string): Promise<boolean>;
  start(input: AuthorizedDevenvInput): Promise<AuthorizedDevenvAck>;
  now(): number;
  sessionId(): string;
  cleanupFailed(): void;
}
function failure(status: number): Response { return Response.json({ error: status === 400 ? "BAD_REQUEST" : "SERVICE_UNAVAILABLE", message: status === 400 ? "invalid DevEnv start configuration" : "DevEnv start unavailable" }, { status }); }
function record(value: unknown): value is Record<string, unknown> { return value !== null && typeof value === "object" && !Array.isArray(value); }
function name(value: unknown): value is string { return typeof value === "string" && /^[a-zA-Z0-9_-]{1,128}$/.test(value); }

/** Called only after verified identity, write authorization and start quota. */
export async function relayAuthorizedDevenvStart(request: Request, tenantId: string, deps: RelayDeps): Promise<Response> {
  let body: unknown; try { body = await bounded(request.json()); } catch { return failure(400); }
  if (!record(body) || Object.keys(body).some(key => !["workspace_name", "profile_name", "tier"].includes(key))) return failure(400);
  const workspaceName = body["workspace_name"], profileName = body["profile_name"] ?? "default", tier = body["tier"] ?? "standard-4";
  if (!name(workspaceName) || !name(profileName) || typeof tier !== "string" || !["standard-2", "standard-4", "power-8", "ultra-16"].includes(tier)) return failure(400);
  const deadline = deps.now() + DEVENV_MAX_TTL_SECONDS * 1000, sessionUuid = deps.sessionId();
  if (!UUID.test(sessionUuid) || !UUID.test(tenantId)) return failure(503);
  let patId: string | null = null, armed = false, accepted = false, computeReservationId: string | null = null;
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
    computeReservationId = await bounded(deps.prepareCompute(sessionUuid, tier));
    if (computeReservationId !== null && computeReservationId !== sessionUuid) return failure(503);
    const ack = await bounded(deps.start({ config: { workspaceName, profileName, tier }, grant: { tenantId, sessionUuid, casPat: token, patId, expiresAtMs: Math.min(expires, deadline), lifecycleGeneration: deps.lifecycleGeneration, ...(computeReservationId !== null ? { computeReservationId } : {}) } }), 20_000);
    if (!ack || ack.sessionUuid !== sessionUuid || !["starting", "running"].includes(ack.status)) return failure(503);
    accepted = await bounded(deps.adopt(sessionUuid, patId)); if (!accepted) return failure(503);
    return Response.json({ sessionUuid, status: ack.status, lifecycle_generation: deps.lifecycleGeneration }, { status: 201 });
  } catch { return failure(503); }
  finally {
    if (!accepted && computeReservationId !== null) { try { await bounded(deps.abandonCompute(computeReservationId)); } catch {} }
    if (armed && !accepted) { let revoked = false; for (let attempt = 0; attempt < 2 && !revoked; attempt++) { try { revoked = await bounded(deps.revoke(sessionUuid)); } catch {} } if (!revoked) deps.cleanupFailed(); }
  }
}
