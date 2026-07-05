/**
 * CoreLink pricing tier definitions.
 *
 * SOURCE OF TRUTH: the signed launch rate card —
 * `apps/docs/src/lib/pricing.ts` (TIER_RATE_CARD, FROZEN 6-tier taxonomy)
 * per `specs/_audits/2026-05-27-pricing-benchmarks.md` §5; positioning
 * context in docs/POSITIONING.md. Keep this admin-ui copy in sync.
 *
 * Tier numbers last verified: 2026-06-10 (6-tier launch ladder).
 */

/**
 * Product axis a tier belongs to. `cache` = the storage/CAS ladder
 * (Free…Enterprise). `runner` = the SEPARATE self-serve CI-runner axis
 * (concurrency + monthly vCPU-h, NOT storage) — rendered as its own
 * section of cards so the two ladders are never visually conflated.
 * Defaults to `cache` when omitted (keeps the existing cache entries
 * terse).
 */
export type TierGroup = "cache" | "runner";

export type Tier = {
  /** Canonical tier id used in Stripe product lookup. */
  id: string;
  name: string;
  /** Display price, e.g. "$0" or "$15". Empty string = "Talk to us". */
  price: string;
  /** Per-month suffix or empty for Enterprise. */
  cadence: string;
  features: string[];
  cta: string;
  ctaHref: string;
  /** When true the card receives a visual "most popular" highlight ring. */
  highlight?: boolean;
  /** Product axis — omitted means `cache`. See {@link TierGroup}. */
  group?: TierGroup;
};

/**
 * Checkout-able tiers — the 4 paid SKUs of the 6-tier ladder
 * (solo/starter/pro/max) plus legacy `team` (still accepted by the 0062
 * CHECK). Single source of truth for BOTH the `/api/checkout/session`
 * gate (`PAID_TIERS`) and the `/upgrade?plan=` query validation, so the
 * two surfaces can never drift. Free is not checkout-able (sign-up is),
 * and Enterprise is NOT checkout-able either (the tier-select backend
 * 422s it toward the inquiry form).
 */
export const CHECKOUT_TIER_IDS = [
  "solo",
  "starter",
  "team",
  "pro",
  "max",
  // Self-serve CI-runner axis (separate product from the cache ladder).
  // These ids are byte-identical to the backend tier-select contract —
  // a mismatch breaks checkout. Do NOT rename.
  "runner_starter",
  "runner_pro",
  "runner_team",
  "runner_scale",
  "runner_max",
] as const;

export type CheckoutTierId = (typeof CHECKOUT_TIER_IDS)[number];

/** Anchor SKU per the launch rate card — the fallback for `?plan=`. */
export const DEFAULT_CHECKOUT_TIER: CheckoutTierId = "pro";

export function isCheckoutTierId(value: string): value is CheckoutTierId {
  return (CHECKOUT_TIER_IDS as readonly string[]).includes(value);
}

/**
 * Normalize a raw `?plan=` query value (as handed over by Next.js
 * `searchParams` — possibly an array for repeated params, possibly
 * absent) to a checkout-able tier id. Case-insensitive; repeated params
 * use the first occurrence; missing/invalid values fall back to the
 * anchor SKU `pro` — the public docs pricing CTAs always pass a valid
 * plan, so the fallback only fires on hand-edited URLs.
 */
export function normalizeCheckoutTier(
  plan: string | readonly string[] | undefined | null,
): CheckoutTierId {
  const raw = Array.isArray(plan) ? plan[0] : plan;
  const candidate = (typeof raw === "string" ? raw : "").trim().toLowerCase();
  return isCheckoutTierId(candidate) ? candidate : DEFAULT_CHECKOUT_TIER;
}

