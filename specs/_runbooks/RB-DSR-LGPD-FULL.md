---
id: "RB-DSR-LGPD-FULL"
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
  - "LGPD-FULL-AUDIT-2026-05-15"
  - "LGPD-ROPA-2026-05-15"
  - "PRIVACY-MODEL"
tags: ["runbook", "lgpd", "lgpd-art-18", "dsr", "data-subject-rights", "gap-22-followup", "anpd"]
---

# RB-DSR-LGPD-FULL — Internal runbook for LGPD Art. 18 DSR processing

> **Status:** DRAFT. Owner: interim Privacy Officer (Gustavo Schneiter); transferred to formal DPO post-appointment.
>
> **Scope:** end-to-end processing of every LGPD Art. 18 data-subject right (9 rights + 1 amendment) through CoreLink's DSR pipeline (`crates/corelink-dsr/` + 12-backend canonical erasure propagation per `privacy_model.md §6.2`).
>
> **Companion docs:**
> - `specs/_compliance/LGPD-FULL-AUDIT-2026-05-15.md` §1.8 (article-by-article rights mapping)
> - `specs/_compliance/LGPD-ROPA-2026-05-15.md` (RoPA-7 DSR ticket processing row)
> - `specs/03_architecture/privacy_model.md` §6 (canonical DSR pipeline)
> - `apps/docs/docs/explanation/privacy/lgpd-full.mdx` (customer-facing summary)
> - `crates/corelink-dsr/` (runtime)
> - `crates/corelink-privacy-erasure-worker/` (12-backend propagation)
>
> **Trigger:** any inbound DSR request, whether via API (`POST /v1/privacy/dsr/{kind}`), admin UI (deferred to WI-S13-* track), or email to `privacy@hugr.com`.

---

## 1. Right type taxonomy + decision tree

Step 1 of any DSR is to classify the request. LGPD Art. 18 enumerates 9
rights; CoreLink's canonical `DsrRequestKind` enum (`crates/corelink-dsr/src/event.rs`)
maps these to 6 destructive + 3 informational arms. The mapping table:

| Request from customer | LGPD Art. 18 ref | CoreLink `DsrRequestKind` | Endpoint | SLA | MFA required? | Audit-fail-CLOSED? |
|---|---|---|---|---|---|---|
| "Do you have data on me?" | §I — Confirmação | `Access` (subset query) | `POST /v1/privacy/dsr/access` | 5 BD | No | YES |
| "Give me a copy of my data" | §II — Acesso | `Access` | `POST /v1/privacy/dsr/access` | 15 BD | No | YES |
| "Fix this incorrect field" | §III — Correção | `Rectification` | `POST /v1/privacy/dsr/rectification` | 5 BD | **YES** | YES |
| "Delete my data" | §IV / §VI — Eliminação | `Erasure` | `POST /v1/privacy/dsr/erasure` | 30 cal d | **YES** | YES |
| "Give me my data so I can move it" | §V — Portabilidade | `Portability` | `POST /v1/privacy/dsr/portability` | 15 BD | No | YES |
| "Who did you share my data with?" | §VII — Compartilhamento | `Access` (recipients query) or `GET /v1/privacy/dsr/shared-with` | `GET` | 15 BD | No | YES |
| "What happens if I don't consent?" | §VIII — Consequências | (UX — shown at consent capture) | UI consent form | Continuous | No | N/A |
| "Unsubscribe from marketing" | §IX — Revogação | `Restriction` for purpose-pause; OR `DELETE /v1/consent/<purpose>` | DELETE | **≤ 5 min** | No | YES |
| "I oppose your processing" | §II amendment — Oposição | `Objection` | `POST /v1/privacy/dsr/objection` | 15 BD | No | YES |

### 1.1 Decision tree on intake

```
[ Inbound request ]
        │
        ▼
[ Q1: Is requester identifiable as a data subject of CoreLink? ]
   ├── NO → Reject with "no relationship found"; log
   │        attempt for ANPD complaint audit (CTRL-PRIV-014).
   │        Do NOT confirm or deny that someone else is a subject.
   ▼
   YES
        │
        ▼
[ Q2: Is the requester a CoreLink user OR a subject of a tenant's blob upload? ]
   ├── Tenant's blob → Route to the controlling tenant per DPA;
   │                    CoreLink is operator, not controller.
   │                    Notify subject of routing within 5 BD.
   │                    Track via "operator-pass-through" audit event.
   ▼
   CoreLink user
        │
        ▼
[ Q3: Has identity been verified (MFA re-auth)? ]
   ├── NO  → Issue verification challenge (WebAuthn step-up).
   │        Clock NOT started yet (per INV-DSR-VERIFIED-CLOCK).
   ▼
   YES (status = `verified`)
        │
        ▼
[ Q4: Which Art. 18 right? ] → use table above to pick endpoint, SLA, MFA gate
```

