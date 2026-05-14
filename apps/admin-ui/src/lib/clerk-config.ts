/**
 * Clerk configuration helpers (WI-S16-001 foundation; full PAT flows in WI-S16-002).
 *
 * Reads env vars and exposes structured config. Never logs the secret key.
 */

export interface ClerkRuntimeConfig {
  publishableKey: string;
  /** Server-only. Never expose to client. */
  secretKey: string | undefined;
  /** Optional JWT template used by the PAT-issuing flow (WI-S16-002). */
  patJwtTemplate: string | undefined;
}

export function getClerkConfig(): ClerkRuntimeConfig {
  const publishableKey = process.env["NEXT_PUBLIC_CLERK_PUBLISHABLE_KEY"];
  if (!publishableKey) {
    // Soft-fail: in dev/test we tolerate missing keys. Prod sign-in will surface a clear error.
    return {
      publishableKey: "",
      secretKey: undefined,
      patJwtTemplate: undefined,
    };
  }
  return {
    publishableKey,
    secretKey: process.env["CLERK_SECRET_KEY"],
    patJwtTemplate: process.env["CLERK_JWT_TEMPLATE_PAT"],
  };
}

/**
 * PAT-issuing flow stub. Full implementation lands in WI-S16-002.
 * Documented here so the type surface is stable across follow-up WIs.
 */
export interface PatIssueRequest {
  scope: "read-only" | "read-write" | "admin";
  expiresIn: "30d" | "90d" | "1y" | "never";
  label: string;
}

export interface PatIssueResult {
  /** Displayed only once to the user (CTRL-CRED-001). */
  patPlaintext: string;
  patId: string;
  expiresAt: string | null;
}

export async function issuePat(_req: PatIssueRequest): Promise<PatIssueResult> {
  throw new Error("issuePat: not implemented (WI-S16-002 deliverable).");
}
