"use server";

import { redirect } from "next/navigation";
import { revalidatePath } from "next/cache";

/**
 * Resolve the currently authenticated Clerk user's ID.
 *
 * Uses `@clerk/nextjs/server` lazy import so test environments without Clerk
 * wiring don't crash. Throws if there is no active session.
 */
async function resolveUserId(): Promise<string> {
  const mod = await import("@clerk/nextjs/server").catch(() => null);
  if (!mod) throw new Error("auth_provider_not_initialized");

  const clerkMod = mod as {
    auth: () => Promise<{ userId: string | null }>;
  };

  const session = await clerkMod.auth();
  if (!session.userId) throw new Error("no_active_session");

  return session.userId;
}

/**
 * Call the Clerk Backend API to remove `pat_plaintext` from the user's
 * PRIVATE metadata.
 *
 * SECURITY (2026-06-19, CRED-pat-plaintext): the PAT plaintext lives in
 * Clerk `private_metadata` (backend-only — never in the session JWT, never
 * readable by `useUser()`), so the clear must target private_metadata. The
 * `pat_plaintext` key is removed by setting it explicitly to `null` (Clerk
 * treats `null` values in metadata as deletion of that key) — merge semantics
 * preserve any other private fields.
 *
 * CTRL-CRED-001: Never logs the PAT or any metadata containing it.
 */
async function callClerkClearPat(userId: string): Promise<void> {
  const secretKey = process.env.CLERK_SECRET_KEY;
  if (!secretKey) throw new Error("CLERK_SECRET_KEY_not_configured");

  const mod = await import("@clerk/nextjs/server").catch(() => null);
  if (!mod) throw new Error("auth_provider_not_initialized");

  const clerk = await (
    mod as {
      clerkClient: () => Promise<{
        users: {
          updateUser: (
            id: string,
            params: { privateMetadata: Record<string, unknown> },
          ) => Promise<unknown>;
        };
      }>;
    }
  ).clerkClient();

  try {
    // Set pat_plaintext to null — Clerk treats null as "delete this key"
    // while preserving the other private fields via merge semantics.
    await clerk.users.updateUser(userId, {
      privateMetadata: { pat_plaintext: null },
    });
  } catch (err) {
    // Do NOT include the error detail — it may carry user metadata/PII.
    const status =
      typeof (err as { status?: unknown })?.status === "number"
        ? (err as { status: number }).status
        : "error";
    throw new Error(`clerk_metadata_clear_failed_${status}`);
  }
}

/**
 * Server Action: clear `pat_plaintext` from Clerk private metadata and
 * redirect to /customer.
 *
 * Called by `CopyPatButton` when the user clicks "I've saved my token".
 * The action:
 *   1. Resolves the session (userId)
 *   2. Calls the Clerk Backend API to remove private_metadata.pat_plaintext
 *   3. Revalidates /customer so it gets fresh data on next render
 *   4. Redirects to /customer
 *
 * This is idempotent: if pat_plaintext is already absent, the update is a
 * no-op (Clerk merge semantics with null = already deleted).
 */
export async function clearPatPlaintext(): Promise<void> {
  const userId = await resolveUserId();

  await callClerkClearPat(userId);

  revalidatePath("/customer");
  redirect("/customer");
}
