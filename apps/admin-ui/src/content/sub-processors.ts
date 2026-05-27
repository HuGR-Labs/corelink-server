// AUTO-GENERATED from sub-processors.json — do not edit manually, edit the .json source.
// Edge-compat static export (no node:fs / node:path).
//
// Types are inlined here to avoid a circular import with load.ts.
// load.ts re-exports SubProcessor / SubProcessorList so callers are unaffected.

export interface SubProcessor {
  id: string;
  name: string;
  role: string;
  region: string;
  certifications: string[];
  dpa_url: string;
  last_audit: string;
}

export interface SubProcessorList {
  version: string;
  items: SubProcessor[];
}

export const subProcessors: SubProcessorList = {
  version: "2026-05-27",
  items: [
    {
      id: "cloudflare",
      name: "Cloudflare, Inc.",
      role: "CDN / edge compute (Workers) / object storage (R2) / KV / Durable Objects / D1 / Pages / Email Routing",
      region: "Global (anycast) with regional pinning where configured",
      certifications: ["ISO 27001", "SOC 2 Type II", "ISO 27018", "PCI DSS Level 1", "EU–US DPF"],
      dpa_url: "https://www.cloudflare.com/cloudflare-customer-dpa/",
      last_audit: "2026-02-12",
    },
    {
      id: "clerk",
      name: "Clerk, Inc.",
      role: "Authentication, SSO, MFA — stores email, name, password hash, session tokens",
      region: "United States (us-east-1)",
      certifications: ["SOC 2 Type II", "GDPR", "CCPA", "EU–US DPF"],
      dpa_url: "https://clerk.com/legal/dpa",
      last_audit: "2026-01-30",
    },
    {
      id: "stripe",
      name: "Stripe, Inc.",
      role: "Payment processing and subscription billing — stores billing email + payment-method tokens",
      region: "United States; EU customers processed by Stripe Payments Europe Ltd. (Ireland)",
      certifications: ["PCI DSS Level 1", "SOC 2 Type II", "ISO 27001", "EU–US DPF"],
      dpa_url: "https://stripe.com/legal/dpa",
      last_audit: "2026-03-04",
    },
    {
      id: "resend",
      name: "Resend",
      role: "Transactional email (account verification, newsletters, sub-processor notices) + Audience for newsletter opt-in list",
      region: "United States",
      certifications: ["SOC 2 Type II (in progress per Resend docs)"],
      dpa_url: "https://resend.com/legal/dpa",
      last_audit: "2026-05-27",
    },
    {
      id: "sentry",
      name: "Sentry (Functional Software, Inc.)",
      role: "Application error monitoring — stores error events with PII fields scrubbed by config",
      region: "United States (Sentry SaaS Cloud)",
      certifications: ["SOC 2 Type II", "GDPR"],
      dpa_url: "https://sentry.io/legal/dpa/",
      last_audit: "2026-05-27",
    },
    {
      id: "plausible",
      name: "Plausible Analytics",
      role: "Privacy-first web analytics for public docs site — cookieless, no PII collected",
      region: "European Union (Germany)",
      certifications: ["GDPR (cookieless by design, no personal data collected)"],
      dpa_url: "https://plausible.io/data-policy",
      last_audit: "2026-05-27",
    },
    {
      id: "betterstack",
      name: "Better Stack, Inc. (Better Uptime)",
      role: "Public status page — displays operational status; no customer personal data processed",
      region: "European Union",
      certifications: ["GDPR"],
      dpa_url: "https://betterstack.com/privacy",
      last_audit: "2026-05-27",
    },
    {
      id: "github",
      name: "GitHub, Inc. (Microsoft)",
      role: "Source code hosting, CI pipeline, release artifact publishing",
      region: "United States",
      certifications: ["SOC 2 Type II", "ISO 27001", "GDPR", "EU–US DPF"],
      dpa_url: "https://docs.github.com/en/site-policy/privacy-policies/github-data-protection-agreement",
      last_audit: "2026-05-27",
    },
  ],
};
