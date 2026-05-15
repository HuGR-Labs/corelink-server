---
id: "RB-DSR-GDPR"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R5-3"
parent_wi: "GAP-22-FOLLOWUP-FULL-AUDIT"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "GDPR-FULL-AUDIT-2026-05-15"
  - "RB-DSR-LGPD-FULL"
  - "PRIVACY-MODEL"
tags: ["runbook", "gdpr", "gdpr-art-12", "gdpr-art-15", "gdpr-art-16", "gdpr-art-17", "gdpr-art-18", "gdpr-art-19", "gdpr-art-20", "gdpr-art-21", "gdpr-art-22", "dsr", "data-subject-rights"]
---

# RB-DSR-GDPR — Internal runbook for GDPR Art. 12-22 DSR processing

> **Status:** DRAFT. Owner: interim Privacy Officer (Gustavo Schneiter); transferred to formal DPO post-appointment.
>
> **Scope:** end-to-end processing of every GDPR Chapter III data-subject right (Art. 15–22) plus Art. 7(3) consent withdrawal, through CoreLink's DSR pipeline (`crates/corelink-dsr/` + 12-backend canonical erasure propagation per `privacy_model.md §6.2`). Sister runbook to `RB-DSR-LGPD-FULL.md` — most internal procedures are shared; this doc documents the **GDPR-specific deltas** (Art. 12 modalities, Art. 19 notification obligation, Art. 21(2) marketing absolute right, Art. 22 N/A declaration, EU supervisory authority escalation).
>
> **Companion docs:**
> - `specs/_compliance/GDPR-FULL-AUDIT-2026-05-15.md` §1.7 (article-by-article rights mapping)
> - `specs/_compliance/GDPR-SCC-EXECUTION-2026-05-15.md` (sub-processor / international transfer context)
> - `specs/_compliance/LGPD-ROPA-2026-05-15.md` (RoPA-7 DSR ticket processing row — same row covers both regimes)
> - `specs/_runbooks/RB-DSR-LGPD-FULL.md` (LGPD parallel runbook — shared internal procedures)
> - `specs/03_architecture/privacy_model.md` §6 (canonical DSR pipeline)
> - `apps/docs/docs/explanation/privacy/gdpr.mdx` (customer-facing summary)
> - `crates/corelink-dsr/` (runtime)
> - `crates/corelink-privacy-erasure-worker/` (12-backend propagation)
>
> **Trigger:** any inbound DSR request from a person within scope of GDPR Art. 3, whether via API (`POST /v1/privacy/dsr/{kind}`), admin UI (deferred to WI-S13-* track), or email to `privacy@hugr.com`.

---

## 1. Right-type taxonomy + decision tree

GDPR Chapter III enumerates 8 rights (Art. 15–22) plus the withdraw-consent right (Art. 7(3)). CoreLink's canonical `DsrRequestKind` enum (`crates/corelink-dsr/src/event.rs`) maps these to 6 destructive + 3 informational arms, with the `Restriction` and `Notification` arms newly canonicalised for GDPR symmetry.

