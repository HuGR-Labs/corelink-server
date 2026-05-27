# DEBT-026 — Pentest RFP Send Tracker + Vendor Selection Prep — Audit Doc

> **Doc kind:** wave-26 R-prep audit / procurement-tracker landing audit (no canonical front matter required — `_audits/` excluded from `validate_specs.py` per `scripts/validate_specs.py::SKIP_ALL`).
>
> **Author:** wave-26 R-prep DEBT-026 RFP-tracker agent (Claude Opus 4.7) — branch `wt/r-prep-debt-026-rfp-tracker`.
> **Base:** `main` @ `2a4e00c` ("merge wt/r-prep-tenant-config-cf-prod-wire into main (wave-25)" — wave-25 SEAL tip).
> **Scope:** Author the DEBT-026 (external pentest engagement) procurement-tracker stack so the Owner can execute the wave-25 SOW + vendor shortlist with minimal friction. Specifically: (a) state-machine-backed CLI tracker, (b) initial 5-vendor JSON status file, (c) RFP email template with per-vendor personalisation, (d) 10-item vendor due-diligence self-attestation checklist, (e) DEBT register reference uplift.
> **Cross-ref:** `specs/_audits/sealed/2026-05-16-pentest-engagement-scope-freeze.md` (wave-25 engagement-scope freeze; binding work statement), `specs/_audits/sealed/pentest-vendor-shortlist.md` (wave-25 broader 5-vendor shortlist), `docs/legal/pentest-engagement-contract-template.md` (wave-25 drop-in contract template), `specs/_audits/sealed/2026-05-15-debt-register.md` (DEBT-026 row).

---

## 1. Charter

Wave-25 sealed the engineering-side artifacts for the external pentest engagement (scope-freeze + contract template + 5-vendor shortlist). Wave-26 inherits the **procurement-execution** track: RFP send, response review, vendor selection, SOW countersign, kickoff. The wave-26 R-prep stream's job is to **lower the activation energy** for the Owner to execute that flow — turning a multi-doc spec corpus into a 3-step execution loop:

1. Pick vendor → look up contact in tracker JSON.
2. Customise RFP template `{{vendor_*}}` placeholders → send packet.
3. Update tracker state → render markdown for the next status sync.

This audit doc lands the engineering-side scaffolding for that loop; the actual RFP sends are user-bound per DEBT-026.

---

## 2. Deliverables shipped this stream

| # | Artifact | Path | Notes |
|---|---|---|---|
| 1 | RFP send-tracker CLI | `scripts/pentest-rfp-tracker.py` | 10-state machine; validate-only + render-markdown + JSON-rollup modes; tier-1 / tier-2 awareness derived from shortlist §6.2; per-state date precondition enforcement; date-ordering monotonic check; per-vendor action-queue derivation |
| 2 | Initial tracker JSON | `reports/pentest-rfp-tracker.json` | 5 vendors all `NOT_CONTACTED` — Bishop Fox (T1, 89), NCC Group (T1, 87), Trail of Bits (T1, 85), Cure53 (T2, 85), Doyensec (T2, 84). schema_version `1.0`. |
| 3 | RFP email template | `docs/legal/pentest-rfp-email-template.md` | Mustache placeholders; per-vendor `{{vendor_personalized_rationale}}` paragraphs already calibrated for all 5 shortlist vendors; decline + out-of-bandwidth boilerplate replies. |
| 4 | Vendor due-diligence checklist | `docs/legal/pentest-vendor-due-diligence.md` | 10-item self-attestation form organised: 5 corporate (registration, ≥4-yr practice, SOC 2 / ISO 27001 vendor self-audit, insurance ≥$5M E&O+cyber, beneficial-ownership + sanctions) + 3 privacy (DPA template, data residency + 12-mo retention, sub-processor disclosure) + 2 technical (Rust portfolio, CF Workers ≥2 engagements). Owner-filled §4 verification status table. |
| 5 | DEBT register reference uplift | `specs/_audits/sealed/2026-05-15-debt-register.md` | DEBT-026 row gains tracker reference + Owner action items table (one row per current `NOT_CONTACTED` vendor + the sequenced selection / SOW / kickoff actions). |
| 6 | This audit doc | `specs/_audits/sealed/2026-05-16-debt-026-rfp-tracker.md` | Land record. |