export const TIERS: Tier[] = [
  {
    id: "free",
    name: "Free",
    price: "$0",
    cadence: "/mo",
    features: [
      "10 GB CAS storage",
      "500K cache requests/mo",
      "1 workspace",
      "Bazel + Turborepo protocol bridges",
      "Community support",
    ],
    cta: "Sign up free",
    ctaHref: "/sign-up",
  },
  {
    id: "solo",
    name: "Solo",
    price: "$15",
    cadence: "/mo",
    features: [
      "50 GB CAS storage",
      "2M cache requests/mo",
      "Unlimited workspaces",
      "Zero-egress transfer (R2)",
      "Email support",
    ],
    cta: "Sign up",
    ctaHref: "/sign-up",
  },
  {
    id: "starter",
    name: "Starter",
    price: "$35",
    cadence: "/mo",
    features: [
      "150 GB CAS storage",
      "6M cache requests/mo",
      "Unlimited workspaces",
      "Zero-egress transfer (R2)",
      "Email support (2-business-day target)",
    ],
    cta: "Sign up",
    ctaHref: "/sign-up",
  },
  {
    id: "pro",
    name: "Pro",
    price: "$50",
    cadence: "/mo",
    features: [
      "500 GB CAS storage",
      "20M cache requests/mo",
      "Unlimited workspaces",
      "Zero-egress transfer (R2)",
      "Email support (1-business-day target)",
    ],
    cta: "Sign up",
    ctaHref: "/sign-up",
    highlight: true,
  },
  {
    id: "max",
    name: "Max",
    price: "$149",
    cadence: "/mo",
    features: [
      "2 TB CAS storage",
      "80M cache requests/mo",
      "Unlimited workspaces",
      "Zero-egress transfer (R2)",
      "Priority email support",
    ],
    cta: "Sign up",
    ctaHref: "/sign-up",
  },
  {
    id: "enterprise",
    name: "Enterprise",
    price: "",
    cadence: "",
    features: [
      "BYOK (bring your own key)",
      "SSO / SAML",
      "99.9% SLA with credits",
      "Custom DPA & data residency",
      "Dedicated support",
    ],
    cta: "Talk to us",
    ctaHref: "mailto:gustavo@humangr.com",
  },
  // ---------------------------------------------------------------------
  // CI-runner axis — a SEPARATE, self-serve product from the cache ladder
  // above (ephemeral runners on the CoreLink cache, per
  // marketing/expansion/ci-build-acceleration.md). Priced on concurrency
  // + monthly vCPU-h, NOT storage. Flat monthly subscription (no metering
  // shown). Rendered as its own card section by the pricing page. The ids
  // are byte-identical to the backend tier-select contract.
  // ---------------------------------------------------------------------
  {
    id: "runner_starter",
    name: "Runner Starter",
    price: "$16",
    cadence: "/mo",
    group: "runner",
    features: [
      "20 concurrent runners",
      "100 vCPU-hours/mo included",
      "Cache-accelerated builds (CoreLink CAS)",
      "Bring your own CI (GitHub Actions, etc.)",
      "Email support",
    ],
    cta: "Get runners",
    ctaHref: "/upgrade?plan=runner_starter",
  },
  {
    id: "runner_pro",
    name: "Runner Pro",
    price: "$40",
    cadence: "/mo",
    group: "runner",
    features: [
      "40 concurrent runners",
      "240 vCPU-hours/mo included",
      "Cache-accelerated builds (CoreLink CAS)",
      "Bring your own CI (GitHub Actions, etc.)",
      "Email support",
    ],
    cta: "Get runners",
    ctaHref: "/upgrade?plan=runner_pro",
    highlight: true,
  },
  {
    id: "runner_team",
    name: "Runner Team",
    price: "$100",
    cadence: "/mo",
    group: "runner",
    features: [
      "80 concurrent runners",
      "600 vCPU-hours/mo included",
      "Cache-accelerated builds (CoreLink CAS)",
      "Bring your own CI (GitHub Actions, etc.)",
      "Email support",
    ],
    cta: "Get runners",
    ctaHref: "/upgrade?plan=runner_team",
  },
  {
    id: "runner_scale",
    name: "Runner Scale",
    price: "$200",
    cadence: "/mo",
    group: "runner",
    features: [
      "160 concurrent runners",
      "1,200 vCPU-hours/mo included",
      "Cache-accelerated builds (CoreLink CAS)",
      "Bring your own CI (GitHub Actions, etc.)",
      "Priority email support",
    ],
    cta: "Get runners",
    ctaHref: "/upgrade?plan=runner_scale",
  },
  {
    id: "runner_max",
    name: "Runner Max",
    price: "$400",
    cadence: "/mo",
    group: "runner",
    features: [
      "320 concurrent runners",
      "2,400 vCPU-hours/mo included",
      "Cache-accelerated builds (CoreLink CAS)",
      "Bring your own CI (GitHub Actions, etc.)",
      "Priority email support",
    ],
    cta: "Get runners",
    ctaHref: "/upgrade?plan=runner_max",
  },
];
