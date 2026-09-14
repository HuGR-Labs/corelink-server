# B-087 questionnaire reconciliation — owner packet

Engineering has reconciled the bounded procurement population in:

- `marketing/sales/legal-questionnaires/CAIQ-V4-pre-filled.md`
- `marketing/sales/legal-questionnaires/SIG-LITE-2026-pre-filled.md`

`scripts/verify_b087_questionnaires.py` is the semantic guard for the named rows.
It is intentionally compatible with B-156: B-156 remains the canonical inventory
of published `BYOK`, `Buck2`, and `pentest` mentions. This guard does not create a
second census; it checks only the procurement rows whose answer depends on the
shipped posture, with a closed population and mutation tests.

## Exact conflict map

| Surface | Engineering answer | Residue / owner action |
|---|---|---|
| BYOK rows (CAIQ CEK/BCR/DSP; SIG N/K) | The Dockerfile production build selects AWS KMS via `byok-aws-real`; this is code wiring, not a verified customer capability. The B-083 receipt has no protected test tenant, image digest, CMK access, activation, CAS/AC round-trip, revocation or p99 drill. Provider-construction or CMK-access failure maps to `501 byok_not_available`; activation is not an unconditional 501 route. | Legal must review the executed SLA's five-minute BYOK kill-switch promise. The residency amendment is a pending legal-review template, not an executed promise; reconcile it before any execution. This change amends neither instrument. |
| WORM / Object Lock (CAIQ LOG-03; SIG LOG-03) | R2 audit storage is tamper-evident, not proven immutable. The 2026-08-25 Object Lock probe returned `NotImplemented`; the latest 2026-09-09 probe was `INDETERMINATE` because credentials were rejected before capability testing. No WORM retention is evidenced in this deployment. | Legal must review the executed DPA's `immutable R2 with Object Lock` language. This change leaves the DPA untouched. |
| SAST, fuzz, and supply-chain rows | Answers now distinguish PR dependency gates, nightly CodeQL, dispatch-only Semgrep/fuzz, unsigned SBOM/provenance, checksums, and preserved signed-commit controls. | No owner action is implied by the wording; future capability claims require a new evidence review. |
| Synthetic paging (CAIQ SEF-03; SIG J.4) | Production has no synthetic cron; PagerDuty rotation is not repository-verifiable. | Operations should provide a PagerDuty schedule export before any 24×7 rotation claim is reused. |
| Superseded questionnaire copies | Current files are truthful and guarded. | Sales/Legal decide whether any prospect who received a superseded copy needs notification; no notification is claimed here. |

This packet records decisions still outside the engineering-closeable portion;
it is not evidence that an owner, customer, prospect, or regulator was contacted.

## Evidence handoff locations

Owners must attach the three independent records below without placing secrets,
contract bytes, or recipient PII in this repository:

- `reports/owner-actions/b170-legal-contract-review.md` — Legal's disposition
  of the executed DPA/SLA claims and the pending residency template before execution.
- `reports/owner-actions/b170-pagerduty-export.json` — Operations' redacted
  schedule/rotation export, including export time and account/workspace.
- `reports/owner-actions/b170-recipient-notification-decision.md` — Sales/Legal
  decision and, if applicable, redacted notification evidence for superseded
  copies.

The B-170 guard treats missing records as `open` and turns red when all three
appear, forcing content review before the backlog item can be closed.

## Proposed follow-up item (ID allocated by the canonical backlog owner)

Use the next dense backlog identifier; do not allocate one in this change:

```backlog-proposal
id: B-<next-dense-id>
repo: corelink-server
owner: owner
status: open
title: reconcile executed legal claims and external questionnaire recipients
scope: review the executed DPA Object Lock and SLA BYOK five-minute promises, reconcile the pending residency template before execution, obtain the PagerDuty rotation export, and decide whether recipients of superseded questionnaire copies require notice
evidence: owner/legal decision record; owner/ops PagerDuty export; owner/sales notification decision
non-claim: no contract amendment, export, notification, or customer/regulator contact is asserted until its owner records evidence
verify: manual — owner evidence is external to this repository
```
