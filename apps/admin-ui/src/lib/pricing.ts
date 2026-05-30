/**
 * CoreLink pricing tier definitions.
 *
 * SOURCE OF TRUTH: docs/POSITIONING.md (org-wide SoT).
 * This file is the admin-ui copy — keep in sync with POSITIONING.md.
 *
 * Tier numbers last verified: 2026-05-30.
 */

export type Tier = {
  /** Canonical tier id used in Stripe product lookup. */
  id: string;
  name: string;
  /** Display price, e.g. "$0" or "$5". Empty string = "Talk to us". */
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
      "10 GB storage",
      "1 user",
      "Public projects only",
      "OSS use case",
      "Bazel + Turborepo protocol bridges",
    ],
    cta: "Sign up free",
    ctaHref: "/sign-up",
  },
  {
    id: "solo",
    name: "Solo",
    price: "$5",
    cadence: "/mo",
    features: [
      "100 GB storage",
      "Unlimited transfer (R2 zero-egress)",
      "1 user",
      "Private projects",
      "Bazel + Turborepo protocol bridges",
    ],
    cta: "Sign up",
    ctaHref: "/sign-up",
  },
  {
    id: "team",
    name: "Team",
    price: "$30",
    cadence: "/mo",
    features: [
      "1 TB storage",
      "Up to 10 users",
      "Multi-tenant within org",
      "Audit trail (Merkle chain)",
      "Unlimited transfer (R2 zero-egress)",
    ],
    cta: "Sign up",
    ctaHref: "/sign-up",
    highlight: true,
  },
  {
    id: "org",
    name: "Org",
    price: "$150",
    cadence: "/mo",
    features: [
      "10 TB storage",
      "Up to 50 users",
      "BYOK (bring your own key)",
      "Multi-region replication",
      "SLA included",
      "Unlimited transfer (R2 zero-egress)",
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
      "Custom storage & users",
      "Custom DPA",
      "Data residency",
      "Compliance packages",
      "Dedicated support",
    ],
    cta: "Talk to us",
    ctaHref: "mailto:gustavo@humangr.com",
  },
];
