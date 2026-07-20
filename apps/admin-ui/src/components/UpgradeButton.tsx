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
 * DPA-first gate (INV-ONBOARD-DPA-FIRST): the backend
 * `POST /v1/onboarding/tier-select` returns `403` with a `dpa_required`
 * body until the tenant has accepted the current Data Processing Agreement.
 * When the checkout POST surfaces that exact signal, this component renders
 * the shared `<DpaStep />` click-through (the SAME component + legal text the
 * team-invite gate uses), requires the scroll-to-end + explicit accept it
 * enforces, records the acceptance via `acceptDpaAction`, and then
 * AUTOMATICALLY retries the checkout POST — so a signed-in user who has not
 * yet accepted the DPA can accept it and proceed to Stripe without leaving
 * the page. Any OTHER non-2xx surfaces inline (not silent) via the kit
 * `InlineError`, which preserves the full error text (status + body). The
 * retry re-runs the same POST the click handler runs.
 */

import * as React from "react";
import { InlineError, Callout } from "@/components/ui/linear";
import type { CheckoutTierId } from "@/lib/pricing";
import { DpaStep } from "@/components/DpaStep";
import type { DpaNotice } from "@/lib/dpa-notice";
import { requestBasePath } from "@/lib/route-matcher";
import type { Locale } from "@/i18n/messages";

const SUPPORTED_LOCALES: readonly Locale[] = ["en", "pt", "es", "de"];

/** Coerce the free-form `locale` prop to a supported `Locale` (default en). */
function coerceLocale(loc: string): Locale {
  return (SUPPORTED_LOCALES as readonly string[]).includes(loc)
    ? (loc as Locale)
    : "en";
}

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
  // When the checkout POST 403s with `dpa_required`, we load the localized DPA
  // notice and render the shared click-through gate below the button. Cleared
  // once the user accepts (which auto-retries the checkout).
  const [dpaNotice, setDpaNotice] = React.useState<DpaNotice | null>(null);

  const startCheckout = React.useCallback(async (): Promise<void> => {
    setError(null);
    setDpaNotice(null);
    setBusy(true);
    try {
      const f = fetchImpl ?? fetch;
      // Re-attach the surface's `/corelink` basePath: Next auto-prefixes
      // basePath onto framework links but NEVER onto a raw `fetch()` URL, so a
      // bare `/api/checkout/session` posts to the apex (→ 405, checkout dead)
      // on the path surface after the humangr.com/corelink migration (#804).
      // Derive it from the current pathname so both migration surfaces work.
      const basePath =
        typeof window !== "undefined" ? requestBasePath(window.location.pathname) : "";
      const res = await f(`${basePath}/api/checkout/session`, {
        method: "POST",
        headers: {
          "Accept": "application/json",
          "Content-Type": "application/json",
        },
        body: JSON.stringify({ tier, locale }),
      });
      if (!res.ok) {
        const text = await res.text();
        // DPA-first lock (INV-ONBOARD-DPA-FIRST): render the accept gate rather
        // than a dead-end error. The route maps the backend 403 through
        // verbatim with a `dpa_required` marker in the body.
        if (res.status === 403 && text.includes("dpa_required")) {
          const { loadDpaNotice } = await import("@/lib/dpa-notice");
          const notice = await loadDpaNotice(coerceLocale(locale));
          setDpaNotice(notice);
          setBusy(false);
          return;
        }
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
      {dpaNotice ? (
        <div className="lin-mt" data-testid="upgrade-dpa-gate">
          <Callout tone="info">
            Before your first paid plan, accept the Data Processing Agreement —
            it governs how CoreLink processes your data. We&rsquo;ll take you
            straight to Stripe Checkout once you accept.
          </Callout>
          <div className="lin-mt">
            <DpaStep
              locale={dpaNotice.locale}
              // The tier-select / dpa-accept backend resolves the tenant from
              // the verified session header, never the body — so no tenant id
              // travels here (same contract as the team-invite gate).
              tenantId=""
              dpaText={dpaNotice.dpaText}
              dpaVersion={dpaNotice.dpaVersion}
              noticeTextHash={dpaNotice.noticeTextHash}
              // Acceptance recorded → auto-retry the checkout POST.
              onAccepted={() => {
                setDpaNotice(null);
                void startCheckout();
              }}
            />
          </div>
        </div>
      ) : null}
    </div>
  );
}
