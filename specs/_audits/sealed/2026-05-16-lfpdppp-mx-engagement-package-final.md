---
id: "AUDIT-2026-05-16-LFPDPPP-MX-ENGAGEMENT-PACKAGE-FINAL"
type: "audit"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-16"
updated: "2026-05-16"
sprint: "S-11"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: ["Privacy Officer", "MX Attorney (TBD — to engage)"]
supersedes: null
superseded_by: null
parent_wi: "WI-S11-004"
wave: "wave-28"
inv: []
references:
  - "specs/_audits/sealed/2026-05-16-lfpdppp-mx-legal-review-package.md"
  - "specs/_audits/sealed/2026-05-15-debt-register.md"
  - "specs/_audits/sealed/pentest-vendor-shortlist.md"
  - "specs/_audits/sealed/2026-05-16-pentest-engagement-scope-freeze.md"
  - "docs/legal/lfpdppp-mx-engagement-letter-template.md"
  - "docs/legal/lfpdppp-mx-attorney-shortlist.md"
  - "docs/legal/lfpdppp-mx-engagement-email-template.md"
  - "docs/legal/lfpdppp-mx-retainer-template.md"
  - "docs/legal/pentest-rfp-email-template.md"
  - "docs/legal/pentest-engagement-contract-template.md"
  - "legal/privacy-notice/v1.0.0/es-MX.md"
  - "legal/privacy-notice/v1.0.0/metadata.yaml"
  - "legal/privacy-notice/REVIEW_PROCESS.md"
  - "docs/customer-comm/breach-notification/v1.0.0/es/audit-chain-integrity-incident.md"
  - "docs/customer-comm/breach-notification/v1.0.0/es/dsr-pipeline-temporary-degradation.md"
  - "scripts/admin/lfpdppp-mx-tracker.py"
  - "reports/lfpdppp-mx-tracker.json"
tags: ["audit", "s11", "wave-28", "lfpdppp", "mexico", "engagement-package-final", "mx-attorney", "arco", "engagement", "debt-025", "step-6"]
---

# LFPDPPP MX Engagement Package — FINAL (wave-28 step-6)

> **Status:** Engineering-side preparation COMPLETE. The Owner is now down to **3 emails + 1 contract signature** to retain a Mexican attorney + receive the written opinion + 3 EVT-044 PDFs that close DEBT-025 and unblock the pre-GA LFPDPPP local-review gate per `WI-S11-004 §6.1.4`.
>
> **Cross-ref:** `specs/_audits/sealed/2026-05-16-lfpdppp-mx-legal-review-package.md` (wave-23 scoping packet — the binding work statement); `specs/_audits/sealed/2026-05-15-debt-register.md DEBT-025` (open; target wave-26+ close on absorption).

---

## 0. Executive summary

Wave-23 (commit `3000121`) shipped the engineering-side scoping packet + engagement-letter template. This wave-28 step-6 R-prep stream finalizes the **send-ready** package so the Owner can execute the engagement with **3 outbound emails + 1 contract signature** — no further engineering preparation is required.

The 4 new deliverables consolidate the procurement workflow proven at wave-25/26 for the pentest vendor selection (shortlist → tracker → email template → contract template) and apply the same pattern to LFPDPPP MX attorney retention.

| # | Deliverable | Path | LOC | State |
|---|---|---|---|---|
| 1 | Attorney/firm shortlist | `docs/legal/lfpdppp-mx-attorney-shortlist.md` | 122 | NEW |
| 2 | Engagement email template (Mustache) | `docs/legal/lfpdppp-mx-engagement-email-template.md` | 184 | NEW |
| 3 | Tracker script + JSON | `scripts/admin/lfpdppp-mx-tracker.py` + `reports/lfpdppp-mx-tracker.json` | 376 + 87 | NEW |
| 4 | Retainer template (engineering-side draft) | `docs/legal/lfpdppp-mx-retainer-template.md` | 213 | NEW |
| 5 | This consolidation audit | `specs/_audits/sealed/2026-05-16-lfpdppp-mx-engagement-package-final.md` | (this doc) | NEW |

**Pending Owner action:** 3 emails (tier-1 parallel send to OLIVARES / Basham / Sánchez Devanny) + 1 retainer signature (with the selected attorney). See §4 below for the step-by-step.

