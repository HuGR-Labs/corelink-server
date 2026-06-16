/**
 * Shared inbound internal-auth gate for Worker-hosted internal endpoints.
 *
 * The `/internal/v1/auth/*` Worker routes that githugr's backend calls
 * server-to-server (`tenant/lookup`, `token-exchange`) are gated by a
 * constant-time compare of the caller-supplied `X-Corelink-Internal-Auth`
 * header against the `CORELINK_INTERNAL_AUTH_KEY` binding — the SAME shared
 * secret the container's `/_internal/pat/mint` and the runners-fabric
 * `/internal/v1/auth/introspect` gates use. The browser never holds this
 * secret; only githugr's www server (a trusted backend) does.
 *
 * Posture (fail-CLOSED):
 *   - secret unbound / too short → endpoint unavailable (403). We never serve
 *     an internal endpoint without a properly sized gate.
 *   - header missing / wrong → 401.
 *   - match → `null` (caller proceeds).
 *
 * The compare mirrors index.ts `ctEqStr` exactly (the established Worker
 * constant-time pattern). It is replicated locally — like `reapiError` is in
 * the sibling lib modules — to avoid a runtime import cycle (index.ts ⇄ lib/*).
 */

import type { Env } from "../index.js";

/** HTTP header carrying the shared internal-auth secret. */
const INTERNAL_AUTH_HEADER = "x-corelink-internal-auth";

/**
 * Minimum length (chars) of the internal-auth secret. Mirrors the container's
 * `build_state_from_env` floor (`openssl rand -hex 32` → 64 chars; the floor is
 * 32). A shorter/absent secret fails the endpoint CLOSED.
 */
const MIN_INTERNAL_AUTH_KEY_LEN = 32;

/**
 * Constant-time string compare — byte-for-byte identical to index.ts
 * `ctEqStr`. Length is compared first (the secret length is a fixed,
 * non-sensitive 64-hex constant, consistent with the existing Worker gate).
 */
function ctEqStr(a: string, b: string): boolean {
  if (a.length !== b.length) {
    return false;
  }
  let diff = 0;
  for (let i = 0; i < a.length; i++) {
    diff |= a.charCodeAt(i) ^ b.charCodeAt(i);
  }
  return diff === 0;
}

/**
 * REAPI error envelope builder — local mirror (avoids the index.ts ⇄ lib import
 * cycle, same as the sibling lib modules). Shape is load-bearing:
 * `{ error, message, request_id }` + `X-Request-Id` header.
 */
function reapiError(error: string, message: string, status: number, requestId: string): Response {
  const body = { error, message, request_id: requestId };
  return new Response(JSON.stringify(body), {
    status,
    headers: {
      "Content-Type": "application/json",
      "X-Request-Id": requestId,
    },
  });
}

/**
 * Gate an inbound request on the shared internal-auth secret.
 *
 * @returns `null` when the caller is authorized (proceed); otherwise a
 *   fail-CLOSED error {@link Response} (403 if the secret is unbound, 401 if the
 *   header is missing/wrong) that the caller must return verbatim.
 */
export function requireInternalAuth(request: Request, env: Env, requestId: string): Response | null {
  const expected = env.CORELINK_INTERNAL_AUTH_KEY;
  if (!expected || expected.length < MIN_INTERNAL_AUTH_KEY_LEN) {
    // No properly sized secret bound → the internal endpoint is unavailable.
    // Fail CLOSED (never an open gate); do not reveal which precondition failed.
    return reapiError("FORBIDDEN", "internal endpoint unavailable", 403, requestId);
  }
  const provided = request.headers.get(INTERNAL_AUTH_HEADER) ?? "";
  if (!ctEqStr(expected, provided)) {
    return reapiError("UNAUTHORIZED", "internal auth required", 401, requestId);
  }
  return null;
}
