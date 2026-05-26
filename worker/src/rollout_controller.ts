/**
 * RolloutController Durable Object — rollout-state manager.
 *
 * This DO tracks canary rollout state per rollout_id (WI-S13-*).
 * It is declared in wrangler.toml under [[durable_objects.bindings]]
 * and must be exported from the Worker entrypoint.
 *
 * The full implementation of rollout logic lives in
 * `crates/corelink-cf-bindings` (Rust, compiled to WASM via corelink-worker).
 * This TypeScript shim satisfies the Worker bundler requirement that every
 * DO class referenced in wrangler.toml is exported from index.ts.
 *
 * Phase C will wire this DO to the Rust implementation once the WASM
 * bridge is ready. For now it implements a no-op that returns 200 for
 * health checks and 501 for all other requests.
 */

import type { DurableObject, DurableObjectState } from "@cloudflare/workers-types";
import type { Env } from "./index.js";

export class RolloutController implements DurableObject {
  private readonly state: DurableObjectState;
  private readonly env: Env;

  constructor(state: DurableObjectState, env: Env) {
    this.state = state;
    this.env = env;
  }

  async fetch(request: Request): Promise<Response> {
    const requestId = request.headers.get("x-request-id") ?? crypto.randomUUID();
    const url = new URL(request.url);

    if (url.pathname === "/_do/health") {
      return new Response(
        JSON.stringify({ status: "ok", class: "RolloutController", request_id: requestId }),
        {
          status: 200,
          headers: {
            "Content-Type": "application/json",
            "X-Request-Id": requestId,
          },
        },
      );
    }

    // All other requests: not yet implemented (Phase C wires WASM)
    return new Response(
      JSON.stringify({
        error: "NOT_IMPLEMENTED",
        message: "RolloutController WASM bridge not yet wired (Phase C)",
        request_id: requestId,
      }),
      {
        status: 501,
        headers: {
          "Content-Type": "application/json",
          "X-Request-Id": requestId,
        },
      },
    );
  }
}
