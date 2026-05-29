/**
 * Update Clerk user public metadata via the Clerk Backend API.
 *
 * Used by the signup-worker to write `tenant_id`, `region`, and the
 * one-time `pat_plaintext` into the user's public metadata so the
 * `/welcome` page can read them from the session claims.
 *
 * Reference: https://clerk.com/docs/reference/backend-api/tag/Users#operation/UpdateUser
 *
 * CTRL-CRED-001: `pat_plaintext` is written ONCE at signup. It is exposed
 * to the user via the /welcome page which reads `session.sessionClaims.pat_plaintext`.
 * Clerk encrypts metadata at rest. The value is cleared from public_metadata
 * by a follow-up scheduled action after first session (deferred to Phase-1;
 * acceptable per CTRL-CRED-001 comment in welcome/page.tsx).
 *
 * NEVER log the metadata object or any field of it that contains `pat_plaintext`.
 */

/** Environment bindings required by the Clerk metadata client. */
export interface ClerkMetadataEnv {
  /**
   * Clerk Backend API secret key (sk_test_... or sk_live_...).
   * Bound via `wrangler secret put CLERK_SECRET_KEY`.
   */
  CLERK_SECRET_KEY: string;
}

/**
 * Public metadata shape written by the signup-worker.
 * These fields are readable from the Clerk session claims on the client.
 */
export interface UserPublicMetadata {
  tenant_id: string;
  region: string;
  /** One-time PAT plaintext. NEVER log. */
  pat_plaintext: string;
}

/**
 * Update a Clerk user's public metadata via the Clerk Backend API.
 *
 * Uses `PATCH /v1/users/{userId}` with `public_metadata` merge semantics —
 * existing keys not present in this update are preserved.
 *
 * Throws on non-200 response.
 */
export async function updateClerkUserMetadata(
  env: ClerkMetadataEnv,
  userId: string,
  metadata: UserPublicMetadata,
): Promise<void> {
  const url = `https://api.clerk.com/v1/users/${encodeURIComponent(userId)}`;
  const resp = await fetch(url, {
    method: "PATCH",
    headers: {
      authorization: `Bearer ${env.CLERK_SECRET_KEY}`,
      "content-type": "application/json",
    },
    // Clerk PATCH /v1/users merges `public_metadata` with existing keys.
    body: JSON.stringify({ public_metadata: metadata }),
  });

  if (!resp.ok) {
    // Do NOT include response body — it may contain user PII.
    throw new Error(`clerk_metadata_update_failed_${resp.status}`);
  }
}