---

## 3. Tracker state-machine reference

The tracker enforces forward-only state transitions per the procurement timeline in scope-freeze §4. Visually:

```
NOT_CONTACTED ──► RFP_SENT ──► RESPONSE_RECEIVED ──► IN_NEGOTIATION
                                                          │
                                                          ▼
COMPLETED ◄── RETEST_PENDING ◄── IN_ENGAGEMENT ◄── KICKED_OFF ◄── SOW_COUNTERSIGNED

(DECLINED is reachable from any non-terminal state — terminal-failure.)
```

### 3.1 State definitions

| State | Meaning | Precondition dates populated |
|---|---|---|
| `NOT_CONTACTED` | Vendor on shortlist; no RFP sent yet | none |
| `RFP_SENT` | RFP packet emailed; awaiting response within 14-day deadline | `rfp_sent_date` |
| `RESPONSE_RECEIVED` | Vendor responded with proposal | `rfp_sent_date`, `response_received_date` |
| `IN_NEGOTIATION` | Owner reviewing + negotiating proposal | (same) |
| `SOW_COUNTERSIGNED` | Both parties signed the SOW | + `sow_countersigned_date` |
| `KICKED_OFF` | Onboarding + access provisioning underway (per scope-freeze §4 D-28 → D+0) | + `kickoff_date` |
| `IN_ENGAGEMENT` | Active testing in progress (Weeks 1-4 per scope-freeze §4) | (same) |
| `RETEST_PENDING` | Vendor performing retest at D+37 (per scope-freeze §4) | (same) |
| `COMPLETED` | Retest letter delivered at D+44 — DEBT-026 closable | + `retest_delivery_date` |
| `DECLINED` | Vendor declined to bid OR was disqualified per shortlist §6.3 | (any subset; terminal-failure) |

### 3.2 Validation guarantees

`python3 scripts/pentest-rfp-tracker.py --validate reports/pentest-rfp-tracker.json` enforces:

1. **Schema:** top-level keys `schema_version`, `updated`, `vendors`; `schema_version == "1.0"`.
2. **Vendor count:** ≥ 5 entries (the shortlist §6.2 baseline; supports adding new candidates without breaking validation).
3. **Per-vendor required fields:** `name`, `contact_email`, `state` always present; `state ∈ STATES`.
4. **Date precondition:** each state requires the dates per §3.1 to be populated (e.g., `RFP_SENT` requires `rfp_sent_date`).
5. **Date monotonic:** `rfp_sent_date ≤ response_received_date ≤ sow_countersigned_date ≤ kickoff_date ≤ retest_delivery_date`.
6. **Email shape:** any non-`NOT_CONTACTED` state requires a `contact_email` containing `@`.
7. **Duplicates:** vendor names MUST be unique.

Exit `0` if green; exit `1` with enumerated errors otherwise. Exit `2` on file / JSON parse error.

### 3.3 Render-modes

- **Default (markdown):** state rollup + per-vendor status table + per-vendor notes + derived Owner action queue. Render is sorted by state (advanced states first) then by tier (T1 before T2) then by name.
- **`--json`:** machine-readable rollup with state counts + tier-1 status map + tier-2 status map. Suitable for piping into a status digest.
- **`--validate FILE`:** validate-only; no render.
- **`--file PATH`:** override default `reports/pentest-rfp-tracker.json` location.

---

## 4. RFP packet contents (per shortlist §7)

The full RFP packet sent to each vendor includes 7 attached docs (per shortlist §7) + the new vendor due-diligence checklist as item #8:

