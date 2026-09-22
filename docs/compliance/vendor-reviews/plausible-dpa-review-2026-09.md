# Vendor Legal-Review Record — Plausible Insights OÜ (Plausible Analytics)

> STATUS: TEMPLATE — pending the actual legal review record (owner/counsel to complete).

| Field | Value |
|---|---|
| Vendor | Plausible Insights OÜ (Plausible Analytics) |
| Sub-processor id | `plausible` |
| Review date | `TBD (YYYY-MM-DD)` |
| Reviewer | `TBD (named Legal Counsel / Privacy Officer)` |
| DPA reference | <https://plausible.io/dpa> |
| DPA status | `TBD (signed copy pending — VR-8)` |
| SCC / transfer mechanism | `TBD` |
| Schrems II TIA | `TBD` |
| Data categories processed | telemetry |
| Data residency / region | European Union |
| Sub-processor flow-down | `TBD (confirm flow-down per GDPR Art. 28(4))` |
| Certifications verified | `TBD (SOC 2 / attestation posture pending — VR-8)` |
| Review outcome | `TBD (approved / approved-with-conditions / rejected)` |
| Conditions / follow-ups | `VR-8 — Legal must obtain and record the signed-copy DPA and attestation disposition.` |
| Next review due | `TBD (YYYY-MM-DD)` |

## Repository-verified technical and data-flow scope

- **Role:** cookieless web analytics for the docs-site marketing funnel.
- **Runtime flow:** `apps/docs/docusaurus.config.ts` injects the external
  `https://plausible.io/js/script.js` with `data-domain:
  corelink-docs.humangr.com`. The repository records this as aggregate page
  view telemetry and documents the no-cookie/no-fingerprinting configuration.
- **Optional server flow:** `apps/analytics-worker/wrangler.toml` declares
  `PLAUSIBLE_API_KEY` as an optional secret for the weekly digest; absence of
  that secret omits Plausible totals rather than failing the cron path.
- **Repository sources:** `apps/docs/docusaurus.config.ts`,
  `apps/analytics-worker/wrangler.toml`, and registry row 21 in
  `specs/_compliance/VENDOR-RISK-REGISTER.md`.

## Notes

No signature, named review, transfer assessment, certification, or approval is
claimed. Legal owns VR-8 in `specs/_compliance/VENDOR-RISK-REGISTER.md` §5,
due 2026-09-24.

---

*Referenced by `legal/sub-processors.md`. Existence and pending-state integrity
are enforced by the B-316 verifier.*