---

## 2. Per-right end-to-end runbook

### 2.1 §I — Confirmação (5 BD SLA)

Lightest-weight informational right. Customer wants a yes/no.

1. Receive request (`POST /v1/privacy/dsr/access` with `{"scope": "confirmation"}` or human email).
2. Verify identity (WebAuthn step-up; clock starts on `verified`).
3. Audit emit `dsr.request_received.v1` BEFORE state mutation (INV-DSR-AUDIT-FAIL-CLOSED).
4. Query: presence check across the 14 RoPA dataflows (`crates/corelink-dsr/src/store.rs::has_data`).
5. Audit emit `dsr.access_completed.v1` with `scope = confirmation`.
6. Issue receipt JWT (RS256, KMS-rooted, 90d anti-replay).
7. Return `{"has_data": <bool>, "request_id": "<uuid7>", "receipt": "<jwt>"}`.
8. Email customer with confirmation + receipt.

**Failure paths:** if any audit emit fails, abort fail-CLOSED with no state
change. Customer receives a 500 with retry guidance and `Retry-After`.

### 2.2 §II — Acesso (15 BD SLA)

Full data export.

1. Receive request.
2. Verify identity. Clock starts.
3. Audit `dsr.request_received.v1`.
4. Spawn export job (`crates/corelink-dsr/src/store.rs::export_subject`):
   a. Iterate the 14 RoPA dataflows; collect subject-scoped rows.
   b. Format: JSON Lines + Parquet bundle (structured for portability).
   c. Sign bundle with KMS key; store in R2 `dsr-export-<region>` with
      24-hour signed-URL availability.
5. Audit `dsr.export_built.v1` with bundle hash.
6. Email customer with signed URL + receipt.
7. After 24h, evict the URL; bundle preserved 90d in DSR evidence store
   for re-issue on request.

**Edge case:** if a referenced sub-processor cannot return data within SLA
(e.g., Grafana logs older than retention), include a manifest of "data
not available" entries with reason. Audit emit `dsr.partial_export.v1`.

### 2.3 §III — Correção (5 BD SLA, MFA REQUIRED)

Customer wants to fix one or more fields.

1. Receive request with `{"field": "billing_address", "new_value": "..."}`.
2. Verify identity. MFA step-up enforced (INV-DSR-MFA-DESTRUCTIVE — destructive arm).
3. Audit `dsr.mfa_verified.v1`.
4. Validate the proposed change against schema (`crates/corelink-dsr/src/event.rs::ValidatedField`).
5. Audit `dsr.rectification_pending.v1` with old hash + new hash.
6. Mutate: Neon `account` row; propagate to KV cache invalidation; downstream
   notify (Stripe Customer.update if billing field; Clerk if email).
7. Audit `dsr.rectification_completed.v1` with diff manifest.
8. Email customer with diff confirmation + receipt.

**Edge case:** if the new value violates a uniqueness constraint (e.g.,
email already in use), reject with `dsr.rectification_rejected.v1` and
return the constraint violation in plain language.

### 2.4 §IV / §VI — Eliminação / Anonimização / Bloqueio (30 cal d SLA, MFA REQUIRED)

The big one. Full erasure pipeline. Canonical 12-backend propagation per
`privacy_model.md §6.2`.

**Pre-flight (Day 0):**

1. Receive request.
2. Verify identity. MFA step-up enforced.
3. Audit `dsr.erasure_requested.v1`.
4. Check legal-hold flag (`crates/corelink-privacy-residency-enforcement/src/migration.rs`):
   if subject is under legal hold → fail-CLOSED with explanation;
   audit `dsr.erasure_blocked_legal_hold.v1`.

**Propagation (Days 0–30):**

Iterate the 12 canonical backends in order. Per
`privacy_model.md §6.2`:

| Order | Backend | Action | Audit event | Effective or pseudonymized? |
|---|---|---|---|---|
| 1 | Neon `dsr_tickets`, `account`, `tenant`, `user_account`, `consent_ledger`, `subscription` | Hard delete subject rows; preserve DSR ticket + audit refs | `dsr.neon_subject_erased.v1` | EFFECTIVE |
| 2 | Neon billing detail (`invoice`, `usage_event`) | Retain under legal_hold per Art. 16 §3 fiscal (5y) | `dsr.neon_billing_legal_hold_retained.v1` | PSEUDONYMIZED (subject_id replaced after 5y) |
| 3 | R2 CAS | Refcount-based eraser: subject_unaffiliated = remove reference + 72h grace; subject_dedicated = tombstone immediate | `dsr.cas_eraser_completed.v1` (with refcount manifest) | EFFECTIVE |
| 4 | R2 AC | Invalidate all `subject_user_id` entries; evict cache; notify downstream | `dsr.ac_invalidated.v1` | EFFECTIVE |
| 5 | D1 `blob_meta` + `ac_meta` | Delete subject-scoped rows; refcount sync with R2 | `dsr.d1_subject_erased.v1` | EFFECTIVE |
| 6 | KV | Invalidate session tokens + cached subject metadata | `dsr.kv_invalidated.v1` | EFFECTIVE |
| 7 | Stripe | `Customer.update` with PII nullified (PCI scope preservation) | `dsr.stripe_pii_nullified.v1` | EFFECTIVE (Stripe-side) |
| 8 | Loki / Grafana | Log deletion API + retention compaction | `dsr.grafana_logs_deleted.v1` | EFFECTIVE |
| 9 | R2 audit log (7y Object Lock) | Pseudonymize subject_id with HKDF (`info = "corelink/v1/audit-pseudonym"`); CANNOT delete | `dsr.audit_pseudonymized.v1` | PSEUDONYMIZED |
| 10 | Backup PITR (Neon, 30d) | Tombstone replay on any restore within window; natural rotation 30d | `dsr.backup_tombstone_armed.v1` | PSEUDONYMIZED (via replay) |
| 11 | R2 CAS legal_hold partition (governance mode) | Pseudonymize index refs; retain content under active hold | `dsr.cas_legal_hold_pseudonymized.v1` | PSEUDONYMIZED |
| 12 | Compliance evidence store (LIA, consent proof, DSR evidence, incident records) | Pseudonymize subject_id; retain per framework SLA (7y) | `dsr.compliance_evidence_pseudonymized.v1` | PSEUDONYMIZED |

**Completion (Day ≤ 30):**

1. Verify all 12 audit events emitted successfully.
2. Hash-chain the 12 event hashes into a Merkle root.
3. Audit `dsr.erasure_completed.v1` with Merkle root.
4. Email customer with Merkle root + receipt + final DSR ticket status.
5. Update DSR ticket status to `completed`.

**Failure paths:**

- Any step fails: pause pipeline; SEV-2 page to DPO; audit `dsr.erasure_paused.v1`
  with failure scope. Clock pauses (documented technical impossibility — cap
  5 BD before customer notification of delay).
- After 5 BD of paused state: customer notified by email with explanation +
  expected resolution date. ANPD informed if delay exceeds 30 cal d.

### 2.5 §V — Portabilidade (15 BD SLA)

Similar to §II Access, but bundle is structured-data format suitable for
import elsewhere (per LGPD Art. 18 §V).

1. Receive request.
2. Verify identity. Clock starts.
3. Audit `dsr.portability_requested.v1`.
4. Export job:
   a. Iterate RoPA dataflows producing **CoreLink-controlled** subject data
      (rows 1, 2, 7, 8, 11, 13 of `LGPD-ROPA-2026-05-15.md` §2).
   b. Format: structured JSON + Parquet with schema documentation
      (`legal/portability-schema/v1.json`).
   c. Sign bundle.
5. Audit `dsr.portability_built.v1`.
6. Email customer with signed URL + import-guide PDF.

**Edge case:** if tenant has used CoreLink as operator for subject's data
in blob uploads, that data is the **tenant's responsibility** — portability
of blob content is tenant's obligation, not HuGR's. Audit
`dsr.portability_operator_passthrough.v1` if applicable.

### 2.6 §VII — Compartilhamento (15 BD SLA)

Customer wants to know everyone we shared their data with.

1. Receive request.
2. Verify identity. Clock starts.
3. Audit `dsr.shared_with_requested.v1`.
4. Query: cross-reference subject_id against the 14 RoPA dataflows;
   collect every sub-processor row where the subject's data flowed.
5. Format: per-recipient record with role (controller / operator / joint),
   purpose, legal basis, region, and date range.
6. Audit `dsr.shared_with_completed.v1`.
7. Email customer with structured report + plain-language summary.

### 2.7 §VIII — Consequências de não consentir (continuous)

