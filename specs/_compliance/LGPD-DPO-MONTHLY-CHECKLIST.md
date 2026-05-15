---
id: "LGPD-DPO-MONTHLY-CHECKLIST"
type: "compliance_checklist"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R5-3"
parent_wi: "GAP-22"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "LGPD-RESIDENCY-ATTESTATION-2026-05-15"
  - "PRIVACY-MODEL"
tags: ["lgpd", "dpo", "checklist", "monthly", "gap-22", "operational-cadence", "anpd"]
---

# LGPD DPO Monthly Checklist

> **doc_status:** DRAFT · **audit_status:** ACTIVE · **owner:** appointed Data Protection Officer (interim DPO formally designated 2026-05-15: Gustavo Schneiter; see `specs/_compliance/DPO-APPOINTMENT-2026-05-15.md`).
>
> **Purpose:** monthly operational review of the LGPD-relevant control surface, executed in the first 5 business days of each calendar month. Each item has a sign-off box; the completed checklist is filed to `specs/_audits/lgpd-dpo-checklist-YYYY-MM.md` with the executing DPO's name + date.
>
> **Cadence trigger:** D+1 of each month (reminder set via PagerDuty calendar). Skipped runs require a documented justification.
>
> **Companion docs:** `specs/_compliance/LGPD-RESIDENCY-ATTESTATION-2026-05-15.md` (the attestation this checklist sustains); `specs/03_architecture/privacy_model.md`; `legal/sub-processors.md`.

---

## How to use this checklist

1. Copy this file to `specs/_audits/lgpd-dpo-checklist-YYYY-MM.md` for the current month.
2. Work top to bottom; for each item, check the box and write the evidence URL/commit/issue link in the **Evidence** column.
3. If an item cannot be completed, mark `BLOCKED` + escalation owner + ETA.
4. Sign and date §end-of-month. Commit to `specs/_audits/`.

---

## A. Sub-processors (LGPD Art. 33 + Art. 39)

| # | Item | Done | Evidence |
|---|---|---|---|
| 1 | Review `legal/sub-processors.md` diff vs. last month. Any new sub-processors added without DPO sign-off? If yes, BLOCK and demand justification. | ☐ |   |
| 2 | For each `pending GAP-14 review` entry (Sentry, PostHog, LogRocket, Stripe Atlas counsel as of 2026-05-15), check status. Push for closure if D+60 ETA breached. | ☐ |   |
| 3 | Confirm `scripts/validate_sub_processors.py` passes on `main` (CI green). | ☐ |   |
| 4 | Spot-check 2 random sub-processors: are their DPA URLs still live? Has their LGPD-relevant SoC report been refreshed in the last 12 months? | ☐ |   |

## B. Residency enforcement (CTRL-PRIV-031)

| # | Item | Done | Evidence |
|---|---|---|---|
| 5 | Run `python3 scripts/verify-lgpd-residency.py --env production`. Confirm exit code 0. Attach output. | ☐ |   |
| 6 | Review production CloudEvents `dev.hugr.corelink.residency.write_rejected_cross_region.v1` count for the last month. Any spike > 10/day per tenant triggers tenant-side investigation. | ☐ |   |
| 7 | Review ANPD bulletins + Resolução CD/ANPD updates in the last month. Any new requirement that touches our Art. 33 §1º reliance? File an issue if so. | ☐ |   |
| 8 | Confirm `INV-DATA-RESIDENCY` 20k property test is still green on `main` (`cargo test -p corelink-privacy-residency-enforcement`). | ☐ |   |

## C. Audit-trail spot-check (LGPD Art. 37)

| # | Item | Done | Evidence |
|---|---|---|---|
| 9 | Pick 1 random `sam` tenant + sample 50 random audit records from the last month. Confirm timestamps monotonic, region tag = `sam`, CloudEvents schema validates. | ☐ |   |
| 10 | Pick 1 cross-region rejection (`write_rejected_cross_region.v1`) from the last month and trace it end-to-end: request → 451 response → audit record → SEV page (if applicable). | ☐ |   |
| 11 | Confirm `audit-sam` R2 bucket Object Lock retention is set to 7y (no manual override events in last 30d). | ☐ |   |