1. `specs/_audits/sealed/2026-05-16-pentest-engagement-scope-freeze.md` — engagement-scope freeze (binding work statement).
2. `docs/legal/pentest-engagement-contract-template.md` — contract template (vendor's redlines welcomed via §1.3 scope-amendment register process).
3. `specs/_audits/sealed/2026-05-16-pre-ga-pentest-scope.md` — wave-19 technical baseline (46 attack chains + ASVS coverage + STRIDE/LINDDUN matrices).
4. `specs/_audits/sealed/pentest/PENTEST-EVIDENCE-PACKAGE.md` — day-1 vendor pack (handover artifacts).
5. `specs/_pentest/findings-template.md` — per-finding card template.
6. `specs/_audits/sealed/pentest/access-provisioning.md` — staging access provisioning playbook.
7. **Cover email** — generated from `docs/legal/pentest-rfp-email-template.md` with vendor placeholders filled.
8. **Vendor due-diligence checklist** — `docs/legal/pentest-vendor-due-diligence.md` (vendor returns completed within the 14-day response window).

---

## 5. Sequenced Owner action items

The Owner-side execution sequence, derived from scope-freeze §4 + shortlist §6.2 + this stream's deliverables:

### 5.1 RFP send (within next 7 days)

| # | Action | Reference | Target date |
|---|---|---|---|
| 1 | Look up primary contact email for **Bishop Fox** (tier-1) | bishopfox.com/contact or Owner network | 2026-05-17 |
| 2 | Look up primary contact email for **NCC Group** (tier-1) | nccgroup.com/contact-us or Cryptography Services partner channel | 2026-05-17 |
| 3 | Look up primary contact email for **Trail of Bits** (tier-1) | trailofbits.com/contact or cargo-audit / RustSec ecosystem network | 2026-05-17 |
| 4 | Fill `{{vendor_*}}` + `{{owner_*}}` placeholders in RFP email template (3 emails — one per tier-1) | `docs/legal/pentest-rfp-email-template.md` | 2026-05-17 |
| 5 | Generate PGP-encrypted ZIP of 8-doc RFP packet (per §4 above) | local | 2026-05-17 |
| 6 | Send 3 RFP emails simultaneously to tier-1 (+ cc tier-2 as "courtesy notice" per shortlist §6.2) | mail client | 2026-05-17 |
| 7 | Update `reports/pentest-rfp-tracker.json` for each sent: state → `RFP_SENT`, populate `rfp_sent_date` + `contact_email` | `python3 scripts/pentest-rfp-tracker.py --validate ...` after edits | 2026-05-17 |
| 8 | Commit tracker update to main with `chore(debt-026): RFP sent to <vendor>` style messages | git | 2026-05-17 |

### 5.2 Response review (14-day window: 2026-05-17 → 2026-05-30)

| # | Action | Reference | Target date |
|---|---|---|---|
| 9 | Triage each incoming proposal: state → `RESPONSE_RECEIVED` + populate `response_received_date` | tracker | 2026-05-17 → 2026-05-30 |
| 10 | For each `RESPONSE_RECEIVED`: vendor MUST submit the §3 due-diligence form + insurance COIs + consultant CVs + sample report | shortlist §7 | within 14 days of RFP send |
| 11 | Run 10-item due-diligence verification per §4 of the due-diligence checklist (7-day vetting window per shortlist §7) | `docs/legal/pentest-vendor-due-diligence.md` | 2026-05-31 → 2026-06-06 |
| 12 | If any disqualifying gap surfaces → state → `DECLINED` + send decline boilerplate per RFP email template §2 | tracker + email | as triggered |
| 13 | If all 10/10 PASS → state → `IN_NEGOTIATION` | tracker | as triggered |

### 5.3 Vendor selection (21-day post-submission window — by 2026-06-13)

| # | Action | Reference | Target date |
|---|---|---|---|
| 14 | Owner picks 1 winning vendor + 1 fallback (retained-no-payment) from `IN_NEGOTIATION` set | shortlist §6.2 | 2026-06-13 |
| 15 | Send selected vendor the SOW countersign packet (final contract from template) | `docs/legal/pentest-engagement-contract-template.md` | 2026-06-14 |
| 16 | Send fallback vendor the "retained" notice (no payment; held in reserve through SOW countersign date) | mail | 2026-06-14 |
| 17 | Send disqualified vendors decline boilerplate | RFP email template §2 | 2026-06-14 |

### 5.4 SOW countersign + kickoff (per scope-freeze §4)

| # | Action | Reference | Target date |
|---|---|---|---|
| 18 | Counter-signature on SOW by both parties → state → `SOW_COUNTERSIGNED` + populate `sow_countersigned_date` + `engagement_window` | tracker | 2026-06-14 |
| 19 | Vendor onboarding kickoff (D-28 → D+0 phase per scope-freeze §4) → state → `KICKED_OFF` + populate `kickoff_date` | tracker | 2026-06-14 → 2026-06-15 |
| 20 | Active testing kickoff (D+0 = 2026-06-15) → state → `IN_ENGAGEMENT` | tracker | 2026-06-15 |
| 21 | D+37 retest → state → `RETEST_PENDING` | tracker | 2026-07-22 |
| 22 | D+44 retest letter delivered → state → `COMPLETED` + populate `retest_delivery_date` (target 2026-07-29) → DEBT-026 closable per register | tracker + DEBT register | 2026-07-29 |

### 5.5 ADR-0034b parallel track (orthogonal to RFP flow)

| # | Action | Reference | Target date |
|---|---|---|---|
| 23 | Nominate `(a nomear)` Security Lead per ADR-0034b 2-key path (wave-24 DEFER #2) — else Owner-only via solo-tier waiver | `specs/03_architecture/adrs/ADR-0034-prr-staffing-waiver-solo-tier.md` | 2026-06-01 (per scope-freeze §10 pending-signature note) |

---

## 6. Quality gates

| Gate | Command | Status |
|---|---|---|
| Tracker validation | `python3 scripts/pentest-rfp-tracker.py --validate reports/pentest-rfp-tracker.json` | GREEN (5 vendors valid) |
| Tracker render-markdown | `python3 scripts/pentest-rfp-tracker.py` | GREEN |
| Tracker render-JSON | `python3 scripts/pentest-rfp-tracker.py --json` | GREEN |
| Spec validator | `python3 scripts/validate_specs.py` | GREEN (no new canonical-source docs; `_audits/` + `reports/` outside SKIP_ALL scope) |
| Reference validator | `python3 scripts/validate_references.py` | GREEN (no new INV / CTRL / PAT IDs introduced) |

---

## 7. Caveats + limitations

1. **No live vendor outreach performed.** The 5 `contact_email` fields remain `null`; Owner fills these from public contact channels + private network during step 5.1 #1-#3.
2. **PGP fingerprint placeholder unfilled.** `{{owner_pgp_fingerprint}}` in the email template must be replaced with the actual Owner key — generate via `gpg --fingerprint`.
3. **Tracker is append-only-ish.** The state machine permits forward transitions + DECLINED-from-any. No "back-tracking" supported by design (procurement-quality: a `SOW_COUNTERSIGNED` can't revert to `IN_NEGOTIATION` without a doc-level rewind). If a re-negotiation occurs, treat it as a new RFP cycle on a new vendor row.
4. **No Statement-of-Work-template authoring.** The contract template (wave-25) is contract-level — the per-engagement SOW (Schedule A binding) is the scope-freeze doc itself. Vendor-side proposal text becomes Schedule B.
5. **Due-diligence is self-attestation primary.** Owner-side verification §4 happens during the 7-day vetting window. Reference calls + independent sanctions screening are part of that vetting.
6. **No tooling for batch-RFP-send.** Owner sends 3 individual emails — by design, to preserve per-vendor personalisation per RFP template §1.
7. **Currency-risk clause missing for Cure53.** Cure53 prices in EUR (per shortlist §5); if selected, SOW countersign needs a currency-fix clause (e.g., fix at countersign-date FX rate ± 5 % collar).

---

## 8. Wave-26 absorption sequence

Per DEBT-026 charter "wave-26 absorption pre-GA":

1. Wave-26 streams open with this audit doc + tracker as the live execution artifact.
2. Each tracker state transition → commit on `main` with `chore(debt-026): <vendor> → <STATE>` message.
3. Per-finding remediation worktrees (`wt/r-pentest-remediate-<finding-id>`) open as findings land in §5.2 of scope-freeze.
4. ASVS gap analysis CSV (vendor §5.3 deliverable) absorbs into `specs/03_architecture/security_model.md` at engagement-close.
5. Retest letter (vendor §5.4 deliverable) absorbs into `specs/_audits/sealed/2026-05-16-ga-readiness-final.md` as the DEFER #3 closure note.
6. DEBT-026 flips OPEN → CLOSED in the canonical register on the retest-letter commit.
7. GA cutover unblocks per `RB-GA-CUTOVER.md` (retest letter is a hard gate).

---

## 9. Cross-references

- `specs/_audits/sealed/2026-05-16-pentest-engagement-scope-freeze.md` — wave-25 engagement-scope freeze (binding work statement; Schedule A of the contract).
- `specs/_audits/sealed/pentest-vendor-shortlist.md` — wave-25 broader 5-vendor shortlist (capability scores: BF 89, NCC 87, ToB 85, Cure53 85, Doyensec 84).
- `docs/legal/pentest-engagement-contract-template.md` — wave-25 drop-in contract template.
- `docs/legal/pentest-rfp-email-template.md` — this stream; RFP send template with per-vendor personalisation.
- `docs/legal/pentest-vendor-due-diligence.md` — this stream; 10-item vendor self-attestation + Owner verification.
- `scripts/pentest-rfp-tracker.py` — this stream; state-machine CLI tracker.
- `reports/pentest-rfp-tracker.json` — this stream; live tracker JSON (5 vendors initial).
- `specs/_audits/sealed/2026-05-15-debt-register.md` — DEBT-026 row uplifted with tracker reference + Owner action table.
- `specs/_audits/sealed/2026-05-16-pre-ga-pentest-scope.md` — wave-19 SEALED technical baseline (46 chains + ASVS coverage).
- `specs/_audits/sealed/pentest/PENTEST-EVIDENCE-PACKAGE.md` — day-1 vendor handover pack.
- `specs/_pentest/findings-template.md` — per-finding card template.
- `specs/_audits/sealed/pentest/access-provisioning.md` — staging access provisioning playbook.
- `specs/03_architecture/adrs/ADR-0034-prr-staffing-waiver-solo-tier.md` — solo-tier waiver path (Security Lead nomination dependency).
- `specs/_runbooks/RB-PENTEST-FINDING-RESPONSE.md` — finding-triage runbook (live during engagement).
- `specs/_runbooks/RB-GA-CUTOVER.md` — cutover runbook (retest letter is hard gate).

---

## Change log

| Version | Date | Author | Change |
|---|---|---|---|
| 1.0.0 | 2026-05-16 | Gustavo Schneiter (via Claude Opus 4.7, wave-26 R-prep stream) | Initial RFP-tracker + vendor-selection-prep land. Ships scripts/pentest-rfp-tracker.py (10-state machine CLI w/ validation + markdown + JSON rollup) + reports/pentest-rfp-tracker.json (5-vendor initial) + docs/legal/pentest-rfp-email-template.md (Mustache, 5 vendor rationales) + docs/legal/pentest-vendor-due-diligence.md (10-item: 5 corporate + 3 privacy + 2 technical) + DEBT register row uplift. Sequences 22 Owner action items across RFP send → response review → vendor selection → SOW countersign → kickoff → retest. |

---

**End audit doc.**

Signed-off-by: Gustavo Schneiter <gustavo@humangr.com>
Co-Authored-By: Claude Opus 4.7 <noreply@anthropic.com>
