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
 * Error handling: any non-2xx surfaces inline (not silent) via the kit
 * `InlineError`, which preserves the full error text (status + body) so
 * the DPA-first 403 signal remains actionable. The retry re-runs the same
 * POST the click handler runs.
 */

import * as React from "react";
import { InlineError } from "@/components/ui/linear";
import type { CheckoutTierId } from "@/lib/pricing";

export interface UpgradeButtonProps {
  /** Locale slug for the post-Checkout redirect — falls back to "en". */
  locale?: string;
  /**
   * Canonical checkout-able tier — the 4 paid SKUs of the 6-tier ladder
   * (`solo` | `starter` | `pro` | `max`) plus legacy `team`
   * (`CHECKOUT_TIER_IDS` in src/lib/pricing.ts).
   * Defaults to "pro" — the anchor SKU per the launch rate card.
   */
  tier?: CheckoutTierId;
  /** Override button label (i18n owners do this; default is English). */
  label?: string;
  /**
   * Fire the checkout POST automatically on mount (used by
   * `/[locale]/upgrade` to drive Checkout immediately for signed-in
   * visitors arriving from the public pricing CTAs). The button stays
   * rendered as the manual fallback: if the auto-fired POST fails, the
   * error surfaces inline and the user can retry by clicking. Fires at
   * most once per mount (StrictMode-safe via ref guard).
   */
  autoStart?: boolean;
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
  autoStart = false,
  fetchImpl,
  redirectImpl,
}: UpgradeButtonProps): React.ReactElement {
  const [busy, setBusy] = React.useState(false);
  const [error, setError] = React.useState<string | null>(null);

  const startCheckout = React.useCallback(async (): Promise<void> => {
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
  }, [fetchImpl, redirectImpl, tier, locale]);

  // Auto-submit path (`/[locale]/upgrade`): kick off the same POST the
  // click handler runs, exactly once per mount. The ref guard keeps
  // React 18 StrictMode's double-invoked dev effects (and any dep-driven
  // re-runs) from minting two Checkout sessions.
  const autoStarted = React.useRef(false);
  React.useEffect(() => {
    if (!autoStart || autoStarted.current) return;
    autoStarted.current = true;
    void startCheckout();
  }, [autoStart, startCheckout]);

  const displayLabel =
    label ?? `Upgrade to ${tier.charAt(0).toUpperCase() + tier.slice(1)}`;

  return (
    <div data-testid="upgrade-button" className="lin-checklist">
      <button
        type="button"
        onClick={startCheckout}
        disabled={busy}
        data-testid="upgrade-open-button"
        className="lin-btn lin-btn--primary"
      >
        {busy ? "Opening Stripe Checkout…" : displayLabel}
      </button>
      {error ? (
        <div role="alert" data-testid="upgrade-error">
          <InlineError error={error} onRetry={() => void startCheckout()} />
        </div>
      ) : null}
    </div>
  );
}
