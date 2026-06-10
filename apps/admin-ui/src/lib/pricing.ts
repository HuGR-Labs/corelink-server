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
};

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
];
