---
id: "PUBLIC-VENDOR-ATTESTATIONS-2026-09"
type: "evidence_snapshot"
status: "informational_only"
captured_at: "2026-09-08"
captured_by: "CoreLink owner remediation run (automated public-source capture)"
scope: ["B-032", "B-316"]
---

# Public vendor-attestation snapshot — 2026-09-08

This is a dated, attributable snapshot of first-party public material. It is
useful input to the vendor review, but it is **not** a Legal review, a signed
DPA, a SOC 2 report obtained under an account/NDA, a Drata workspace export, or
an approval to change the effective commitments text. B-032 and B-316 remain
open until their packets' external actions are completed.

## Sources captured

| Vendor | First-party source | Public fact observed | Boundary |
|---|---|---|---|
| Cloudflare | <https://www.cloudflare.com/trust-hub/compliance-resources/> | Trust Hub lists ISO 27001:2022 and SOC 2 Type II; detailed reports are account/dashboard material. | No CoreLink account export or current report was retrieved. |
| Stripe | <https://docs.stripe.com/security> | Stripe says SOC 1 and SOC 2 Type II reports are produced annually and that SOC 3 is public. | The page is not a CoreLink vendor-review outcome or signed-DPA evidence. |
| Clerk | <https://clerk.com/security> | Clerk states SOC 2 Type 2 is held; it expressly says Clerk does not hold ISO 27001 and report access is plan/support gated. | No report or executed CoreLink DPA was retrieved. |
| AWS | <https://aws.amazon.com/compliance/soc-faqs/> | AWS states SOC 2 is available through AWS Artifact and SOC 3 is public; SOC 2 access is customer/NDA gated. | No AWS Artifact report or bridge letter was retrieved. |
| Google Cloud | <https://cloud.google.com/security/compliance/soc-2> | Google Cloud states core SOC 2 Type II reports are issued quarterly through Compliance Reports Manager. | No CoreLink Compliance Reports Manager export was retrieved. |
| Microsoft Azure | <https://learn.microsoft.com/en-us/compliance/regulatory/offering-soc-2> | Microsoft identifies Azure in the SOC 2 Type 2 attestation scope. | No Service Trust Portal report or current Azure review was retrieved. |
| Drata | <https://trust.drata.com/> | Drata's public Trust Center lists SOC 2 Type 2, SOC 3, ISO/IEC 27001:2022 and other programs for Drata itself. | This does not expose CoreLink's Drata workspace or its seven vendor records; no `DRATA_API_KEY` was available in this run. |
| Resend | <https://resend.com/legal/dpa> and <https://resend.com/security> | Resend's DPA page says it was updated 2026-08-27; its security page publicly states SOC 2 and GDPR posture. | Public policy is not a signed-copy DPA for CoreLink; VR-6 remains open. |
| Sentry | <https://sentry.io/astro-assets/resources/legal/how-to-comply-with-gdpr-october-2024.pdf> | Sentry's first-party GDPR material says current SOC 2 Type 2 and ISO 27001 reports are available in the customer legal/compliance area. | No customer-area report or signed CoreLink DPA was retrieved; VR-7 remains open. |
| Plausible | <https://plausible.io/docs/compliance> and <https://plausible.io/security> | Plausible says its DPA applies automatically, data stays in the EU, and its security page was updated in 2026-08; its public compliance material does not establish SOC 2. | Public DPA/policy is not a CoreLink signed-copy artifact; VR-8 remains open. |
| Better Stack | <https://betterstack.com/security> | Better Stack publicly states SOC 2 Type 2 and GDPR posture and says the current report is available on request/NDA. | No report, signed CoreLink DPA, or Legal review was retrieved; VR-9 remains open. |

## Retrieval and review limits

- URLs above were fetched on 2026-09-08 UTC from the vendors' own domains.
- A public statement is recorded as a statement by the vendor, not as a fact
  independently attested by CoreLink.
- The snapshot does not fill any `TBD`, set `contract_signed_at`, close VR-6
  through VR-9, close B-032, or authorize a version of
  `legal/dpa/SUB-PROCESSOR-COMMITMENTS.md`.
- The next action for B-032 is still an owner-authorized Drata credential and
  counsel/compliance review. The next action for B-316 is still the four
  attributable Legal reviews, signed-copy evidence, and Legal approval of the
  nine-vendor effective commitments text.