**Engagement window:** 2026-05-20 → 2026-06-20 (3 weeks; pre-GA gate).

**Budget envelope:** USD 2,000-5,000 retainer + USD 250-450/hr capped at 13 hours = USD 8,000 hard cap.

---

## §1 — The 4 deliverables (detailed)

### §1.1 Attorney/firm shortlist (`docs/legal/lfpdppp-mx-attorney-shortlist.md`)

5-candidate shortlist built from public sources (firm websites, Chambers LATAM, Legal500 LATAM, IAPP MX, INAI public register):

**Tier-1 (recommended for parallel send):**

| # | Firm | Lead attorney (illustrative) | Score | Indicative rate |
|---|---|---|---|---|
| 1 | OLIVARES | Abraham Díaz (Partner, IP + Data Protection) | **88/100** | $350-500/hr |
| 2 | Basham, Ringe y Correa | Juan Carlos Hernández (Partner, Privacy + Tech) | **86/100** | $300-450/hr |
| 3 | Sánchez Devanny | Mariano Calderón (Partner, Privacy + Tech) | **83/100** | $280-420/hr |

**Tier-2 (fallback if tier-1 declines or out-of-bandwidth):**

| # | Firm | Lead attorney (illustrative) | Score | Indicative rate |
|---|---|---|---|---|
| 4 | Galicia Abogados | Christian Lippert (Partner, TMT + Data Protection) | **78/100** | $320-480/hr |
| 5 | Creel, García-Cuéllar, Aiza y Enríquez | TMT Partner (consult directory) | **74/100** | $400-600/hr |

**Selection rubric:** 9 dimensions (7 primary capability gates ×7 = 70 max + 2 secondary ×3 = 30 max = 100 total). Minimum acceptance threshold ≥ 70/100. All 5 candidates clear the threshold; tier classification distinguishes recommended-send vs fallback.

**Per-candidate metadata:** firm, lead attorney (representative — Owner verifies actual partner at intake), contact email path (firm general routing), INAI familiarity, prior LFPDPPP SaaS-client signal, BAR registration (Barra Mexicana — Colegio de Abogados; cédula profesional verifiable via SEP RNPC), indicative hourly rate (USD), language (Spanish-native MX register + English bilingual required), and conflict-of-interest considerations (4 sub-processors + competitor + cap-table participants to verify before sending).

**Caveats** (`docs/legal/lfpdppp-mx-attorney-shortlist.md §0 disclaimer`): capability scores and firm metadata are best-effort estimates from publicly available sources; Owner MUST verify contact email + cédula profesional + conflict posture before send; fee bands are indicative at MX market rates 2026 and bind only after the firm returns a written proposal.

### §1.2 Engagement email template (`docs/legal/lfpdppp-mx-engagement-email-template.md`)

Bilingual (Spanish-primary + English-secondary) Mustache-templated email with 8 placeholders:

| Placeholder | Description |
|---|---|
| `{{attorney_name}}` | Attorney full name with title prefix (Lic. / Mtro. / Dr.) |
| `{{attorney_email}}` | Primary attorney contact email |
| `{{firm_name}}` | Firm display name |
| `{{attorney_top_strength}}` | Per-firm strength (from shortlist §1-§2) |
| `{{personalized_rationale}}` | 2-3 sentence personalized rationale paragraph (5 per-firm options provided) |
| `{{deadline_date}}` | Response deadline (send_date + 14 days) |
| `{{owner_pgp}}` | Owner's PGP key fingerprint |
| `{{calendar_link}}` | Calendar booking link for scoping call |

The template enumerates 9 proposal requirements (capability attestation, hourly rate within band, schedule commitment, CV, cédula profesional, conflict warranty, retainer template preference, redline mode, references) and links to the attached engagement packet (7 anexos including the engagement letter, retainer template, scoping packet, 3 review artifacts, and EVT-044 SOP).

Decline / out-of-bandwidth boilerplate replies included for tracker-state management.

5 per-firm `{{personalized_rationale}}` paragraphs already calibrated to each firm's shortlist strengths so Owner just selects the matching paragraph at send.