| Request from customer | GDPR Art. ref | CoreLink `DsrRequestKind` | Endpoint | SLA | MFA required? | Audit-fail-CLOSED? |
|---|---|---|---|---|---|---|
| "Do you have data on me?" / "Give me a copy" | Art. 15 (Access) | `Access` | `POST /v1/privacy/dsr/access` | 5 BD confirmation / 15 BD full export | No | YES |
| "Fix this incorrect field" | Art. 16 (Rectification) | `Rectification` | `POST /v1/privacy/dsr/rectification` | 5 BD | **YES** | YES |
| "Delete my data" | Art. 17 (Erasure / RTBF) | `Erasure` | `POST /v1/privacy/dsr/erasure` | 30 cal d | **YES** | YES |
| "Pause processing on this purpose pending dispute" | Art. 18 (Restriction) | `Restriction` | `POST /v1/privacy/dsr/restriction` | 5 BD | No | YES |
| "Notify the third parties that received my data of the change" | Art. 19 (Notification obligation) | (auto-triggered by Art. 16/17/18) | Internal (no separate endpoint) | Same as triggering right | N/A (inherits) | YES |
| "Give me my data so I can move it elsewhere" | Art. 20 (Portability) | `Portability` | `POST /v1/privacy/dsr/portability` | 15 BD | No | YES |
| "I oppose your legitimate-interest processing" | Art. 21(1) (Objection) | `Objection` | `POST /v1/privacy/dsr/objection` | 15 BD | No | YES |
| "Unsubscribe from marketing" | Art. 21(2) (Objection — absolute for direct marketing) | `Objection` (marketing scope) | `DELETE /v1/consent/marketing_email` | **Immediate (≤ 5 min)** | No | YES |
| "I want out of automated decision-making" | Art. 22 | N/A — **no Art. 22 processing at CoreLink** (return Art. 13(2)(f) disclosure inline) | Email reply | 5 BD | No | YES (disclosure event) |
| "Withdraw my consent" | Art. 7(3) | (consent ledger revoke) | `DELETE /v1/consent/<purpose>` | **Immediate (≤ 5 min)** | No | YES |

### 1.1 Decision tree on intake

```
[ Inbound request ]
        |
        v
[ Q1: Is requester within GDPR Art. 3 scope? ]
   |-- NO (not in EU + not on EU-targeted service surface)
   |    -> Route to LGPD pipeline (RB-DSR-LGPD-FULL.md) if BR subject;
   |       otherwise apply best-effort under terms of service.
   v
   YES
        |
        v
[ Q2: Is requester a CoreLink user OR a subject in a tenant's blob? ]
   |-- Tenant's blob -> Route to controlling tenant per DPA Art. 28;
   |                    CoreLink is processor, not controller.
   |                    Notify subject of routing within 5 BD.
   |                    Audit "operator-pass-through" event.
   v
   CoreLink user
        |
        v
[ Q3: Identity verified (MFA re-auth)? ]
   |-- NO  -> Issue verification challenge (WebAuthn step-up).
   |          Clock NOT started yet (per INV-DSR-VERIFIED-CLOCK).
   v
   YES (status = verified)
        |
        v
[ Q4: Which Art. 15-22 / Art. 7(3) right? ]
        |
        v
[ Q5: Is this a manifestly unfounded or excessive request (Art. 12(5))? ]
   |-- YES -> Reject with reasoning + complaint-right disclosure.
   v          DPO countersign required for rejection.
   NO
        |
        v
[ Process the right per §2 procedures below ]
```

---

## 2. Per-right end-to-end runbook

### 2.1 Art. 15 — Right of access (5 BD confirmation / 15 BD full export)

Two scopes: (a) confirmation that we process data ("yes / no"); (b) full export.

**Confirmation scope:**

1. Receive `POST /v1/privacy/dsr/access` with `{"scope": "confirmation"}` or human email.
2. Verify identity (WebAuthn step-up; clock starts on `verified`).
3. Audit `dsr.request_received.v1` BEFORE state mutation (INV-DSR-AUDIT-FAIL-CLOSED).
4. Query: presence check across the 14 RoPA dataflows (`crates/corelink-dsr/src/store.rs::has_data`).
5. Audit `dsr.access_completed.v1` with `scope = confirmation`.
6. Issue receipt JWT (RS256, KMS-rooted, 90d anti-replay).
7. Return `{"has_data": <bool>, "request_id": "<uuid7>", "receipt": "<jwt>"}`.
8. Email customer with confirmation + receipt.

**Full export scope:** see §2.1 of `RB-DSR-LGPD-FULL.md` (identical procedure; bundle includes Art. 15(1)(a–h) mandatory metadata: purposes, categories, recipients, retention, source if Art. 14, automated-decision-making disclosure).

**Failure paths:** if any audit emit fails, abort fail-CLOSED with no state change. Customer receives a 500 with retry guidance and `Retry-After`.

