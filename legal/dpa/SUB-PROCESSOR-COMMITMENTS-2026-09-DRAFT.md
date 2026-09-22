---
document_type: "sub_processor_commitments_draft"
version: "draft-2026-09"
effective: false
published: false
approval_status: "Pending Legal approval"
source_register: "specs/_compliance/VENDOR-RISK-REGISTER.md"
effective_artifact: "legal/dpa/SUB-PROCESSOR-COMMITMENTS.md"
---

# Proposed CoreLink Sub-Processor Commitments — private draft

> **DRAFT — NOT EFFECTIVE, NOT PUBLISHED, and not customer-facing.** This
> working document records the repository-owned target for Legal review. It
> does not amend `legal/dpa/SUB-PROCESSOR-COMMITMENTS.md`, the DPA, the SCC
> schedule, `legal/sub-processors.md`, or the public Trust Center.

## Proposed active population

Subject to Legal's approval of the four pending packets and the applicable
30-day notification process, the target population is exactly these nine
vendors:

| ID | Vendor | Repository role / data scope | Evidence path |
|---|---|---|---|
| cloudflare | Cloudflare, Inc. | Infrastructure; metadata, encrypted blobs, audit logs, telemetry | `docs/compliance/vendor-reviews/cloudflare-dpa-review-2026-04.md` |
| clerk | Clerk, Inc. | Authentication and identity; account PII | `docs/compliance/vendor-reviews/clerk-dpa-review-2026-04.md` |
| resend | Resend, Inc. | Email delivery; recipient email PII | `docs/compliance/vendor-reviews/resend-dpa-review-2026-09.md` |
| stripe | Stripe, Inc. | Billing and payment processing | `docs/compliance/vendor-reviews/stripe-dpa-review-2026-04.md` |
| github | GitHub, Inc. | Source repository and CI/CD; source and CI artifacts | `docs/compliance/vendor-reviews/github-dpa-review-2026-04.md` |
| pagerduty | PagerDuty, Inc. | Incident management and on-call alerting | `docs/compliance/vendor-reviews/pagerduty-dpa-review-2026-04.md` |
| sentry | Functional Software, Inc. (Sentry) | Scrubbed application diagnostic telemetry | `docs/compliance/vendor-reviews/sentry-dpa-review-2026-09.md` |
| plausible | Plausible Insights OÜ (Plausible Analytics) | Cookieless docs analytics telemetry | `docs/compliance/vendor-reviews/plausible-dpa-review-2026-09.md` |
| betterstack | Better Stack, Inc. (BetterStack / Statuspage) | Synthetic monitoring telemetry and public status page | `docs/compliance/vendor-reviews/betterstack-dpa-review-2026-09.md` |

## Proposed change to the effective text

- Remove the current `Neon, Inc. (optional / tenant-selectable Postgres)`
  section from the effective commitments only after Legal approves the change.
- Add the six currently missing target sections: Resend, GitHub, PagerDuty,
  Sentry, Plausible, and Better Stack.
- Keep this draft private and non-effective until Legal records dated reviews,
  signed-copy evidence, transfer decisions, and any required contract dates in
  the canonical packets. Packet existence is not approval or execution evidence.

## Change control

The canonical effective artifact remains
`legal/dpa/SUB-PROCESSOR-COMMITMENTS.md` with its current four-vendor
population. Publishing or applying this proposal requires a separate
Legal-approved version, the contractual 30-day notice workflow, and the
corresponding public disclosure update. This draft intentionally makes no
public Trust Center change.
