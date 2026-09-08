/** Clerk, exchange, runner, OCI, and customer route domain. */
import type { ExecutionContext } from "@cloudflare/workers-types";
import type { Env } from "./index_common.js";
import type { RouteMatch } from "./route_match.js";
import { handleSpecialOnboardingRoute } from "./index_special_onboarding.js";
import { handleSpecialMiscRoute } from "./index_special_misc.js";
import { handleSpecialPassThroughRoute } from "./index_special_passthrough.js";
import { handleSpecialCustomerRoute } from "./index_special_customer.js";

export async function handleSpecialRoute(
  request: Request,
  env: Env,
  ctx: ExecutionContext,
  requestId: string,
  requestStart: number,
  requestCounter: number,
  route: RouteMatch,
): Promise<Response | null> {
  const onboarding = await handleSpecialOnboardingRoute(request, env, ctx, requestId, requestStart, requestCounter, route);
  if (onboarding !== null) return onboarding;
  const misc = await handleSpecialMiscRoute(request, env, requestId, requestStart, requestCounter, route);
  if (misc !== null) return misc;
  const passthrough = await handleSpecialPassThroughRoute(request, env, ctx, requestId, route);
  if (passthrough !== null) return passthrough;
  const customer = await handleSpecialCustomerRoute(request, env, requestId, route);
  if (customer !== null) return customer;
  return null;
}
