---
doc_id: "FIPS-RFI-QUESTIONNAIRE"
id: "FIPS-RFI-QUESTIONNAIRE"
type: "compliance_doc"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: ["Crypto SME (TBD)", "VP-Sec"]
gap: "GAP-02"
soc2_controls: ["CC6.1", "C1.1", "CC9.2"]
review_cadence: "annual"
next_review: "2027-05-15"
references:
  - "specs/_compliance/BYOK-FIPS-ATTESTATION-MATRIX.md"
  - "specs/_compliance/VENDOR-RISK-METHODOLOGY.md"
  - "specs/_runbooks/RB-FIPS-ATTESTATION-RENEWAL.md"
  - "NIST FIPS 140-3"
  - "NIST SP 800-57 Pt 1 Rev 5"
tags: ["byok", "fips", "rfi", "vendor-onboarding", "gap-02", "soc2-cc9-2"]
---

# FIPS RFI questionnaire — new BYOK provider onboarding

> **Purpose.** Use this 30-question RFI for **every new KMS / HSM
> provider** being evaluated for inclusion in the CoreLink BYOK
> enterprise tier. The vendor must answer all 30 questions in
> writing and counter-sign the response before legal / commercial
> negotiations commence. Answers feed into the
> `BYOK-FIPS-ATTESTATION-MATRIX.md` and the
> `VENDOR-RISK-REGISTER.md`.
>
> **Hard pass criteria** (any "no" → reject):
> - Q3 (FIPS 140-3 or 140-2 Level 2+ minimum, with active CMVP cert).
> - Q12 (envelope encryption / wrap-unwrap APIs available).
> - Q18 (audit log access for `Decrypt` events).
> - Q22 (≤ 72h breach-notification SLA contractually committed).
> - Q26 (sub-processor list disclosable under NDA).

---

## Section A — Vendor identity & corporate

1. **Legal entity name, jurisdiction of incorporation, and registered
   address.**
2. **Primary regulator(s) and any active enforcement actions, consent
   decrees, or material litigation in the last 36 months
   (cryptography / data-protection scope).**
3. **CMVP certificate inventory.** Provide every active NIST CMVP
   certificate ID, module name, validation level (FIPS 140-2 L1/L2/L3
   or FIPS 140-3 L1/L2/L3), validation date, sunset date, and the
   revalidation roadmap for sunsetting modules.

## Section B — FIPS mode mechanics

4. **How is FIPS mode enabled** in your service (per-account toggle,
   per-region endpoint, per-key flag, build-time flag)?
5. **What FIPS-specific endpoints / hostnames** must customers use to
   guarantee a request lands on a FIPS-validated module? Provide the
   full hostname pattern (including any regional variants).
6. **How does a customer verify at runtime** that a given key
   operation was processed on a FIPS-validated module (response
   header, audit-log field, attestation API)?
7. **Are there any FIPS-incompatible features** that, if enabled by
   the customer, would silently bypass FIPS mode? List exhaustively.
8. **What is the default protection level / tier** for new keys, and
   how is "HSM-backed" vs "software-backed" distinguished in the
   API?

## Section C — Key hierarchy & lifecycle

9. **Describe the key hierarchy**: root of trust → key encryption
   keys → customer master keys → data encryption keys. Identify
   which keys are vendor-controlled vs customer-controlled.
10. **Key generation entropy.** Is the DRBG NIST SP 800-90A approved?
    Provide CMVP module ID of the DRBG.
11. **Key rotation.** Maximum supported rotation cadence; rotation
    atomicity; key-overlap window during rotation; whether old key
    versions remain usable for unwrap after rotation.
12. **Envelope encryption support.** Confirm explicit `wrapKey` /
    `unwrapKey` (or `Encrypt` / `Decrypt`) APIs. Confirm AAD /
    `encryption_context` parameter is enforced server-side.
