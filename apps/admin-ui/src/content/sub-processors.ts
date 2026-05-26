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
  last_audit: string;
}

export interface SubProcessorList {
  version: string;
  items: SubProcessor[];
}

export const subProcessors: SubProcessorList = {
  version: "2026-05-14",
  items: [
    {
      id: "cloudflare",
      name: "Cloudflare, Inc.",
      role: "CDN, DDoS protection, edge TLS termination",
      region: "Global (anycast)",
      certifications: ["ISO 27001", "SOC 2 Type II", "ISO 27018"],
      last_audit: "2026-02-12",
    },
    {
      id: "clerk",
      name: "Clerk, Inc.",
      role: "Identity provider — authentication, SSO, MFA",
      region: "United States (us-east-1)",
      certifications: ["SOC 2 Type II"],
      last_audit: "2026-01-30",
    },
    {
      id: "stripe",
      name: "Stripe, Inc.",
      role: "Payment processing and billing",
      region: "United States, Ireland",
      certifications: ["PCI DSS Level 1", "SOC 2 Type II", "ISO 27001"],
      last_audit: "2026-03-04",
    },
  ],
};