### §1.3 Tracker script + JSON (`scripts/admin/lfpdppp-mx-tracker.py` + `reports/lfpdppp-mx-tracker.json`)

State machine mirror of the wave-26 `scripts/pentest-rfp-tracker.py` pattern:

```
NOT_CONTACTED → EMAIL_SENT → RESPONSE_RECEIVED → IN_NEGOTIATION
              → RETAINER_SIGNED → OPINION_RECEIVED → ABSORBED   (terminal-success)
                                                   → DECLINED   (terminal-failure)
```

**Validation gates** (run `python3 scripts/admin/lfpdppp-mx-tracker.py --validate reports/lfpdppp-mx-tracker.json`):

- Schema: `schema_version` == 1.0, `updated` is ISO date, `attorneys` is a non-empty list of ≥ 5.
- Per-attorney required fields: `name`, `firm`, `contact_email`, `tier` (1 or 2), `state` (one of 8).
- Duplicate firm names rejected.
- Per-state date precondition: state advances require corresponding date fields populated (e.g. `RETAINER_SIGNED` requires `email_sent_date` + `response_received_date` + `retainer_signed_date`).
- Date monotonicity: forward-only chronology across `email_sent_date → response_received_date → retainer_signed_date → opinion_received_date → absorption_date`.
- Contact email shape: if state != `NOT_CONTACTED`, contact must be a valid `@`-bearing string.

**Initial state** (`reports/lfpdppp-mx-tracker.json`): 5 attorneys, all `NOT_CONTACTED`, with shortlist references (firm, lead attorney, tier, score, notes referencing the shortlist rationale).

**Rendering modes:**

- Default markdown: state rollup table + per-attorney status table + per-attorney notes + Owner action queue (auto-derived from state).
- `--json`: machine-readable rollup with state counts + tier-1/tier-2 status maps.
- `--validate`: schema + state-machine check; exit 0 green, 1 errors, 2 file/parse error.

### §1.4 Retainer template (`docs/legal/lfpdppp-mx-retainer-template.md`)

Engineering-side draft retainer template — the **negotiating baseline** ensuring CoreLink-side requirements (scope freeze, deliverables, IP, confidentiality, cap) are met regardless of which counter-party template wins at signature.

**Key clauses:**

| §  | Clause | Substance |
|----|--------|-----------|
| §2 | Scope freeze | Written opinion on §1-§7 of scoping packet; 8-13 attorney hours envelope; out-of-scope carve-outs (DPA / fiscal / litigation / other locales / implementation) |
| §3 | Deliverables | Written opinion (PDF ≥ 4 pages Spanish formal-legal); 3 signed EVT-044 PDFs (1 privacy notice + 2 breach templates); redlines if any; P0/P1/P2 issue list; 2-hr Q&A reserve |
| §4 | Fee structure | USD 2,000-5,000 retainer + USD 250-450/hr capped at 13 hrs; hard cap **USD 8,000**; 80% notification threshold (10.4 hrs); 50% deposit + 50% on delivery |
| §5 | IP | Work-for-hire assignment per Mexican Ley Federal del Derecho de Autor Art. 83; Attorney retains moral rights (Art. 21); Client use rights for spec corpus + auditor disclosure + customer NDA; Attorney pre-existing IP retained under non-exclusive license |
| §6 | Engagement window | **2026-05-20 → 2026-06-20** primary; opinion delivery T+21d from retainer signature; outer deadline 2026-09-01 |
| §7 | Confidentiality | 3-year NDA covering all anexos; Mexican attorney-client privilege under Código Federal de Procedimientos Civiles; standard carve-outs (already-public / independent / compelled); destruction or return within 30 days of termination |
| §8 | General | Mexican federal law governing; CAM arbitration Mexico City Spanish sole arbitrator; conflict-of-interest warranty (4 sub-processors + competitor + cap-table); independent contractor; for-convenience + for-cause termination; no third-party reliance |
| §10 | Anexo hash manifest | Engagement-packet BLAKE3/SHA-256 hashes pinned at signature for EVT-044 PDF attestation |

**Final retainer is pinned to the attorney's preferred template** at signature; this draft is the baseline against which the attorney's template is redlined (or vice-versa).

---

## §2 — Quality-gate run

All gates GREEN against this commit:

