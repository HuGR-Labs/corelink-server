/**
 * POST /api/checkout/session — Stripe Checkout Session bridge
 * (Phase 0.C — PLG defer-billing pattern).
 *
 * Flow:
 *   1. Client `<UpgradeButton />` POSTs `{ tier }` (defaults to "pro").
 *   2. We resolve the Clerk session token server-side (NEVER the
 *      tenant_id from the client — defense-in-depth: the backend
 *      cross-checks the JWT claim).
 *   3. We forward to the canonical backend route
 *      `POST /v1/onboarding/tier-select`, which:
 *        - enforces the `INV-ONBOARD-DPA-FIRST` D1 lock (rejects with
 *          403 if the tenant has not accepted the current DPA),
 *        - mints a Stripe-hosted Checkout Session URL (Stripe owns
 *          PCI scope; CoreLink never sees a card number),
 *        - records the session id in `stripe_checkout_sessions` so
 *          the subsequent `checkout.session.completed` webhook (idem-
 *          potent via Stripe `evt_*` event id — see
 *          `crates/corelink-tier-selection/src/ledger.rs`) can
 *          activate the tier.
 *   4. We respond based on the request `Accept` header:
 *        - `Accept: application/json` (the `<UpgradeButton />` path):
 *          200 with `{ checkout_url, session_id }` — the client
 *          performs the redirect via `window.location.assign(...)`.
 *          This is the primary, reliably-cross-browser path.
 *        - Otherwise (raw `<form method="POST">` post — kept for
 *          progressive-enhancement / no-JS fallback): 303 redirect to
 *          `checkout_url`. 303 (See Other) is the correct status for
 *          "POST that completes with a GET-able resource elsewhere".
 *
 * Why NOT call `stripe.checkout.sessions.create()` directly from this
 * route?
 *   - admin-ui runs on Cloudflare Pages (edge runtime). The Node-only
 *     `stripe` SDK ships Buffer/crypto-node bindings that fail to
 *     bundle for edge workers.
 *   - The canonical Stripe HTTPS client lives in
 *     `crates/corelink-stripe-real` with the dual-mode wallet-broker
 *     auth path (wave-31). Bypassing it would split the auth surface
 *     and double-up audit/idempotency wiring.
 *   - The activation path (`checkout.session.completed` →
 *     `tenants.plan = 'pro'`) is owned by `corelink-tier-selection`
 *     and already covered by 10k-iter property tests for idempotency
 *     + DPA-first ordering.
 *
 * Webhook idempotency: the canonical Stripe `evt_*` id is used as the
 * dedup key by the existing `/v1/billing/stripe-webhook` handler
 * (`crates/corelink-billing-stripe/src/webhook.rs` +
 * `crates/corelink-tier-selection/src/ledger.rs`). Replaying the same
 * `checkout.session.completed` event is a no-op (returns the prior
 * `StripeAdapterDecision::DuplicateRejected` audit row without
 * touching the tier ledger).
 *
 * See `specs/_audits/2026-05-27-phase-0-execution-plan.md` §2.C +
 * `specs/_audits/2026-05-27-plg-onboarding-framework.md` §4.
 */

import { NextResponse, type NextRequest } from "next/server";
import { apiPost, ApiClientError } from "@/lib/api-client";
import { CHECKOUT_TIER_IDS, DEFAULT_CHECKOUT_TIER } from "@/lib/pricing";

// Edge runtime — required by Cloudflare Pages (per Wave 32 Phase F).
// Clerk's server SDK works on edge via the lazy `await import(...)`
// pattern below; the same approach is used in `/api/welcome/stream`
// and `/api/v1/[...path]`. JWT decode uses Web Crypto (`crypto.subtle`)
// which is available in the edge runtime — no Buffer dependency.
export const dynamic = "force-dynamic";

/**
 * Canonical checkout-able tiers accepted by this route — the 4 paid SKUs of
 * the 6-tier ladder (solo/starter/pro/max) plus legacy `team` (still accepted
 * by the 0062 CHECK). Enterprise is NOT checkout-able (the tier-select
 * backend 422s it toward the inquiry form), so it gets the clear 400 below.
 * Sourced from `CHECKOUT_TIER_IDS` (src/lib/pricing.ts) so this gate and the
 * `/[locale]/upgrade?plan=` validation can never drift apart.
 */
const PAID_TIERS = new Set<string>(CHECKOUT_TIER_IDS);
const DEFAULT_TIER = DEFAULT_CHECKOUT_TIER;

