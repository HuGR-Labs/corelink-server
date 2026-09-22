# Vendor Legal-Review Record — Better Stack, Inc. (BetterStack / Statuspage)

> STATUS: TEMPLATE — pending the actual legal review record (owner/counsel to complete).

| Field | Value |
|---|---|
| Vendor | Better Stack, Inc. (BetterStack / Statuspage) |
| Sub-processor id | `betterstack` |
| Review date | `TBD (YYYY-MM-DD)` |
| Reviewer | `TBD (named Legal Counsel / Privacy Officer)` |
| DPA reference | <https://betterstack.com/privacy> |
| DPA status | `TBD (signed copy pending — VR-9)` |
| SCC / transfer mechanism | `TBD` |
| Schrems II TIA | `TBD` |
| Data categories processed | telemetry |
| Data residency / region | European Union |
| Sub-processor flow-down | `TBD (confirm flow-down per GDPR Art. 28(4))` |
| Certifications verified | `TBD (SOC 2 / attestation posture pending — VR-9)` |
| Review outcome | `TBD (approved / approved-with-conditions / rejected)` |
| Conditions / follow-ups | `VR-9 — Legal must obtain and record the signed-copy DPA and attestation disposition.` |
| Next review due | `TBD (YYYY-MM-DD)` |

## Repository-verified technical and data-flow scope

- **Role:** uptime/status monitoring, synthetic probes, and public status page
  hosting.
- **Runtime flow:** `monitoring/synthetic/probes.yml` defines seven HTTP health
  probes against CoreLink's own public endpoints. The docs StatusPill fetches
  the vendor status page's `index.json` with `GET` and `credentials: omit`,
  then renders status data; it does not send visitor data, cookies, or
  fingerprints to the vendor.
- **Data boundary:** probe results are HTTP status codes and JSON assertions;
  the repository classifies this stream as `telemetry` and records no customer
  PII or content transfer.
- **Repository sources:** `monitoring/synthetic/probes.yml`,
  `apps/docs/src/components/StatusPill/StatusPill.tsx`,
  `apps/docs/src/statuspage-url.ts`, and registry row 22 in
  `specs/_compliance/VENDOR-RISK-REGISTER.md`.

## Notes

No signature, named review, transfer assessment, certification, or approval is
claimed. Legal owns VR-9 in `specs/_compliance/VENDOR-RISK-REGISTER.md` §5,
due 2026-09-24.

---

*Referenced by `legal/sub-processors.md`. Existence and pending-state integrity
are enforced by the B-316 verifier.*