| Gate | Command | Result |
|---|---|---|
| Spec validation | `python3 scripts/validate_specs.py` | ✅ PASS (zero regression) |
| Reference validation | `python3 scripts/validate_references.py` | ✅ PASS (zero dangling) |
| Tracker schema + state validation | `python3 scripts/admin/lfpdppp-mx-tracker.py --validate reports/lfpdppp-mx-tracker.json` | ✅ PASS (5 attorneys; all `NOT_CONTACTED`) |
| Tracker markdown render | `python3 scripts/admin/lfpdppp-mx-tracker.py` | ✅ PASS (renders state rollup + per-attorney table + Owner action queue) |
| Tracker JSON rollup | `python3 scripts/admin/lfpdppp-mx-tracker.py --json` | ✅ PASS (machine-readable rollup with tier-1 + tier-2 status maps) |

**Freeze:** §3.b GA-blocker prep per task charter — engineering-side prep COMPLETE, no further engineering work required to unblock the Owner's send-3-emails-+-sign-1-contract path. DEBT-025 stays OPEN until absorption (Owner action chain below); engineering-side closure is achieved at this commit.

---

## §3 — Three-email Owner action sequence (the executable path)

### §3.1 Email 1 of 3 — OLIVARES (tier-1; recommended highest score 88/100)

| Field | Value |
|---|---|
| **Send target** | 2026-05-20 (T+0) |
| **To** | `mail@olivares.mx` (firm general; request routing to Privacy/Data Protection partner — illustrative attorney Abraham Díaz) |
| **Subject** | `[Engagement Request] CoreLink LFPDPPP MX legal review — written opinion + 3 EVT-044 PDFs (response by 2026-06-03)` |
| **Body template** | `docs/legal/lfpdppp-mx-engagement-email-template.md §1` (Spanish primary) + §2 (English secondary) |
| **Personalized rationale** | `docs/legal/lfpdppp-mx-engagement-email-template.md §5.1` (OLIVARES paragraph) |
| **Attachments** | 7 anexos PGP-encrypted ZIP (per email-template §0) — engagement letter, retainer template, scoping packet, privacy notice, 2 breach templates, review-process SOP |
| **Response deadline** | 2026-06-03 (T+14) |
| **Tracker action post-send** | Update `reports/lfpdppp-mx-tracker.json` → OLIVARES.state = `EMAIL_SENT`, `email_sent_date: "2026-05-20"`, populate `contact_email` |

### §3.2 Email 2 of 3 — Basham, Ringe y Correa (tier-1; 86/100)

Same metadata as §3.1 with:

- **To:** `info@basham.com.mx` (firm general; request routing to Data Privacy practice — illustrative attorney Juan Carlos Hernández)
- **Personalized rationale:** `docs/legal/lfpdppp-mx-engagement-email-template.md §5.2` (Basham paragraph)
- **Tracker action post-send:** Basham.state = `EMAIL_SENT`, `email_sent_date: "2026-05-20"`

### §3.3 Email 3 of 3 — Sánchez Devanny (tier-1; 83/100)

Same metadata as §3.1 with:

- **To:** `info@sanchezdevanny.com` (firm general; request routing to Privacy partner — illustrative attorney Mariano Calderón)
- **Personalized rationale:** `docs/legal/lfpdppp-mx-engagement-email-template.md §5.3` (Sánchez Devanny paragraph)
- **Tracker action post-send:** Sánchez Devanny.state = `EMAIL_SENT`, `email_sent_date: "2026-05-20"`

### §3.4 Owner pre-send checklist (per email)

Per `docs/legal/lfpdppp-mx-engagement-email-template.md §6`:

- [ ] PGP-encrypt the engagement-packet ZIP with `{{owner_pgp}}` published key.
- [ ] Fill all 8 Mustache placeholders.
- [ ] Verify recipient email on the firm website (not a directory aggregator) — phishing-vector risk reduction.
- [ ] Update tracker JSON to `EMAIL_SENT`.
- [ ] Set calendar reminder for `deadline_date` + 1 day = follow-up trigger.

### §3.5 Vetting + selection (T+14 to T+23)

After 14-day response window (2026-06-03):