### 2.2 Art. 16 — Right to rectification (5 BD SLA, MFA REQUIRED)

Procedure identical to `RB-DSR-LGPD-FULL.md` §2.3. Additional GDPR step: trigger Art. 19 notification (see §2.5).

### 2.3 Art. 17 — Right to erasure / RTBF (30 cal d SLA, MFA REQUIRED)

Procedure identical to `RB-DSR-LGPD-FULL.md` §2.4 (12-backend canonical propagation per `privacy_model.md §6.2`). Additional GDPR considerations:

- **Art. 17(3) exceptions** — erasure does NOT apply when:
  - Art. 17(3)(a) exercise of freedom of expression and information — not applicable to CoreLink.
  - Art. 17(3)(b) compliance with legal obligation requiring processing — applies to billing records under member-state fiscal law (Art. 16 §3 LGPD equivalent + DE/FR ~10y obligation). Subject is informed which records remain under legal hold and why.
  - Art. 17(3)(c) public interest in public health — N/A.
  - Art. 17(3)(d) archival/research — N/A.
  - Art. 17(3)(e) establishment / exercise of legal claims — applies on subpoena (rare).
- **Pseudonymised retention** of audit chain + backups + compliance evidence is documented to subject in the completion email; this satisfies Art. 17(3)(b) compliance.
- **Art. 19 notification** is triggered automatically (see §2.5).

### 2.4 Art. 18 — Right to restriction (5 BD SLA)

Restriction pauses processing for a purpose without deleting data. Use cases: subject contests accuracy (pending Art. 16); processing is unlawful but subject opposes erasure; controller no longer needs data but subject needs it for legal claims; subject objected pending Art. 21(1) verification.

1. Receive `POST /v1/privacy/dsr/restriction` with `{"purpose": "<tag>", "reason": "<enum>"}`.
2. Verify identity. Clock starts.
3. Audit `dsr.restriction_requested.v1`.
4. Mutate consent ledger: purpose enters `restricted` state (similar to `consent_lapsed` but reversible on resolution).
5. Propagate to downstream sub-processors per recipient manifest (Art. 19).
6. Audit `dsr.restriction_completed.v1`.
7. Email customer with restriction-active confirmation + receipt + estimated review timeline.

**Edge case:** restriction cannot be applied to `regulatory_compliance` purposes (legal obligation overrides). Reject with explanation referencing Art. 6(1)(c) + the controlling law.

### 2.5 Art. 19 — Notification obligation (automatic; same SLA as triggering right)

Triggered by Art. 16 (Rectification), Art. 17 (Erasure), Art. 18 (Restriction). Notifies each recipient sub-processor that received the subject's data of the change.

1. Triggering right emits its completion event (`dsr.rectification_completed.v1` / `dsr.erasure_completed.v1` / `dsr.restriction_completed.v1`).
2. Notification dispatcher reads the recipient manifest for the affected subject (cross-reference RoPA).
3. For each recipient: emit a notification per the recipient's API or webhook (Stripe `Customer.update`; Clerk webhook; Grafana log-deletion API; etc.).
4. Audit `dsr.notification_propagated.v1` per recipient with status (success / partial / failed-but-documented).
5. If any recipient cannot be notified (Art. 19 second sentence: "unless this proves impossible or involves disproportionate effort"), document the reason and notify the subject in the completion email.
6. Subject receives a final manifest of recipients-notified + recipients-where-impossible.

**Edge case:** for recipients in non-adequate countries where notification might be legally constrained, escalate to DPO before propagation.

### 2.6 Art. 20 — Right to portability (15 BD SLA)

Identical to `RB-DSR-LGPD-FULL.md` §2.5. The portability bundle is structured JSON + Parquet with the **canonical portability schema** (`legal/portability-schema/v1.json`); only CoreLink-controlled subject data is included (not tenant-controlled blob content — for that, the tenant is responsible).

GDPR-specific: bundle includes only data processed under (a) consent or (b) contract bases (Art. 20(1)(a/b)); legitimate-interest processed data is excluded from portability (but available under Art. 15 Access).

