/** Public, health, and internal route domain. */
import type { ExecutionContext } from "@cloudflare/workers-types";
import type { Env } from "./index_common.js";
import { handlePreflight } from "./index_auth.js";
import { type RouteMatch } from "./route_match.js";
import { handlePublicHealthRoute } from "./index_public_health.js";
import { handlePublicInternalRoute } from "./index_public_internal.js";

export async function handlePublicAndInternalRoute(
  request: Request,
  env: Env,
  ctx: ExecutionContext,
  requestId: string,
  route: RouteMatch,
): Promise<Response | null> {
  const preflight = handlePreflight(request);
  if (preflight !== null) return preflight;
  const publicResponse = await handlePublicHealthRoute(request, env, requestId, route);
  if (publicResponse !== null) return publicResponse;
  const internalResponse = await handlePublicInternalRoute(request, env, ctx, requestId, route);
  if (internalResponse !== null) return internalResponse;
  return null;
}