1. Update tracker per firm: `RESPONSE_RECEIVED` if proposal arrives; `DECLINED` if firm declines or doesn't respond.
2. **7-day vetting window** (2026-06-03 → 2026-06-10):
   - Reference-check (1-2 prior clients per firm, NDA-permitting).
   - Cédula profesional verification via SEP RNPC.
   - Chambers / Legal500 cross-check.
   - Conflict-of-interest waiver if needed.
3. **Selection decision** (2026-06-10, T+21): rank by weighted-score match within cost envelope + soonest turnaround + cleanest conflict posture.
4. Move selected firm to `IN_NEGOTIATION`.

### §3.6 Tier-2 escalation (only if all 3 tier-1 decline / OOB)

If by 2026-06-10 no tier-1 firm is `RESPONSE_RECEIVED` with a viable proposal:

1. Send to Galicia Abogados (`info@galicia.com.mx`; §5.4 rationale).
2. Send to Creel (`info@creelmx.com`; §5.5 rationale).
3. New response deadline 2026-06-24 (14 days from tier-2 send).

---

## §4 — One-contract Owner action (retainer signature)

### §4.1 Retainer negotiation (T+21 → T+23)

After selecting a firm (state `IN_NEGOTIATION`):

1. Send the attorney `docs/legal/lfpdppp-mx-retainer-template.md` as the negotiating baseline.
2. Receive attorney's counter (their preferred template OR redlines on the CoreLink template).
3. Joint redline-resolution: ensure §1.4 above (scope freeze, deliverables, fees ≤ USD 8k, IP work-for-hire, 3-yr NDA, engagement window) are preserved in the executed instrument.
4. Pin §10 hash manifest with BLAKE3 hashes of the 6 anexos as they stand at signature.

### §4.2 Signature + deposit (target 2026-06-12)

| Action | Owner | Target |
|---|---|---|
| Retainer signed by both parties | HuGR Labs + selected attorney | 2026-06-12 (T+23) |
| 50% deposit paid (USD 1,000-2,500) | HuGR Labs | T+30d (2026-06-19) |
| Engagement packet shared with attorney with pinned hashes | HuGR Labs | T+1d from signature (2026-06-13) |
| **Tracker action:** state → `RETAINER_SIGNED` with `retainer_signed_date: "2026-06-12"` | HuGR Labs | At signature |

### §4.3 Opinion delivery (T+21 from signature = 2026-07-03)

| Action | Owner | Target |
|---|---|---|
| Mid-engagement check-in (Attorney status update) | Attorney | T+10d from signature (2026-06-22) |
| Written opinion + 3 EVT-044 PDFs + redlines delivered | Attorney | T+21d from signature (**2026-07-03**) |
| Final 50% payment | HuGR Labs | T+35d from signature (2026-07-17) |
| **Tracker action:** state → `OPINION_RECEIVED` with `opinion_received_date: "2026-07-03"` | HuGR Labs | At delivery |

### §4.4 Absorption (wave-26+ orchestrator)

| Action | Owner | Target |
|---|---|---|
| Privacy notice §4 / §6 corrections (if redlines applied) | Orchestrator | T+28d (wave-26+ absorption) |
| Metadata.yaml `legal_review.mx_attorney` + cédula recorded | Orchestrator | T+28d |
| `notice_text_hash` re-compute on any text change | Orchestrator | T+28d |
| EVT-044 PDFs uploaded to R2 `evidence-legal/v1.0.0-*.pdf` | Orchestrator | T+28d |
| DEBT-025 → CLOSED in debt register | Orchestrator | T+28d |
| **Tracker action:** state → `ABSORBED` with `absorption_date` | Orchestrator | T+28d |

---

## §5 — Risk register

