/** Read-only runner authorization probe used before capacity reservation. */

import type { Env } from "../index.js";
import { handleRunnerMint } from "./runner_mint.js";

/** Reuses the runner-mint authorization chokepoint without minting/throttling. */
export async function handleRunnerAuthorize(
  request: Request,
  env: Env,
  requestId: string,
): Promise<Response> {
  return handleRunnerMint(request, env, requestId, true);
}
