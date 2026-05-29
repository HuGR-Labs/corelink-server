/**
 * Typed HTTP client for the CoreLink container's internal PAT mint endpoint.
 *
 * Calls `POST /_internal/pat/mint` on the container via the Worker Durable
 * Object proxy. The request is authenticated with the `X-Corelink-Internal-Auth`
 * shared secret.
 *
 * NEVER log the `token_plaintext` field of the response (CTRL-CRED-001).
 */

/** Parameters for the `/_internal/pat/mint` request. */
export interface InternalMintParams {
  tenantId: string;
  principalId: string;
  /** "admin" | "cas:rw" */
  scopes: string;
  /** TTL in seconds (e.g. 365 * 86400 = 31536000). */
  ttlSeconds: number;
}

/** Response from `/_internal/pat/mint`. */
export interface InternalMintResult {
  /** PAT plaintext — NEVER log or persist. Surface to Clerk metadata ONCE. */
  token_plaintext: string;
  /** UUID of the new PAT row (`pat.pat_id`). */
  pat_id: string;
  /** 16-char Crockford b32 lookup key (`pat.token_id`). */
  token_id: string;
  /** Expiry epoch milliseconds (`pat.expires_ms`). */
  expires_ms: number;
  /** Argon2id PHC hash string (`pat.pat_hash`). */
  hash: string;
}

/** Environment bindings required by the internal mint client. */
export interface InternalMintEnv {
  /** Base URL of the CoreLink API (e.g. "https://corelink-api.humangr.com"). */
  CORELINK_API_BASE: string;
  /** Shared secret for `X-Corelink-Internal-Auth`. */
  CORELINK_INTERNAL_AUTH_KEY: string;
}

/**
 * Mint a PAT via the container's internal endpoint.
 *
 * Throws on non-200 response or network error. The caller is responsible
 * for not logging the `token_plaintext` field of the returned object.
 */
export async function mintPatInternal(
  env: InternalMintEnv,
  params: InternalMintParams,
): Promise<InternalMintResult> {
  const url = `${env.CORELINK_API_BASE}/_internal/pat/mint`;
  const body = JSON.stringify({
    tenant_id: params.tenantId,
    principal_id: params.principalId,
    scopes: params.scopes,
    ttl_seconds: params.ttlSeconds,
  });

  const resp = await fetch(url, {
    method: "POST",
    headers: {
      "content-type": "application/json",
      "x-corelink-internal-auth": env.CORELINK_INTERNAL_AUTH_KEY,
    },
    body,
  });

  if (!resp.ok) {
    // Never include the response body in the error message — it may contain
    // diagnostic info from the container that we don't want in logs.
    throw new Error(`internal_pat_mint_failed_${resp.status}`);
  }

  return (await resp.json()) as InternalMintResult;
}
