/**
 * Update Clerk user metadata via the Clerk Backend API.
 *
 * Used by the signup-worker to write `tenant_id` + `region` into the user's
 * PUBLIC metadata (legit session claims the app/githugr read from the session
 * JWT) and the one-time `pat_plaintext` into the user's PRIVATE metadata
 * (backend-only) so the `/welcome` page can reveal it once via a server-side
 * Clerk Backend API read.
 *
 * Reference: https://clerk.com/docs/reference/backend-api/tag/Users#operation/UpdateUser
 *
 * CTRL-CRED-001: the `pat_plaintext` is written ONCE at signup. It lives in
 * Clerk **`private_metadata`**, which is:
 *   - NEVER embedded in the session JWT (only `public_metadata` rides claims),
 *   - NEVER readable by the client (`useUser()` / the Frontend API only sees
 *     `public_metadata`).
 * The value is cleared two ways: (1) the client-driven `/welcome` reveal PATCHes
 * `private_metadata.pat_plaintext` to null after showing it once, and (2) a
 * guaranteed server-side scrub cron (`pat_scrub_cron.ts`) clears it for any user
 * whose reveal is older than a short TTL — so an un-visited `/welcome` cannot
 * leave the secret resident. `pat_revealed_at` (epoch-ms) is co-written so the
 * cron can compute the reveal age.
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
 * These fields are readable from the Clerk session claims on the client and
 * are embedded in the session JWT — so they MUST NOT carry any secret.
 */
export interface UserPublicMetadata {
  tenant_id: string;
  region: string;
}

/**
 * Private metadata shape written by the signup-worker. Clerk `private_metadata`
 * is backend-only: never in the JWT, never readable by `useUser()`.
 */
export interface UserPrivateMetadata {
  /** One-time PAT plaintext. NEVER log. Cleared by /welcome + the scrub cron. */
  pat_plaintext: string;
  /** Epoch-ms when the PAT was written (reveal-age clock for the scrub cron). */
  pat_revealed_at: number;
}

/**
 * Update a Clerk user's public + private metadata via the Clerk Backend API.
 *
 * Uses `PATCH /v1/users/{userId}` with merge semantics — existing keys not
 * present in this update are preserved. `public_metadata` gets only the legit
 * session claims (`tenant_id`, `region`); the secret `pat_plaintext` goes to
 * `private_metadata` (backend-only, never in the JWT / client).
 *
 * Throws on non-200 response.
 */
export async function updateClerkUserMetadata(
  env: ClerkMetadataEnv,
  userId: string,
  publicMetadata: UserPublicMetadata,
  privateMetadata: UserPrivateMetadata,
): Promise<void> {
  const url = `https://api.clerk.com/v1/users/${encodeURIComponent(userId)}`;
  const resp = await fetch(url, {
    method: "PATCH",
    headers: {
      authorization: `Bearer ${env.CLERK_SECRET_KEY}`,
      "content-type": "application/json",
    },
    // Clerk PATCH /v1/users merges each metadata object with existing keys.
    // NEVER log this body — `private_metadata.pat_plaintext` is a secret.
    body: JSON.stringify({
      public_metadata: publicMetadata,
      private_metadata: privateMetadata,
    }),
  });

  if (!resp.ok) {
    // Do NOT include response body — it may contain user PII.
    throw new Error(`clerk_metadata_update_failed_${resp.status}`);
  }
}
