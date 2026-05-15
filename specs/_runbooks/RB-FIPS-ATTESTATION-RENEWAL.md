---
id: "RB-FIPS-ATTESTATION-RENEWAL"
type: "runbook"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
gap: "GAP-02"
soc2_controls: ["CC6.1", "C1.1"]
review_cadence: "quarterly"
next_review: "2026-08-15"
references:
  - "specs/_compliance/BYOK-FIPS-ATTESTATION-MATRIX.md"
  - "specs/_compliance/FIPS-RFI-QUESTIONNAIRE.md"
  - "specs/_compliance/fips-attestation-letters/"
  - "scripts/verify-fips-endpoints.py"
  - "compliance/byok-fips-matrix.md"
  - "specs/_runbooks/RB-VENDOR-RISK-QUARTERLY-REVIEW.md"
tags: ["runbook", "byok", "fips", "gap-02", "soc2", "quarterly"]
---

# RB-FIPS-ATTESTATION-RENEWAL — quarterly attestation renewal protocol

> **Status:** ACTIVE. Owner: Crypto SME. Cadence: quarterly
> (calendar-driven) + ad-hoc on trigger events (§3). Audit evidence
> for SOC 2 CC6.1 + C1.1 (GAP-02 closure).

---

## 1. Purpose

Keep the **BYOK FIPS attestation matrix**
(`specs/_compliance/BYOK-FIPS-ATTESTATION-MATRIX.md`) accurate at all
times by (a) renewing vendor-signed attestation letters on a
semi-annual cadence, (b) reacting within 5 business days to any CMVP
status change, and (c) preserving an unbroken chain of evidence for
SOC 2 fieldwork.

## 2. Schedule

| Quarter | Window | Activities | Owner |
|---|---|---|---|
| 2026 Q3 | 2026-07-15 → 2026-08-15 | Q3 review; AWS attestation renewal (semi-annual); CMVP status sweep | Crypto SME |
| 2026 Q4 | 2026-10-15 → 2026-11-15 | Q4 review; GCP / Azure / Vault attestation renewal | Crypto SME |
| 2027 Q1 | 2027-01-15 → 2027-02-15 | Q1 review; AWS renewal | Crypto SME |
| 2027 Q2 | 2027-04-15 → 2027-05-15 | Q2 review; GCP / Azure / Vault renewal; **FIPS 140-2 final sunset audit** (post 2026-09-22 — confirm no providers regressed) | Crypto SME + VP-Sec |

> Calendar reminders MUST be created in the Crypto SME calendar at
> the start of every quarter with **14 calendar days lead-time** to
> the renewal-window opening.

## 3. Trigger conditions (ad-hoc renewal)

Any of the following MUST trigger an immediate ad-hoc review within
**5 business days**:

1. NIST CMVP publishes a status change for any tracked module
   (Active → Historical, Active → Revoked, certificate suspended).
2. A provider issues a service announcement deprecating a tier
   currently in use (e.g. Azure Premium HSM EOL).
3. A provider experiences a publicly disclosed security incident
   touching key-management surfaces (e.g. KMS data plane outage,
   sub-processor breach).
4. `scripts/verify-fips-endpoints.py` exits non-zero on `main`
   (indicates a code change introduced a non-FIPS endpoint
   regression).
5. CoreLink onboards a new BYOK provider — full RFI
   (`FIPS-RFI-QUESTIONNAIRE.md`) + initial attestation letter.
6. The 2026-09-22 FIPS 140-2 sunset date arrives — confirm every
   tracked module is either revalidated under 140-3 or covered by an
   explicit waiver in `compliance/byok-fips-matrix.md`.

## 4. Quarterly checklist (Crypto SME)

For each provider row in `BYOK-FIPS-ATTESTATION-MATRIX.md` §2:

- [ ] Open NIST CMVP entry for the module ID; confirm status
      still "Active". Snapshot the page (`wget -p` / PDF export) and
      file under `compliance/attestation-evidence/cmvp-snapshots/`.
- [ ] If "Next renewal" date is within the next quarter, send the
      vendor letter template
      (`specs/_compliance/fips-attestation-letters/LETTER-*.md`)
      with placeholders filled. Open internal ticket.
- [ ] Verify SHA-256 of stored attestation letter matches the value
      recorded in matrix §2 "Attestation doc" column.
- [ ] Run `python3 scripts/verify-fips-endpoints.py` locally; confirm
      exit 0; archive output to evidence log.
- [ ] Run `python3 scripts/validate_specs.py` to ensure spec corpus
      remains valid.
- [ ] If any item above fails, escalate per §5.
- [ ] Once all 4 providers complete, update matrix §6 Changelog with
      the new version + date + summary.

## 5. Vendor escalation comms

When a vendor has not responded within the **D+30 SLA** stated in
the letter template:

### 5.1 Reminder (D+30; same-day)

