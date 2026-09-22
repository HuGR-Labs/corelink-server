# Vendor Legal-Review Record — Functional Software, Inc. (Sentry)

> STATUS: TEMPLATE — pending the actual legal review record (owner/counsel to complete).

| Field | Value |
|---|---|
| Vendor | Functional Software, Inc. (Sentry) |
| Sub-processor id | `sentry` |
| Review date | `TBD (YYYY-MM-DD)` |
| Reviewer | `TBD (named Legal Counsel / Privacy Officer)` |
| DPA reference | <https://sentry.io/legal/dpa/> |
| DPA status | `TBD (signed copy pending — VR-7)` |
| SCC / transfer mechanism | `TBD` |
| Schrems II TIA | `TBD` |
| Data categories processed | telemetry |
| Data residency / region | United States |
| Sub-processor flow-down | `TBD (confirm flow-down per GDPR Art. 28(4))` |
| Certifications verified | `TBD (SOC 2 Type II report not yet pulled into Drata — VR-7)` |
| Review outcome | `TBD (approved / approved-with-conditions / rejected)` |
| Conditions / follow-ups | `VR-7 — Legal must obtain and record the signed-copy DPA and SOC 2 evidence.` |
| Next review due | `TBD (YYYY-MM-DD)` |

## Repository-verified technical and data-flow scope

- **Role:** application error monitoring for the admin UI and docs-site build.
- **Runtime flow:** `apps/admin-ui/sentry.server.config.ts` and
  `sentry.edge.config.ts` initialise Sentry only when `SENTRY_DSN` (or its
  public fallback) is present. Events use `sendDefaultPii: false` and pass
  through `beforeSend`, `beforeSendTransaction`, and `beforeBreadcrumb`; the
  shared scrubber removes secret/PII-shaped values before transmission.
- **Data boundary:** the repository classifies the resulting exception,
  breadcrumb, and diagnostic event stream as `telemetry`; no customer content
  flow is asserted by this packet.
- **Repository sources:** `apps/admin-ui/sentry.server.config.ts`,
  `apps/admin-ui/sentry.edge.config.ts`, `apps/admin-ui/src/lib/sentry-scrub.ts`,
  and registry row 20 in `specs/_compliance/VENDOR-RISK-REGISTER.md`.

## Notes

No signature, named review, transfer assessment, certification, or approval is
claimed. Legal owns VR-7 in `specs/_compliance/VENDOR-RISK-REGISTER.md` §5,
due 2026-09-24.

---

*Referenced by `legal/sub-processors.md`. Existence and pending-state integrity
are enforced by the B-316 verifier.*