| # | Risk | Likelihood | Impact | Mitigation |
|---|---|---|---|---|
| 1 | All 3 tier-1 firms decline / OOB | LOW | MED | Tier-2 escalation (Galicia / Creel) ready; same template stack reused without modification. |
| 2 | Cost overrun beyond USD 8k cap | LOW | MED | 80% threshold notification (10.4 hrs) per retainer §4.2; Owner authorizes scope-reduce OR overage before work proceeds. |
| 3 | Conflict-of-interest surfaces post-signature | LOW | HIGH | Retainer §8.2 warranty + 5-business-day notification + mid-engagement waiver path or withdrawal with prorated refund. |
| 4 | Attorney delivers Q1-Q4 partial only | MED | MED | Mid-engagement check-in at T+10d catches partial coverage early; remaining hours redirected within cap. |
| 5 | INAI verificación initiated during engagement | VERY LOW | HIGH | Retainer §8.4 for-cause termination + separate PPD-defense engagement carve-out (§2.3); does NOT affect scope freeze. |
| 6 | Privacy notice / breach template diff between hash pin and review | LOW | LOW | Retainer §10 manifest re-issue on any artifact change ≤ 1 business day; Attorney can pause review until hash refresh. |
| 7 | Outer deadline 2026-09-01 missed | LOW | HIGH | Tier-2 parallel send if tier-1 doesn't respond by T+14d; tier-3 escalation (Owner-personal-network) if all 5 candidates decline. |

---

## §6 — DEBT-025 disposition

`DEBT-025` in `specs/_audits/sealed/2026-05-15-debt-register.md` was added by wave-23 to track this engagement. Engineering-side state at this commit:

- **Engineering-side preparation:** COMPLETE (4 deliverables + this audit; nothing further to ship).
- **Owner action:** 3 emails + 1 contract signature (§3 + §4 above).
- **Absorption:** post-opinion-delivery (target 2026-07-03 + ~28 days for orchestrator absorption); DEBT-025 → CLOSED at absorption.

**No spec corpus changes** in this wave-28 step-6 (zero text changes to existing privacy notice / breach templates / metadata.yaml); doc-only addition of 4 new files + this audit. Privacy notice + breach templates retain `legal_review_status: PENDING_MX_ATTORNEY` until the EVT-044 PDFs are absorbed.

---

## §7 — Cross-references

- **Wave-23 scoping packet (the binding work statement):** `specs/_audits/sealed/2026-05-16-lfpdppp-mx-legal-review-package.md`
- **Wave-23 engagement letter (the binding scope freeze):** `docs/legal/lfpdppp-mx-engagement-letter-template.md`
- **Debt-register row:** `specs/_audits/sealed/2026-05-15-debt-register.md DEBT-025`
- **Pentest precedent stack (same shortlist→tracker→email→contract pattern):**
  - `specs/_audits/sealed/pentest-vendor-shortlist.md` (shortlist precedent)
  - `reports/pentest-rfp-tracker.json` + `scripts/pentest-rfp-tracker.py` (tracker precedent)
  - `docs/legal/pentest-rfp-email-template.md` (email precedent)
  - `docs/legal/pentest-engagement-contract-template.md` (contract precedent)
- **Review process SOP:** `legal/privacy-notice/REVIEW_PROCESS.md`
- **Privacy notice metadata:** `legal/privacy-notice/v1.0.0/metadata.yaml` (where attorney name + cédula + EVT-044 path will be recorded post-review)
- **Work item:** `specs/04_sprints/S11/work_items/WI-S11-004-privacy-notice-versioning-3-locales-diff-publication.md` §6.1.4

---

## §8 — Sign-off

**Engineering-side preparation:** COMPLETE.

**Owner action path:** 3 emails (parallel send 2026-05-20) → 14-day response window → vetting + selection → 1 retainer signature (target 2026-06-12) → opinion delivery (target 2026-07-03) → orchestrator absorption (wave-26+ post-2026-07-03).

**Quality gates** (per task charter):

- `python3 scripts/validate_specs.py` → ✅ PASS
- `python3 scripts/validate_references.py` → ✅ PASS
- `python3 scripts/admin/lfpdppp-mx-tracker.py --validate reports/lfpdppp-mx-tracker.json` → ✅ PASS (5 attorneys; all NOT_CONTACTED)

**Freeze:** §3.b GA-blocker prep — engineering-side prep frozen at this commit; subsequent changes only via wave-26+ absorption commit pinning attorney name + cédula + EVT-044 PDFs.

---

**End wave-28 step-6 LFPDPPP MX engagement package FINAL** — 5 candidates ranked, send-ready email + retainer + tracker; 3-email + 1-contract Owner action sequence enumerated; engineering-side DEBT-025 unblocked.

Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