### 2.7 Art. 21(1) — Right to object (legitimate-interest processing; 15 BD SLA)

Identical to `RB-DSR-LGPD-FULL.md` §2.9. DPO reviews the LIA balancing; outcome ACCEPT (opt-out) or REJECT (reasoned, citing LIA).

**Edge case:** objection to `regulatory_compliance` purpose is not honourable (legal obligation overrides). Reject with explanation pointing to Art. 6(1)(c) + the controlling law.

### 2.8 Art. 21(2) — Right to object to direct marketing (IMMEDIATE — absolute right)

This is **absolute** — no balancing test, no DPO review. Subject simply opts out.

1. Receive `DELETE /v1/consent/marketing_email` (or email, or "unsubscribe" link click).
2. Verify identity (lightweight: existing session sufficient).
3. Audit `consent.revoked.v1` with full proof bundle.
4. Propagate in ≤ 5 min: update consent ledger; invalidate KV cache; notify downstream marketing tool.
5. Audit `consent.revocation_propagated.v1`.
6. Return success to customer + final email confirmation.

**No fail-open:** marketing processing stops immediately; basis does NOT degrade.

### 2.9 Art. 22 — Disclosure-only handler (no Art. 22 processing at CoreLink)

If a subject invokes Art. 22:

1. Receive email or API request.
2. Audit `dsr.art22_inquiry_received.v1`.
3. Reply (within 5 BD) with the Art. 13(2)(f) disclosure: "CoreLink does not engage in automated decision-making with legal or similarly significant effect. The closest activity is the `training_ml_models` opt-in purpose; details in /privacy/gdpr §7."
4. Audit `dsr.art22_inquiry_completed.v1`.

If a future feature crosses the Art. 22 threshold, this section is rewritten and a DPIA is mandatory (GDPR-DPIA-LIBRARY §2.1 trigger criteria).

### 2.10 Art. 7(3) — Withdraw consent (IMMEDIATE)

Identical to `RB-DSR-LGPD-FULL.md` §2.8. "As easy to withdraw as to give" — verified by `EVT-024` (load-test parity between opt-in and revoke latency).

---

## 3. Art. 12 modalities — cross-cutting

Art. 12 sets the meta-rules for all the above. Procedurally:

| Modality | Operational expression |
|---|---|
| Concise / intelligible / plain language | All endpoints return structured JSON + human email; customer-facing copy reviewed quarterly by DPO |
| Free of charge | All endpoints free; manifestly-unfounded gate (§4) is the only exception, used sparingly |
| One-month statutory cap | All SLAs above are inside the cap; complex requests get the 2-month extension with reasoned notice |
| Identity verification (Art. 12(6)) | WebAuthn step-up for Erasure + Rectification; session re-auth for Access + Portability + Restriction + Objection |
| Refusal — reasoned + complaint-right | `dsr.rejected.v1` event carries reasoning; rejection email points to subject's local SA |

---

## 4. Manifestly unfounded or excessive (Art. 12(5)) — strict gate

GDPR allows refusing or charging a "reasonable fee" for manifestly unfounded or excessive requests. **CoreLink applies this gate extremely sparingly:**

| Scenario | Treatment |
|---|---|
| Same subject submits >5 identical Access requests within 30 days | Treat as excessive; first one fulfilled; subsequent ones rejected with explanation + DPO countersign |
| Request requires manual joining of pre-Art.20 legacy data outside our 14 RoPA rows | Considered "manifestly unfounded" only if no reasonable engineering path exists — DPO countersign + Legal countersign required |
| Request from a non-subject impersonating a subject | Reject with "no relationship found"; do not confirm or deny identity of others (CTRL-PRIV-014) |
| Request via a third-party representative without valid mandate | Request mandate proof; pause clock until provided |

**Sign-off bar:** every manifestly-unfounded or excessive determination requires **DPO countersign + Legal countersign**. Audit `dsr.rejected_unfounded.v1` includes both signatures.