Not a runbook step per se — this is a UX requirement enforced at consent
capture. The consent form (`apps/admin-ui/src/privacy/ConsentForm.tsx` —
deferred wiring) shows the "se você não consentir, [consequência]" text
inline, and the `wording_id` of that text is captured in every consent
record (`EVT-049` payload).

If a customer specifically asks "what happens if I don't consent to X?",
respond from the current `legal/privacy-notice/v*.{locale}.md` §5
canonical text. Email response SLA: 5 BD.

### 2.8 §IX — Revogação (≤ 5 min — IMMEDIATE)

The fastest right.

1. Receive `DELETE /v1/consent/<purpose>` (or email).
2. Verify identity (lightweight: existing session is sufficient; no MFA
   re-auth required because this is a non-destructive purpose-pause).
3. Audit `consent.revoked.v1` with full proof bundle (notice_text_hash of
   the "unsubscribe" text shown, locale, wording_id, timestamps).
4. Propagate in ≤ 5 min:
   a. Update `consent_ledger` (Neon) — purpose enters `revoked` state.
   b. Invalidate KV cache.
   c. Notify downstream processors (marketing tool, analytics tool, etc.).
5. Verify propagation: `crates/corelink-privacy-consent-ledger/` runs the
   propagation check before returning success.
6. Audit `consent.revocation_propagated.v1`.
7. Return success to customer + email confirmation.

**No fail-open:** if the consent expires or is revoked, the purpose enters
`consent_lapsed` state. Basis does NOT degrade to `legitimate_interest`.
Processing PAUSES until re-consent or hard erasure (§IV).

### 2.9 §II amendment — Oposição (15 BD SLA)

Customer wants to oppose processing under legitimate interest (Art. 7 §IX).
Applies primarily to `security_monitoring` and `analytics_aggregated`.

1. Receive `POST /v1/privacy/dsr/objection` with `{"purpose": "<tag>", "reason": "<text>"}`.
2. Verify identity. Clock starts.
3. Audit `dsr.objection_received.v1` with reason text hash.
4. DPO reviews:
   a. Is the objection grounded in subject's particular situation?
   b. Does CoreLink have a "legitimate interest that overrides the subject's
      interest, rights and freedoms" (the LIA balancing test)?
   c. Outcome: ACCEPT (switch subject to opt-out) or REJECT (with reasoned
      explanation citing the LIA balancing in `legal/lia/<purpose>.md`).
5. Audit `dsr.objection_decided.v1` with decision + reasoning hash.
6. If ACCEPT: subject's processing under that purpose is paused; LIA-record
   notes the opt-out.
7. Email customer with decision + reasoning + receipt.

**Edge case:** objection to `regulatory_compliance` purpose is not
honorable (legal obligation overrides). Reject with explanation that
points to Art. 7 §II + the controlling law (LGPD Art. 37 for audit
retention; Art. 16 §3 for fiscal).

---

## 3. Special cases

### 3.1 Tenant-blob subject (CoreLink is operator)

When a DSR comes from a subject whose data is in a tenant's blob upload:

1. CoreLink is operator (DPA Art. 7); tenant is controller.
2. Route the DSR to the controlling tenant within 5 BD (per DPA).
3. Audit `dsr.operator_pass_through.v1` with tenant ID + routing timestamp.
4. Notify subject by email: "Your data is controlled by tenant X under our
   DPA. We have routed your request to tenant X, who must respond within
   30 cal d per LGPD Art. 18. If they do not, you may escalate to ANPD
   with reference number [request_id]."

### 3.2 Subject under legal hold

1. Check `legal_hold` flag on the subject's records.
2. If flag = true: pause the requested DSR action on the affected scope;
   notify subject with the scope of the hold + the legal basis (subpoena,
   court order, ongoing investigation).
3. Continue all other DSR actions on un-held scope (e.g., marketing opt-out
   can proceed even if billing records are under hold).
4. When hold expires, resume paused actions automatically; audit
   `dsr.legal_hold_expired_resumed.v1`.

### 3.3 Subject's request references another subject's privacy

1. Redact references to other subjects in export bundles (e.g., shared
   audit records, multi-subject blobs).
2. Document redactions in export manifest.
3. Audit `dsr.cross_subject_redaction_applied.v1`.

### 3.4 ANPD inquiry on a DSR

If ANPD inquires about a specific DSR (request_id received via subpoena
or inquiry):