```
Subject: Reminder — pending FIPS attestation letter for CoreLink BYOK (D+30)

Dear {{vendor_contact}},

This is a friendly reminder regarding our FIPS attestation letter
request sent on {{date_sent}} (your case / ticket {{case_id}}). The
30-day SLA expired today and we have not received a response.

CoreLink's SOC 2 Type I audit fieldwork is scheduled for 2026-Q3 and
this letter is a blocking-GA evidence item. Could you please confirm
either (a) the letter is in flight and an expected delivery date, or
(b) the case has been routed to a different team and the new owner.

Happy to jump on a 15-minute call if helpful.

Thank you,
Gustavo
```

### 5.2 Escalation (D+45; CC account manager + AE)

```
Subject: ESCALATION — pending FIPS attestation letter for CoreLink BYOK (D+45)

Dear {{vendor_account_manager}} (+ {{vendor_account_executive}}),

We have a pending compliance request open for 45 days with no
substantive response (case {{case_id}}). The artefact is mandatory
for our SOC 2 fieldwork and the absence of it would force a
qualified opinion from our auditor on CC6.1 / C1.1.

Could one of you escalate to the appropriate Compliance Director?
If we do not receive resolution by {{date_sent + 60d}}, we will be
required to (a) invoke the fallback ADR (CoreLink WI-S20-003 §5.2)
classifying GAP-02 as non-blocking-with-waiver — which materially
weakens our auditor's evidence chain on your service, and (b)
document the SLA breach in our public sub-processor attestation
log.

We greatly value the partnership and would prefer to resolve this
inside the SLA. Please advise.

Best,
Gustavo Schneiter
Founder / Architect, HuGR Labs
```

### 5.3 Final escalation (D+60; legal + auditor in CC)

If D+60 passes without resolution, the Crypto SME convenes a
go/no-go meeting with VP-Sec + Founder + SOC 2 audit firm contact
to decide:

- Invoke fallback ADR (WI-S20-003 §5.2) — reclassify GAP-02 as
  **non-blocking with explicit waiver** (T+90d expiry) in
  `compliance/byok-fips-matrix.md`.
- Notify the SOC 2 auditor in writing about the vendor SLA breach
  and offered evidence substitute (public CMVP record + vendor
  legal counsel attestation).
- If the affected provider is **Vault** (customer-hosted), the
  fallback also requires the affected customer to provide
  self-attestation per their environment.

## 6. Calendar template (Crypto SME)

```ics
SUMMARY: CoreLink FIPS attestation quarterly review (BYOK-FIPS-ATTESTATION-MATRIX)
DTSTART;TZID=America/Sao_Paulo:20260715T090000
DTEND;TZID=America/Sao_Paulo:20260715T103000
RRULE:FREQ=QUARTERLY;BYMONTH=1,4,7,10;BYMONTHDAY=15
DESCRIPTION:Run §4 checklist; renew vendor letters; update matrix §6.
```

## 7. Audit evidence layout

```
compliance/
├── byok-fips-matrix.md                 # technical CMVP reference (existing)
└── attestation-evidence/
    ├── AWS-KMS-FIPS-2026-03.pdf        # vendor letter (signed)
    ├── AWS-KMS-FIPS-2026-09.pdf        # next renewal
    ├── GCP-KMS-FIPS-2026-06.pdf
    ├── Azure-KV-FIPS-2026-06.pdf
    ├── Vault-FIPS-2026-06.pdf
    ├── cmvp-snapshots/
    │   ├── cmvp-4523-2026-Q3.pdf       # AWS module status snapshot
    │   ├── cmvp-3318-2026-Q3.pdf       # GCP HSM module status
    │   └── cmvp-3516-2026-Q3.pdf       # Azure Premium HSM status
    └── verifier-runs/
        └── verify-fips-endpoints-2026-Q3.txt
```

Each PDF is hashed (`sha256sum`) and the hash recorded in matrix §2
"Attestation doc" column; the matrix doc is committed; the PDF
itself is stored in the secure compliance bucket (
`s3://corelink-compliance-evidence/attestation/`) with object-lock +
versioning. **Never** commit the raw PDFs to git.

## 8. Cross-reference

- Matrix: `specs/_compliance/BYOK-FIPS-ATTESTATION-MATRIX.md`
- Letters: `specs/_compliance/fips-attestation-letters/`
- RFI: `specs/_compliance/FIPS-RFI-QUESTIONNAIRE.md`
- Verifier: `scripts/verify-fips-endpoints.py`
- SOC 2 rollup: `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md`
- Vendor risk register: `specs/_compliance/VENDOR-RISK-REGISTER.md`
- Existing CMVP reference: `compliance/byok-fips-matrix.md`

## 9. Changelog

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-05-15 | Gustavo (via Claude Opus 4.7) | Initial GAP-02 renewal runbook. |