---

## 5. Special cases (parallel to LGPD)

### 5.1 Tenant-blob subject (CoreLink is processor)

Identical to `RB-DSR-LGPD-FULL.md` §3.1. Route the DSR to the controlling tenant within 5 BD; audit `dsr.operator_pass_through.v1`. Notify subject with the tenant identity + their 30-day SLA + their EU SA contact if applicable.

### 5.2 Subject under legal hold

Identical to LGPD §3.2. Pause affected scope; continue un-held scope. Document Art. 17(3)(b) / (e) basis.

### 5.3 Cross-subject privacy

Identical to LGPD §3.3. Redact references to other subjects in export bundles.

### 5.4 EU supervisory authority inquiry on a DSR

If a member-state SA inquires about a specific DSR (request_id received via formal Art. 58 inquiry):

1. Route to legal counsel + DPO immediately (SEV-2).
2. Retrieve full audit chain for the DSR request_id (Object Lock 7y).
3. Build response packet using `specs/_runbooks/RB-REGULATOR-INQUIRY.md` (to be added; for now: ad-hoc using LGPD-parallel structure).
4. Response SLA: per SA's stated deadline (typically 30 days under Art. 58(1)).
5. If the inquiry concerns a cross-border processing, route through the lead SA per the one-stop-shop mechanism (Art. 56) — requires EU representative appointed (currently pending; ad-hoc multi-SA response until then).

### 5.5 Cross-regime overlap (subject claims both GDPR + LGPD)

If a subject is both an EU resident AND a Brazilian resident (rare but real for dual-nationals or expats):

1. Apply the **stricter** of the two regimes per right.
2. For Art. 33 (GDPR) / Art. 48 (LGPD) breach notification: 48h internal SLA satisfies both.
3. For Art. 17 (GDPR) / Art. 18 §IV (LGPD) erasure: identical 30-day cap; same 12-backend pipeline.
4. Audit per-regime applicability flags in `dsr_tickets.regime_mask`.

---

## 6. Audit + evidence + metrics

### 6.1 Audit events emitted per DSR

Every DSR run emits at minimum:
- `dsr.request_received.v1` (intake)
- `dsr.verified.v1` (identity verified — clock_start)
- One or more decision arm events (`dsr.access_completed.v1`, `dsr.erasure_completed.v1`, `dsr.restriction_completed.v1`, etc.)
- `dsr.notification_propagated.v1` per recipient (Art. 19)
- `dsr.receipt_issued.v1` (JWT receipt)

All events go to R2 `audit-<region>` (Object Lock 7y) + hash-chained daily per PAT-AUDIT-VERIFY-001.

### 6.2 Evidence retention

| Evidence | Where | Retention |
|---|---|---|
| DSR ticket row | Neon `dsr_tickets` | 5y post-resolution |
| Audit chain | R2 audit log | 7y Object Lock |
| Export bundle (Art. 15, Art. 20) | R2 `dsr-export-<region>` | 90d (re-issuable) |
| Erasure Merkle root | Audit chain | 7y |
| Objection LIA decision | `legal/lia/<purpose>.md` + audit | Annual review |
| Notification recipient manifest (Art. 19) | Audit chain | 7y |
| Manifestly-unfounded rejection countersigns | Audit chain | 7y |

### 6.3 Monthly SLO report

DPO monthly checklist items 8 + 17 (GDPR delta) + 18 (TIA refresh) track:

- Count of DSRs per right type per regime (GDPR vs LGPD vs both).
- SLA hit rate per right type per regime.
- Median + p95 turnaround.
- Pause-event rate (legal hold, technical impossibility, ambiguity).
- Operator-pass-through rate (tenant-routed DSRs).
- Rejection rate + reasons (with separate count for Art. 12(5) manifestly-unfounded — high bar to flag drift).
- Art. 19 notification success rate per recipient.

Source data: `EVT-013` (DSR SLO report). Target: ≥ 95% SLA hit rate per right type per regime.

---