interface CheckoutRequestBody {
  tier?: string;
  /**
   * Optional locale slug for the success/cancel redirect — falls back
   * to "en". Locale is taken from the body (NOT the URL) because the
   * client component owns the locale via the layout context.
   */
  locale?: string;
}

interface TierSelectResponse {
  checkout_url: string;
  session_id: string;
}

/**
 * Resolve the Clerk session token at the server boundary. Mirrors the
 * pattern in `apps/admin-ui/src/app/[locale]/onboarding/actions.ts`
 * (lazy-import so tests / non-Clerk environments don't crash).
 */
async function getSessionToken(): Promise<string | null> {
  const mod = await import("@clerk/nextjs/server").catch(() => null);
  if (!mod) return null;
  try {
    const session = await (
      mod as { auth: () => Promise<{ getToken: () => Promise<string | null> }> }
    ).auth();
    return await session.getToken();
  } catch {
    return null;
  }
}

function originFromRequest(req: NextRequest): string {
  // Trust the forwarded host/proto if behind Cloudflare; else use the
  // request URL as parsed by Next.js. Never trust user-supplied origin
  // from the JSON body — that would let an attacker direct Stripe's
  // post-checkout redirect anywhere.
  const proto =
    req.headers.get("x-forwarded-proto") ??
    new URL(req.url).protocol.replace(":", "");
  const host =
    req.headers.get("x-forwarded-host") ??
    req.headers.get("host") ??
    new URL(req.url).host;
  return `${proto}://${host}`;
}

export async function POST(req: NextRequest): Promise<NextResponse> {
  // --- 1. Parse body (small, defensive). ----------------------------
  let body: CheckoutRequestBody = {};
  try {
    body = (await req.json()) as CheckoutRequestBody;
  } catch {
    // Empty body is fine — we default to `tier=pro`. Malformed JSON is
    // also benign here since we only read two optional fields.
    body = {};
  }

  const tier = (body.tier ?? DEFAULT_TIER).toLowerCase();
  if (!PAID_TIERS.has(tier)) {
    return NextResponse.json(
      { error: "invalid_tier", message: `tier must be one of ${[...PAID_TIERS].join(", ")}` },
      { status: 400 },
    );
  }
  const locale = (body.locale ?? "en").replace(/[^a-z-]/gi, "").slice(0, 8) || "en";

  // --- 2. Auth. -----------------------------------------------------
  const token = await getSessionToken();
  if (!token) {
    return NextResponse.json(
      { error: "unauthenticated", message: "active Clerk session required" },
      { status: 401 },
    );
  }

  // --- 3. Build URLs. -----------------------------------------------
  const origin = originFromRequest(req);
  const success_url = `${origin}/${locale}/upgraded?session_id={CHECKOUT_SESSION_ID}`;
  const cancel_url = `${origin}/${locale}/pricing`;

  // --- 4. Call canonical backend route. -----------------------------
  let resp: TierSelectResponse;
  try {
    resp = await apiPost<TierSelectResponse>(
      "/v1/onboarding/tier-select",
      { tier, success_url, cancel_url },
      { token },
    );
  } catch (e) {
    if (e instanceof ApiClientError) {
      // Map backend status through verbatim — 403 means DPA-first
      // lock fired, 429 means rate-limited, etc. The body is already
      // token-redacted by `api-client.ts`.
      return NextResponse.json(
        { error: "tier_select_failed", status: e.status, body: e.body },
        { status: e.status >= 400 && e.status < 600 ? e.status : 502 },
      );
    }
    return NextResponse.json(
      { error: "tier_select_unreachable", message: (e as Error).message },
      { status: 502 },
    );
  }

  if (
    typeof resp.checkout_url !== "string" ||
    !resp.checkout_url.startsWith("https://")
  ) {
    return NextResponse.json(
      { error: "invalid_checkout_url", message: "backend returned non-HTTPS Checkout URL" },
      { status: 502 },
    );
  }

  // --- 5. Respond per Accept header. --------------------------------
  // The XHR/fetch path (Accept: application/json) gets the URL as
  // JSON so the client can `window.location.assign(...)` with full
  // control of the redirect (avoids opaque-redirect issues across
  // browsers). The form-post path (e.g. <form method="POST"
  // action="/api/checkout/session">) gets a 303 so the browser
  // navigates directly to Stripe.
  const accept = (req.headers.get("accept") ?? "").toLowerCase();
  if (accept.includes("application/json")) {
    return NextResponse.json(
      { checkout_url: resp.checkout_url, session_id: resp.session_id },
      { status: 200 },
    );
  }
  return NextResponse.redirect(resp.checkout_url, { status: 303 });
}
