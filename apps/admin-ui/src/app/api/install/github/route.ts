/**
 * GET /api/install/github — start the runner GitHub-App install flow.
 *
 * The "Install GitHub App" button (settings/runners) links here. This route:
 *   1. Resolves the authenticated CoreLink `tenant_id` SERVER-SIDE from the
 *      Clerk session claim (NEVER from the client — the signed state below is
 *      only as trustworthy as this binding). This is the canonical CoreLink
 *      tenant UUID carried in `publicMetadata.tenant_id`, NOT the Clerk
 *      `org_id`/`user_id` (those are Clerk identifiers, not the tenant the
 *      runner entitlement / provisioning is keyed by).
 *   2. Mints a signed `state = tenant_id` ([`@/lib/install-state`]) — the HMAC
 *      the signup-worker callback (`/install/github/callback`) verifies back to
 *      this exact tenant before provisioning `tenant_gh_installation_map` +
 *      `runner_repo_allowlist`.
 *   3. Redirects the browser into GitHub's App-install page with that state.
 *
 * Fail-closed: no session/tenant → 401; the signing key or App slug unbound →
 * 503 (mirrors the callback staying inert until secrets exist). We never mint a
 * state we can't bind, and never redirect to a half-configured install.
 *
 * See `docs/knowledge/flows/runner-github-install.md`.
 */

import { NextResponse } from "next/server";
import { signInstallState } from "@/lib/install-state";

// Edge runtime (Cloudflare Pages) — mirrors `/api/checkout/session`. Clerk's
// server SDK works on edge via the lazy `await import(...)` below; the HMAC uses
// Web Crypto (no Buffer dependency).
export const dynamic = "force-dynamic";

interface SessionClaims {
  /** Flattened tenant claim (as `/api/welcome/stream` reads it). */
  tenant_id?: string;
  /** Nested Clerk v6 public metadata (as the welcome page reads it). */
  publicMetadata?: { tenant_id?: string };
}

/**
 * Resolve the canonical CoreLink `tenant_id` from the Clerk session, tolerating
 * both claim shapes the app uses (flattened `tenant_id` and nested
 * `publicMetadata.tenant_id`). Lazy-imports `@clerk/nextjs/server` so tests /
 * non-Clerk environments don't crash.
 */
async function resolveTenantId(): Promise<string | null> {
  const mod = await import("@clerk/nextjs/server").catch(() => null);
  if (!mod) return null;
  try {
    const session = await (
      mod as { auth: () => Promise<{ sessionClaims?: SessionClaims }> }
    ).auth();
    const c = session.sessionClaims;
    return c?.tenant_id ?? c?.publicMetadata?.tenant_id ?? null;
  } catch {
    return null;
  }
}

/**
 * Read a primitive secret at edge runtime. `next-on-pages`/open-next exposes
 * Workers env via `process.env` for primitive secrets, but we also check
 * `globalThis.__env__` so the handler is testable without a Workers shim (same
 * defensive read as `/api/welcome/stream`).
 */
function readSecret(name: string): string | undefined {
  const fromProc = (process.env as Record<string, string | undefined>)[name];
  if (fromProc) return fromProc;
  const g = (globalThis as { __env__?: Record<string, string | undefined> }).__env__;
  return g?.[name];
}

export async function GET(): Promise<Response> {
  // 1. Identity — the ONLY authenticated tenant binding.
  const tenantId = await resolveTenantId();
  if (!tenantId) {
    return NextResponse.json(
      { error: "unauthenticated", message: "active Clerk session with a tenant required" },
      { status: 401 },
    );
  }

  // 2. Config — fail closed until the App exists and the secret is bound.
  const signingKey = readSecret("INSTALL_STATE_SIGNING_KEY");
  const appSlug = readSecret("GITHUB_APP_SLUG");
  if (!signingKey || !appSlug) {
    return NextResponse.json({ error: "runner_install_not_configured" }, { status: 503 });
  }

  // 3. Mint the signed state and redirect into GitHub's install page.
  const state = await signInstallState(tenantId, signingKey, Date.now());
  const target =
    `https://github.com/apps/${encodeURIComponent(appSlug)}` +
    `/installations/new?state=${encodeURIComponent(state)}`;
  return NextResponse.redirect(target, { status: 302 });
}