## 7. Failure modes + escalations

| Failure | Severity | Owner | Mitigation |
|---|---|---|---|
| Audit emit fails | SEV-1 | Oncall + DPO | Fail-CLOSED abort; retry; if persistent, page on-call SRE |
| MFA step-up fails for legitimate subject | SEV-3 | Support | Manual re-verification via privacy@hugr.com; document in audit |
| Erasure pipeline pauses on a backend | SEV-2 | DPO + SRE Lead | Investigate backend; resume within 5 BD; if exceeds, customer notification + SA notification if SLA blown |
| 12-backend Merkle root mismatch | SEV-1 | DPO + Security Lead | Halt all DSR processing; integrity investigation; audit-chain verify |
| Operator-pass-through tenant unresponsive | SEV-3 | DPO | Customer notified + SA escalation reference number issued (subject can escalate themselves) |
| Sub-processor outage during export window | SEV-3 | DPO + SRE | Partial export with manifest; re-issue after recovery |
| Legal hold conflict | SEV-2 | DPO + Legal | Legal counsel review; document Art. 17(3) basis; customer notified |
| Art. 19 notification fails for recipient | SEV-3 | DPO | Document Art. 19 second sentence (impossible / disproportionate); subject notified with manifest |
| EU SA inquiry on DSR | SEV-2 | DPO + Legal | RB-REGULATOR-INQUIRY procedure; response SLA per SA |
| Cross-regime overlap (subject claims GDPR + LGPD) | SEV-3 | DPO | Apply stricter regime per right; mark `regime_mask` |

Escalation chain:
1. **L0**: automated audit-fail-CLOSED.
2. **L1**: oncall SRE (PagerDuty).
3. **L2**: DPO (interim Privacy Officer = Gustavo Schneiter).
4. **L3**: Legal counsel + Security Lead.
5. **L4**: SA notification (member-state SA per subject residence; lead SA via one-stop-shop once EU representative appointed).

---

## 8. Cross-references

- `specs/_compliance/GDPR-FULL-AUDIT-2026-05-15.md` §1.7 (Art. 15–22 mapping)
- `specs/_compliance/GDPR-SCC-EXECUTION-2026-05-15.md` (cross-border context for portability + notification)
- `specs/_compliance/GDPR-DPIA-LIBRARY.md` (Art. 35 + Lighthouse triggers; DPIA gates DSR-impacting changes)
- `specs/_compliance/LGPD-FULL-AUDIT-2026-05-15.md` §1.8 (LGPD Art. 18 parallel)
- `specs/_compliance/LGPD-ROPA-2026-05-15.md` (RoPA-7 row)
- `specs/_compliance/LGPD-RESIDENCY-ATTESTATION-2026-05-15.md` (Art. 44 GDPR mirror posture)
- `specs/_compliance/LGPD-DPO-MONTHLY-CHECKLIST.md` (operational cadence; items 17 + 18 added by GDPR audit)
- `specs/03_architecture/privacy_model.md` §6 (DSR pipeline) + §5.6 (consent)
- `specs/_runbooks/RB-DSR-LGPD-FULL.md` (LGPD parallel runbook — shared internal procedures)
- `apps/docs/docs/explanation/privacy/gdpr.mdx` (customer-facing summary)
- `crates/corelink-dsr/` (runtime; trait surface + property tests)
- `crates/corelink-privacy-consent-ledger/`
- `crates/corelink-privacy-erasure-worker/`
- `crates/corelink-privacy-pseudonymize/`
- `crates/corelink-privacy-residency-enforcement/` (legal-hold check)
- `legal/dpa/` (controller / processor / sub-processor clauses)
- `legal/lia/` (legitimate-interest assessments for Art. 21(1) objection workflow)
- `legal/tia/` (Schrems II Transfer Impact Assessments; informs Art. 20 portability scope decisions)
- `legal/dpia/` (Art. 35 DPIAs)
- `legal/breach-notification/eu-sa-contacts.md` (EU SA contact registry — to populate T+15)
