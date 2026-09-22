# Vendor Legal-Review Record — Resend, Inc.

> STATUS: TEMPLATE — pending the actual legal review record (owner/counsel to complete).

| Field | Value |
|---|---|
| Vendor | Resend, Inc. |
| Sub-processor id | `resend` |
| Review date | `TBD (YYYY-MM-DD)` |
| Reviewer | `TBD (named Legal Counsel / Privacy Officer)` |
| DPA reference | <https://resend.com/legal/dpa> |
| DPA status | `TBD (signed copy pending — VR-6)` |
| SCC / transfer mechanism | `TBD` |
| Schrems II TIA | `TBD` |
| Data categories processed | recipient_email_pii |
| Data residency / region | United States |
| Sub-processor flow-down | `TBD (confirm flow-down per GDPR Art. 28(4))` |
| Certifications verified | `TBD` |
| Review outcome | `TBD (approved / approved-with-conditions / rejected)` |
| Conditions / follow-ups | `VR-6 — Legal must obtain and record the signed-copy DPA; public policy is not execution evidence.` |
| Next review due | `TBD (YYYY-MM-DD)` |

## Repository-verified technical and data-flow scope

- **Role:** transactional email and newsletter-audience delivery.
- **Runtime flow:** `apps/admin-ui/src/app/api/newsletter/subscribe/route.ts`
  accepts a newsletter email, reads `RESEND_API_KEY` and
  `RESEND_NEWSLETTER_AUDIENCE_ID` only on the server, and sends an HTTPS
  `POST` to `api.resend.com/audiences/{audience}/contacts`. The payload contains
  the recipient email and `unsubscribed: false`; source and locale are bounded
  non-PII tags. The route never returns the upstream response or logs the email.
- **Additional flow:** `apps/analytics-worker/wrangler.toml` declares the same
  `RESEND_API_KEY` for the Monday digest email. The secret is not committed to
  the repository.
- **Repository sources:** `apps/admin-ui/src/app/api/newsletter/subscribe/route.ts`;
  `apps/analytics-worker/wrangler.toml`; registry row 16 in
  `specs/_compliance/VENDOR-RISK-REGISTER.md`.

## Notes

No signature, named review, transfer assessment, certification, or approval is
claimed. Legal owns VR-6 in `specs/_compliance/VENDOR-RISK-REGISTER.md` §5,
due 2026-09-23.

---

*Referenced by `legal/sub-processors.md`. Existence and pending-state integrity
are enforced by the B-316 verifier.*
