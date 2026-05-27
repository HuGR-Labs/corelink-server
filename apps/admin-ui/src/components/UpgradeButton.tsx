"use client";

/**
 * <UpgradeButton /> — PLG defer-billing upgrade trigger
 * (Phase 0.C — see `specs/_audits/2026-05-27-phase-0-execution-plan.md`
 * §2.C + `specs/_audits/2026-05-27-plg-onboarding-framework.md` §4).
 *
 * Replaces the in-wizard billing scaffold (deleted in this same
 * commit — was `apps/admin-ui/src/app/[locale]/onboarding/billing/`).
 * Lives on the customer billing page next to the existing
 * `<PortalLauncher />`. Free-tier tenants see "Upgrade to Pro" here;
 * paid tenants manage their subscription via the Stripe Portal.
 *
 * Flow:
 *   1. User clicks the button.
 *   2. We POST to `/api/checkout/session` with `Accept:
 *      application/json` so the route returns `{ checkout_url,
 *      session_id }` (vs the form-post path which gets a 303).
 *   3. We hand off via `window.location.assign(checkout_url)`. We do
 *      NOT open in a new tab — Stripe needs to own the document so
 *      cookie scoping + post-Checkout return URL work correctly
 *      (same rationale as `<PortalLauncher />`).
 *   4. On Stripe success → `/${locale}/upgraded?session_id=cs_…`.
 *      On Stripe cancel → `/${locale}/pricing`. Activation
 *      (`tenants.plan='pro'`) happens server-side via the
 *      `checkout.session.completed` webhook (idempotent — replaying
 *      the same Stripe `evt_*` id is a no-op via the existing
 *      `stripe_checkout_sessions` dedup row in
 *      `corelink-tier-selection/src/ledger.rs`).
 *
 * Error handling: any non-2xx surfaces inline (not silent). The
 * button re-enables so the user can retry. A 403 from the backend
 * means the DPA-first lock fired — surfacing the body text gives the
 * user the actionable signal ("you must accept the DPA first").
 */

import * as React from "react";

export interface UpgradeButtonProps {
  /** Locale slug for the post-Checkout redirect — falls back to "en". */
  locale?: string;
  /**
   * Canonical paid tier (`starter` | `team` | `pro` | `enterprise`).
   * Defaults to "pro" — the launch tier per launch-readiness §2.
   */
  tier?: "starter" | "team" | "pro" | "enterprise";
  /** Override button label (i18n owners do this; default is English). */
  label?: string;
  /** Injected fetch impl for tests. */
  fetchImpl?: typeof fetch;
  /**
   * Optional redirect callback (test-only injection). Production
   * leaves this unset and the component hands off via
   * `window.location.assign(url)`.
   */
  redirectImpl?: (url: string) => void;
}

interface CheckoutSessionJson {
  checkout_url: string;
  session_id: string;
}

export function UpgradeButton({
  locale = "en",
  tier = "pro",
  label,
  fetchImpl,
  redirectImpl,
}: UpgradeButtonProps): React.ReactElement {
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);

  async function onClick(): Promise<void> {
    setError(null);
    setBusy(true);
    try {
      const f = fetchImpl ?? fetch;
      const res = await f("/api/checkout/session", {
        method: "POST",
        headers: {
          "Accept": "application/json",
          "Content-Type": "application/json",
        },
        body: JSON.stringify({ tier, locale }),
      });
      if (!res.ok) {
        const text = await res.text();
        throw new Error(`upgrade failed: ${res.status} ${text.slice(0, 200)}`);
      }
      const body = (await res.json()) as CheckoutSessionJson;
      if (!body.checkout_url || !body.checkout_url.startsWith("https://")) {
        throw new Error("server returned non-HTTPS Checkout URL");
      }
      // Hand off — Stripe owns the document from here.
      if (redirectImpl) {
        redirectImpl(body.checkout_url);
      } else if (typeof window !== "undefined") {
        window.location.assign(body.checkout_url);
      }
    } catch (e) {
      setError((e as Error).message);
      setBusy(false);
    }
  }

  const displayLabel =
    label ?? `Upgrade to ${tier.charAt(0).toUpperCase() + tier.slice(1)}`;

  return (
    <div data-testid="upgrade-button">
      <button
        type="button"
        onClick={onClick}
        disabled={busy}
        data-testid="upgrade-open-button"
      >
        {busy ? "Opening Stripe Checkout…" : displayLabel}
      </button>
      {error ? (
        <p role="alert" data-testid="upgrade-error">
          {error}
        </p>
      ) : null}
    </div>
  );
}
