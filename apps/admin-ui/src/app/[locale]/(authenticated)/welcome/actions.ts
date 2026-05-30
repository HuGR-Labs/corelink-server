"use server";

import { redirect } from "next/navigation";
import { revalidatePath } from "next/cache";

/**
 * Clerk user public metadata shape after the signup-worker writes it.
 * `pat_plaintext` is present on first session only; we drop it here.
 */
interface ClerkPublicMetadata {
  tenant_id?: string;
  region?: string;
  // pat_plaintext intentionally absent — we're clearing it
}

/**
 * Resolve the currently authenticated Clerk user's ID and public metadata.
 *
 * Uses `@clerk/nextjs/server` lazy import so test environments without Clerk
 * wiring don't crash. Throws if there is no active session.
 */
async function resolveAuthContext(): Promise<{
  userId: string;
  publicMetadata: ClerkPublicMetadata;
}> {
  const mod = await import("@clerk/nextjs/server").catch(() => null);
  if (!mod) throw new Error("auth_provider_not_initialized");

  const clerkMod = mod as {
    auth: () => Promise<{
      userId: string | null;
      sessionClaims?: {
        publicMetadata?: ClerkPublicMetadata;
      };
    }>;
  };

  const session = await clerkMod.auth();
  if (!session.userId) throw new Error("no_active_session");

  return {
    userId: session.userId,
    publicMetadata: session.sessionClaims?.publicMetadata ?? {},
  };
}

/**
 * Call the Clerk Backend API to remove `pat_plaintext` from the user's
 * public metadata, preserving `tenant_id` and `region`.
 *
 * Uses `PATCH /v1/users/{userId}` with `public_metadata` merge semantics.
 * The `pat_plaintext` key is removed by setting it explicitly to `null`
 * (Clerk treats `null` values in publicMetadata as deletion of that key).
 *
 * CTRL-CRED-001: Never logs the PAT or any metadata containing it.
 */
async function callClerkClearPat(
  userId: string,
  retainedMetadata: ClerkPublicMetadata,
): Promise<void> {
  const secretKey = process.env.CLERK_SECRET_KEY;
  if (!secretKey) throw new Error("CLERK_SECRET_KEY_not_configured");

  const url = `https://api.clerk.com/v1/users/${encodeURIComponent(userId)}`;
  const resp = await fetch(url, {
    method: "PATCH",
    headers: {
      authorization: `Bearer ${secretKey}`,
      "content-type": "application/json",
    },
    // Set pat_plaintext to null — Clerk treats null as "delete this key"
    // while preserving the other fields via merge semantics.
    body: JSON.stringify({
      public_metadata: {
        ...retainedMetadata,
        pat_plaintext: null,
      },
    }),
  });

  if (!resp.ok) {
    // Do NOT include response body — it may contain user PII.
    throw new Error(`clerk_metadata_clear_failed_${resp.status}`);
  }
}

/**
 * Server Action: clear `pat_plaintext` from Clerk public metadata and
 * redirect to /customer.
 *
 * Called by `CopyPatButton` when the user clicks "I've saved my token".
 * The action:
 *   1. Resolves the session (userId + retained metadata fields)
 *   2. Calls Clerk Backend API to remove pat_plaintext
 *   3. Revalidates /customer so it gets fresh data on next render
 *   4. Redirects to /customer
 *
 * This is idempotent: if pat_plaintext is already absent, the PATCH is a
 * no-op (Clerk merge semantics with null = already deleted).
 */
export async function clearPatPlaintext(): Promise<void> {
  const { userId, publicMetadata } = await resolveAuthContext();

  // Retain tenant_id and region; drop pat_plaintext
  const toRetain: ClerkPublicMetadata = {
    ...(publicMetadata.tenant_id ? { tenant_id: publicMetadata.tenant_id } : {}),
    ...(publicMetadata.region ? { region: publicMetadata.region } : {}),
  };

  await callClerkClearPat(userId, toRetain);

  revalidatePath("/customer");
  redirect("/customer");
}
