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
 * The compare mirrors the padded `crypto.subtle.timingSafeEqual` gate index.ts
 * uses for `/_internal/*` (no length oracle; CAA-360 #27). It is replicated
 * locally — like `reapiError` is in the sibling lib modules — to avoid a
 * runtime import cycle (index.ts ⇄ lib/*).
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
 * Internal-auth CONSUMERS — the per-surface key-split axis (red-team #3).
 *
 * Mirrors the container's just-merged Rust split: each distinct internal
 * surface authenticates with its OWN key so that leaking one consumer's secret
 * does not unlock every `/_internal/*` route. Each consumer key FALLS BACK to
 * the shared `CORELINK_INTERNAL_AUTH_KEY` when its dedicated key is unset/short,
 * so the split can be rolled out per-consumer without a flag-day.
 *
 * FROZEN env names (identical to the Rust control-plane side):
 *   - pat_mint → CORELINK_PAT_MINT_AUTH_KEY (`/_internal/pat/mint`)
 *   - admin    → CORELINK_ADMIN_AUTH_KEY    (admin `/_internal/*`)
 *   - erase    → CORELINK_ERASE_AUTH_KEY    (erase `/_internal/*`)
 */
export type InternalConsumer = "pat_mint" | "admin" | "erase";

/**
 * Resolve the internal-auth key to verify against for a given consumer.
 *
 * Selection (matches the Rust contract exactly):
 *   1. the consumer-specific key  iff set AND length >= MIN_INTERNAL_AUTH_KEY_LEN;
 *   2. else the shared key        iff length >= MIN_INTERNAL_AUTH_KEY_LEN;
 *   3. else `null` → the caller MUST fail CLOSED (no properly sized gate bound).
 *
 * A too-short dedicated key is treated as ABSENT (falls through to the shared
 * key) rather than weakening the gate — never trust a sub-floor secret.
 *
 * @returns the chosen key string, or `null` when neither qualifies (fail-CLOSED).
 */
export function resolveConsumerKey(env: Env, consumer: InternalConsumer): string | null {
  const specific =
    consumer === "pat_mint"
      ? env.CORELINK_PAT_MINT_AUTH_KEY
      : consumer === "admin"
        ? env.CORELINK_ADMIN_AUTH_KEY
        : env.CORELINK_ERASE_AUTH_KEY;
  if (specific && specific.length >= MIN_INTERNAL_AUTH_KEY_LEN) {
    return specific;
  }
  const shared = env.CORELINK_INTERNAL_AUTH_KEY;
  if (shared && shared.length >= MIN_INTERNAL_AUTH_KEY_LEN) {
    return shared;
  }
  return null;
}

/**
 * Constant-time secret compare WITHOUT a length oracle (CAA-360 #27).
 *
 * The prior `ctEqStr` returned early on `a.length !== b.length`, taking a
 * timing path that depended on the provided length — a (weak) length oracle.
 * This mirrors the padded `crypto.subtle.timingSafeEqual` gate that
 * `index.ts` uses for `/_internal/*`: copy the provided bytes into a fixed
 * buffer sized to the EXPECTED length (zero-pad short input / truncate long
 * input), run exactly ONE `timingSafeEqual` over equal-length buffers, then AND
 * with a single length-equality bit. No branch depends on the provided length,
 * and a wrong length that happens to share the expected prefix is still
 * rejected by the length bit.
 *
 * The caller guarantees `expected` is non-empty (≥ MIN_INTERNAL_AUTH_KEY_LEN).
 */
function constantTimeSecretEqual(expected: string, provided: string): boolean {
  const enc = new TextEncoder();
  const expectedBytes = enc.encode(expected);
  const providedBytes = enc.encode(provided);
  const fixed = new Uint8Array(expectedBytes.length);
  const copyLen =
    providedBytes.length < expectedBytes.length
      ? providedBytes.length
      : expectedBytes.length;
  fixed.set(providedBytes.subarray(0, copyLen));
  const bytesEqual = crypto.subtle.timingSafeEqual(fixed, expectedBytes);
  // Single integer compare (not a per-char path) → no length oracle; a length
  // mismatch can never authenticate even if the prefix bytes match.
  const lenEqual = providedBytes.length === expectedBytes.length;
  return bytesEqual && lenEqual;
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
  if (!constantTimeSecretEqual(expected, provided)) {
    return reapiError("UNAUTHORIZED", "internal auth required", 401, requestId);
  }
  return null;
}

/**
 * Gate an inbound request on a SPECIFIC internal-auth consumer's key.
 *
 * Resolves the consumer's dedicated key via {@link resolveConsumerKey} (the
 * #297 per-consumer-key + shared-fallback pattern): the dedicated key if set and
 * properly sized, else the shared `CORELINK_INTERNAL_AUTH_KEY`, else `null`. A
 * `null` resolution means NO properly sized gate is bound → fail CLOSED (403).
 *
 * Used by the D-9 runner-mint / runner-revoke routes (`pat_mint` consumer →
 * `CORELINK_PAT_MINT_AUTH_KEY` with shared fallback), mirroring the container's
 * `/_internal/pat/mint` per-consumer gate so leaking one surface's secret does
 * not unlock the rest of the `/internal/*` family.
 *
 * @returns `null` when the caller is authorized (proceed); otherwise a
 *   fail-CLOSED error {@link Response} (403 if no sized key is bound, 401 if the
 *   header is missing/wrong) that the caller must return verbatim.
 */
export function requireConsumerAuth(
  request: Request,
  env: Env,
  consumer: InternalConsumer,
  requestId: string,
): Response | null {
  const expected = resolveConsumerKey(env, consumer);
  if (expected === null) {
    // Neither the dedicated nor the shared key qualifies → unavailable.
    // Fail CLOSED; do not reveal which precondition failed.
    return reapiError("FORBIDDEN", "internal endpoint unavailable", 403, requestId);
  }
  const provided = request.headers.get(INTERNAL_AUTH_HEADER) ?? "";
  if (!constantTimeSecretEqual(expected, provided)) {
    return reapiError("UNAUTHORIZED", "internal auth required", 401, requestId);
  }
  return null;
}
