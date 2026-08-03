"use client";

/**
 * Client component that opens the Stripe Customer Portal.
 *
 * Flow:
 *   1. User clicks "Open Stripe Portal".
 *   2. Component POSTs to `/corelink/api/v1/customer/billing/portal-session`
 *      (the basePath is re-attached via `withAppBasePath`) with
 *      `{ return_url }`. The return_url is derived from the current
 *      `window.location` so closing the portal returns the user to the
 *      billing page (NOT, e.g., to the marketing site).
 *   3. The server responds with `{ portal_url, session_id, expires_at_unix }`.
 *   4. We redirect via `window.location.assign(portal_url)`. We do NOT
 *      open in a new tab — Stripe's flow needs to own the document so
 *      cookie scoping works.
 *
 * Auth: the underlying backend route requires an authenticated Clerk
 * session + a valid tenant_id (resolved server-side from the JWT). The
 * page already lives under `[locale]/customer/...` which is gated by
 * middleware; we do NOT pass any customer-id from the client.
 *
 * Error handling: any non-2xx response surfaces inline (not silent).
 * The button is re-enabled so the user can retry.
 */

import * as React from "react";
import { withAppBasePath } from "@/lib/route-matcher";

interface PortalSessionResponse {
  portal_url: string;
  session_id: string;
  expires_at_unix: number;
}

export interface PortalLauncherProps {
  /** Locale slug for the return URL — falls back to "en". */
  locale: string;
  /** Tenant id from the Clerk session (server-resolved + passed in). */
  tenantId: string;
  /** Injected fetch impl for tests. */
  fetchImpl?: typeof fetch;
}

export function PortalLauncher({
  locale,
  tenantId,
  fetchImpl,
}: PortalLauncherProps): React.ReactElement {
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);

  async function open(): Promise<void> {
    setError(null);
    setBusy(true);
    try {
      // The catch-all mock at `/api/v1/[...path]` and production both
      // expect `tenant_id` + `return_url`. tenant_id is *also* derived
      // server-side from the JWT for defense-in-depth — the client
      // value MUST agree.
      const returnUrl =
        typeof window !== "undefined"
          ? `${window.location.origin}/${locale}/customer/billing`
          : `https://humangr.com/corelink/${locale}/customer/billing`;
      const f = fetchImpl ?? fetch;
      // Re-attach the surface's `/corelink` basePath. Next auto-prefixes
      // basePath onto framework-generated links but NEVER onto a raw `fetch()`
      // URL, so a bare `/api/v1/...` POSTs to the apex `humangr.com` — which is
      // the hugr-site MARKETING app, not this one (proven live: 405). Same
      // class as the checkout fix in #804.
      const res = await f(withAppBasePath("/api/v1/customer/billing/portal-session"), {
        method: "POST",
        headers: {
          "Accept": "application/json",
          "Content-Type": "application/json",
        },
        body: JSON.stringify({ return_url: returnUrl, tenant_id: tenantId }),
      });
      if (!res.ok) {
        const text = await res.text();
        throw new Error(`portal session failed: ${res.status} ${text.slice(0, 200)}`);
      }
      const body = (await res.json()) as PortalSessionResponse;
      if (!body.portal_url.startsWith("https://")) {
        throw new Error("server returned non-HTTPS portal URL");
      }
      // Hand off — Stripe owns the document from here.
      if (typeof window !== "undefined") {
        window.location.assign(body.portal_url);
      }
    } catch (e) {
      setError((e as Error).message);
      setBusy(false);
    }
  }

  return (
    <div data-testid="portal-launcher">
      <button
        type="button"
        onClick={open}
        disabled={busy}
        data-testid="portal-open-button"
      >
        {busy ? "Opening Stripe portal…" : "Open Stripe Portal"}
      </button>
      {error ? (
        <p role="alert" data-testid="portal-error">
          {error}
        </p>
      ) : null}
    </div>
  );
}
