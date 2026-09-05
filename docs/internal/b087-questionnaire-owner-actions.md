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
| BYOK rows (CAIQ CEK/BCR/DSP; SIG N/K) | BYOK is unavailable; activation fails closed with `501 byok_not_available`; default provider is `InMemoryFake`. | Legal must review the executed SLA's five-minute BYOK kill-switch promise and the executed residency amendment's five-minute crypto-erase promise. This change does not amend either instrument. |
| WORM / Object Lock (CAIQ LOG-03; SIG LOG-03) | R2 audit storage is tamper-evident, not immutable; R2 Object Lock is unavailable. | Legal must review the executed DPA's `immutable R2 with Object Lock` language. This change leaves the DPA untouched. |
| SAST, fuzz, and supply-chain rows | Answers now distinguish PR dependency gates, nightly CodeQL, dispatch-only Semgrep/fuzz, unsigned SBOM/provenance, checksums, and preserved signed-commit controls. | No owner action is implied by the wording; future capability claims require a new evidence review. |
| Synthetic paging (CAIQ SEF-03; SIG J.4) | Production has no synthetic cron; PagerDuty rotation is not repository-verifiable. | Operations should provide a PagerDuty schedule export before any 24×7 rotation claim is reused. |
| Superseded questionnaire copies | Current files are truthful and guarded. | Sales/Legal decide whether any prospect who received a superseded copy needs notification; no notification is claimed here. |

This packet records decisions still outside the engineering-closeable portion;
it is not evidence that an owner, customer, prospect, or regulator was contacted.