13. **Key destruction.** Hard-delete SLA after revoke (seconds /
    minutes / days). Crypto-shred guarantees on backups.
14. **Key export.** Is the customer ever able to export raw key
    material? (For CoreLink BYOK, the answer **MUST be "no"**.)

## Section D — Access control & audit log

15. **Authentication methods** supported (mTLS, OIDC, IAM, SAML).
16. **Authorization model.** RBAC granularity for `Encrypt` /
    `Decrypt` / `DescribeKey` operations.
17. **Audit log scope.** What events are logged? Specifically: are
    every `Encrypt` and `Decrypt` request logged with caller
    identity, key ARN, AAD hash, success/failure, latency?
18. **Audit log access.** How does the customer retrieve audit
    events (API, log-pipe to cloud storage, syslog)? Retention
    minimum; immutability guarantees; tamper-evidence mechanism.
19. **Anomaly detection / KMS-side alerting** available to customer
    (e.g. unusual `Decrypt` volume).

## Section E — Operational SLA & breach

20. **Service availability SLA** (percent uptime + credit schedule).
21. **Support SLA tiers.** Response time for SEV-1 / SEV-2 /
    SEV-3 / SEV-4 tickets. 24x7 availability? Region of support
    staff.
22. **Breach notification SLA.** Contractual maximum time from
    vendor's discovery to customer notification. CoreLink requires
    ≤ **72 hours** (LGPD Art. 48 compatible) — confirm in writing.
23. **Security incident history** (last 36 months) and root-cause
    summaries (NDA acceptable).
24. **Penetration test cadence** and willingness to share executive
    summaries under NDA.

## Section F — Sub-processors & data residency

25. **Data residency.** Where (country, region) are key operations
    physically processed? Where are audit logs stored?
26. **Sub-processor list.** Names, roles, countries of every
    sub-processor with logical or physical access to keys or
    audit-log streams. Provide under NDA if required.
27. **Cross-border transfer mechanism.** SCCs (EU/UK), Standard
    Contractual Clauses (LGPD ANPD), or equivalent.

## Section G — Commercial & contractual

28. **Pricing model.** Per-key, per-operation, per-month; volume
    tiers; egress charges for audit-log retrieval.
29. **Right-to-audit clause.** Confirm the vendor will sign a DPA
    granting customer (or customer's auditor) the right to request
    SOC 2 Type II report, ISO 27001 certificate, and an annual
    pentest executive summary.
30. **Termination / data return.** Maximum customer-data retention
    after contract termination; secure-destruction certificate
    provided on request; crypto-shred timeline.

---

## Response format

Vendor MUST respond using the following structure:

```yaml
vendor: "{{vendor_legal_name}}"
respondent: "{{respondent_name + role}}"
date_signed: "{{yyyy-mm-dd}}"
signature_method: "{{wet-ink / docusign / counter-signed-pdf}}"
answers:
  q1: "..."
  q2: "..."
  # ... q30
attachments:
  - "FIPS-certificate-cmvp-####.pdf"
  - "SOC-2-Type-II-{{vendor}}-{{year}}.pdf"
  - "ISO-27001-{{vendor}}-{{year}}.pdf"
  - "pentest-exec-summary-{{vendor}}-{{year}}.pdf"
```

A response that is incomplete, equivocates on Q3/Q12/Q18/Q22/Q26, or
omits the listed attachments is **automatically rejected**. The
CoreLink VP-Sec must counter-sign the RFI response before commercial
terms are negotiated.

---

## Storage & cross-reference

Filed responses live under
`compliance/rfi-responses/{{vendor_slug}}-{{yyyy-mm}}.pdf` (SHA-256
recorded in `BYOK-FIPS-ATTESTATION-MATRIX.md` §2 row for the new
provider). The vendor is added to `VENDOR-RISK-REGISTER.md` with
tier classification driven by Section E + F answers
(per `VENDOR-RISK-METHODOLOGY.md`).