1. Route to legal counsel + DPO immediately (SEV-2).
2. Retrieve full audit chain for the DSR request_id (Object Lock 7y).
3. Build response packet using `specs/_runbooks/RB-REGULATOR-INQUIRY.md`
   (to be added; for now: ad-hoc).
4. Response SLA: ANPD-determined (typically 15 BD).

---

## 4. Audit + evidence + metrics

### 4.1 Audit events emitted per DSR

Every DSR run emits at minimum:
- `dsr.request_received.v1` (intake)
- `dsr.verified.v1` (identity verified — clock_start)
- One or more decision arm events (`dsr.access_completed.v1`,
  `dsr.erasure_completed.v1`, etc.)
- `dsr.receipt_issued.v1` (JWT receipt)

All events go to R2 `audit-<region>` (Object Lock 7y) + hash-chained
daily per PAT-AUDIT-VERIFY-001.

### 4.2 Evidence retention

| Evidence | Where | Retention |
|---|---|---|
| DSR ticket row | Neon `dsr_tickets` | 5y post-resolution |
| Audit chain | R2 audit log | 7y Object Lock |
| Export bundle (§II, §V) | R2 `dsr-export-<region>` | 90d (re-issuable) |
| Erasure Merkle root | Audit chain | 7y |
| Objection LIA decision | `legal/lia/<purpose>.md` + audit | Annual review |

### 4.3 Monthly SLO report

DPO monthly checklist item 8 (extended in this delivery) tracks:

- Count of DSRs per right type.
- SLA hit rate per right type.
- Median + p95 turnaround.
- Pause-event rate (legal hold, technical impossibility, ambiguity).
- Operator-pass-through rate (tenant-routed DSRs).
- Rejection rate + reasons.

Source data: `EVT-013` (DSR SLO report). Target: ≥ 95% SLA hit rate per
right type.

---

## 5. Failure modes + escalations

| Failure | Severity | Owner | Mitigation |
|---|---|---|---|
| Audit emit fails | SEV-1 | Oncall + DPO | Fail-CLOSED abort; retry; if persistent, page on-call SRE |
| MFA step-up fails for legitimate subject | SEV-3 | Support | Manual re-verification via privacy@hugr.com; document in audit |
| Erasure pipeline pauses on a backend | SEV-2 | DPO + SRE Lead | Investigate backend; resume within 5 BD; if exceeds, customer notification + ANPD if SLA blown |
| 12-backend Merkle root mismatch | SEV-1 | DPO + Security Lead | Halt all DSR processing; integrity investigation; audit-chain verify |
| Operator-pass-through tenant unresponsive | SEV-3 | DPO | Customer notified + ANPD escalation reference number issued |
| Sub-processor outage during export window | SEV-3 | DPO + SRE | Partial export with manifest; re-issue after recovery |
| Legal hold conflict | SEV-2 | DPO + Legal | Legal counsel review; document basis; customer notified |

Escalation chain:
1. **L0**: automated audit-fail-CLOSED.
2. **L1**: oncall SRE (PagerDuty).
3. **L2**: DPO (interim Privacy Officer = Gustavo Schneiter).
4. **L3**: Legal counsel + Security Lead.
5. **L4**: ANPD notification (if SLA breach or breach of Art. 18 itself).

---

## 6. Cross-references

- `specs/_compliance/LGPD-FULL-AUDIT-2026-05-15.md` §1.8 (Art. 18 mapping)
- `specs/_compliance/LGPD-ROPA-2026-05-15.md` (RoPA-7 row)
- `specs/_compliance/LGPD-RESIDENCY-ATTESTATION-2026-05-15.md` (Art. 33 context)
- `specs/_compliance/LGPD-DPO-MONTHLY-CHECKLIST.md` (operational cadence)
- `specs/03_architecture/privacy_model.md` §6 (DSR pipeline) + §5.6 (consent)
- `apps/docs/docs/explanation/privacy/lgpd-full.mdx` (customer-facing summary)
- `crates/corelink-dsr/` (runtime; trait surface + property tests)
- `crates/corelink-privacy-consent-ledger/`
- `crates/corelink-privacy-erasure-worker/`
- `crates/corelink-privacy-pseudonymize/`
- `crates/corelink-privacy-residency-enforcement/` (legal-hold check)
- `legal/dpa/` (controller / operator clauses for tenant-pass-through)
- `legal/lia/` (legitimate-interest assessments for objection workflow)
- `legal/breach-notification/anpd-contacts.md`

---

**Fim de RB-DSR-LGPD-FULL.** Owner reviews this runbook after every quarterly
RoPA refresh. Material changes require DPO + Legal countersign.