## D. DSR backlog review (LGPD Art. 18)

| # | Item | Done | Evidence |
|---|---|---|---|
| 12 | Pull DSR backlog from `corelink-privacy-dsr-erasure` / DSR self-service surface. List counts by type (acesso, correção, anonimização, portabilidade, revogação, oposição). | ☐ |   |
| 13 | Confirm no DSR > 30 days open (LGPD Art. 19 §1º + privacy_model.md §6 SLA table). Escalate any aging request. | ☐ |   |
| 14 | Review any tenant requests for per-tenant residency attestation extract (manual handover until admin-ui residency tile ships). SLA: 5 business days. | ☐ |   |
| 15 | Sample 3 completed DSR-erasure flows; confirm `EVT-042` + `EVT-017` emitted and all backends purged per `privacy_model.md §8.2`. | ☐ |   |

## E. Consent + legal basis (LGPD Art. 7 / Art. 8)

| # | Item | Done | Evidence |
|---|---|---|---|
| 16 | Sample 5 random consent records from `EVT-049` events; confirm each carries `purpose_tag`, `legal_basis`, `language`, `notice_version`. | ☐ |   |
| 17 | Confirm `legal/lia/` LIA documents are within annual refresh window (per `privacy_model.md §9.2`). Flag any > 12 months stale. | ☐ |   |
| 18 | Confirm `legal/privacy-notice/` pt-BR version is current (translation track H-16 — flag if stale). | ☐ |   |

## F. Breach + incident (LGPD Art. 48)

| # | Item | Done | Evidence |
|---|---|---|---|
| 19 | Review any P-BREACH or P-DSR incident from the last month. Did response stay within the 48h-internal / 72h-legal envelope (privacy_model.md §11)? | ☐ |   |
| 20 | Confirm `RB-BREACH-NOTIF.md` dry-run is scheduled (semestral cadence; verify quarter alignment). | ☐ |   |
| 21 | Confirm `RB-DATA-RESIDENCY-LEAK.md` is reviewed annually; flag if review date > 12 months. | ☐ |   |

## G. Documentation + training

| # | Item | Done | Evidence |
|---|---|---|---|
| 22 | Confirm `LGPD-RESIDENCY-ATTESTATION-YYYY-MM-DD.md` is within its 90-day refresh window. If breaching, schedule attestation refresh sprint. | ☐ |   |
| 23 | Confirm all engineers with prod access have completed annual LGPD training (R5-8 advisor pool tracks; HR system of record). | ☐ |   |
| 24 | Customer-facing transparency: confirm `apps/docs/docs/explanation/residency/lgpd-brazil.mdx` is still accurate (no drift vs. attestation §3). | ☐ |   |
| 25 | Final: review previous month's checklist exceptions. Any item that was `BLOCKED` last month — is it resolved? If still blocked > 2 months, escalate to Security Lead. | ☐ |   |

---

## Month sign-off

| Field | Value |
|---|---|
| Calendar month | _YYYY-MM_ |
| Executed by | _DPO name_ |
| Date completed | _YYYY-MM-DD_ |
| Items completed | _XX / 25_ |
| Items BLOCKED | _list, with ETA_ |
| Items escalated | _list, with owner_ |
| Filed to | `specs/_audits/lgpd-dpo-checklist-YYYY-MM.md` |
| Signature | _________________ |

---

## Cross-references

- **Attestation bundle this checklist sustains:** `specs/_compliance/LGPD-RESIDENCY-ATTESTATION-2026-05-15.md`
- **Privacy model (canonical):** `specs/03_architecture/privacy_model.md`
- **Sub-processors register:** `legal/sub-processors.md`
- **Verifier script:** `scripts/verify-lgpd-residency.py`
- **Residency-leak runbook:** `specs/_runbooks/RB-DATA-RESIDENCY-LEAK.md`
- **Breach notification runbook:** `legal/breach-notification/`
