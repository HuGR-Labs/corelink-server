---
id: "RB-SUBPROCESSOR-CHANGE"
type: "runbook"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-05-15"
updated: "2026-05-27"
sprint: "R-prep"
parent_wi: "WI-R-PREP-SUBPROCESSOR-SYNC"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from: ["VENDOR-RISK-REGISTER-2026-05-15", "RB-VENDOR-RISK-QUARTERLY-REVIEW"]
tags: ["runbook", "subprocessor", "lgpd", "gdpr", "privacy", "ctrl-priv-021", "legal-review"]
---

> **Post Wave 35 Phase 2 update 2026-05-27:** `corelink-privacy-sub-processor-emit` was absorbed into `corelink-privacy` via inline `mod sub_processor;` per SEAL `specs/_audits/sealed/2026-05-26-w35-p2-privacy-absorption.md`. Canonical consumer path is now `corelink_privacy::sub_processor::*`. Operational references using the absorbed crate path still work for HISTORICAL log/grep reference but new automation should use the umbrella.

<!-- forensics-backlink -->
> **Forensics:** see `docs/internal/FORENSICS-GUIDE.md` §10.

# RB-SUBPROCESSOR-CHANGE — Sub-processor add / change / remove runbook

> **Status:** ACTIVE. Owned by VP-Sec (technical) + DPO (regulatory).
> Triggered whenever the internal
> [Vendor Risk Register](../_compliance/VENDOR-RISK-REGISTER.md) is amended
> in a way that affects the public sub-processor list (rows where
> `Cat. ∈ {C, I}` and `Data sharing ≠ none`).

## 1. Purpose

Operationalise the end-to-end flow that keeps three artifacts in lock-step:

1. **Internal source of truth** — `specs/_compliance/VENDOR-RISK-REGISTER.md`.
2. **Public surface** — `apps/docs/docs/trust/subprocessors.mdx`.
3. **Customer broadcast** — `corelink.privacy.subprocessor.notify_required`
   CloudEvents fan-out, starting the **30-day grace clock** required by:
   - **LGPD Art. 27 §4º** — controller informed before changes that affect
     the agreed processing chain.
   - **LGPD Art. 39** — sub-processor list disclosure obligations.
   - **GDPR Art. 28 §2** — controller right to object before a new
     sub-processor is engaged.
   - **DPA §6.2 / §6.4** (contractual ≥30-day notice + objection window).

## 2. Roles

| Role | Responsibility |
|---|---|
| VP-Sec | Technical owner — vendor DD packet, register update, residual-risk score, failover-map row. |
| DPO / Privacy-Officer | Regulatory owner — DPA review, customer-broadcast wording, objection-window adjudication. |
| Eng-Lead | Implementation owner — register PR, generator + workflow health, broadcast pipeline. |
| Legal Counsel | Reviews DPA-impacting clauses and termination-for-cause path. |
| Customer Success | Routes inbound objection tickets to DPO within the 14-day SLA (DPA §6.4). |

## 3. Trigger taxonomy

| Trigger | Direction | Grace window | Public-page impact | Broadcast required |
|---|---|---|---|---|
| New vendor onboarded (Critical / Important, customer-data flow) | **add** | **30 calendar days** | Yes (new row) | **Yes** |
| Vendor service-scope change (new data class flowing) | **change** | 30 calendar days *if* new data class is customer PII / payment | Yes (row updated) | Yes |
| Vendor offboarded | **remove** | 0 (no grace required; informational only) | Yes (row removed) | Informational |
| Vendor metadata-only change (region note, DPA URL refresh) | **patch** | 0 | Yes (cosmetic) | No |
| Standard-tier-only internal vendor (no customer data) | **internal** | n/a | No | No |

> **Decision rule:** if in doubt, treat as **add** and start the 30-day
> clock — over-notification is regulatorily safe; under-notification is
> a CTRL-PRIV-021 violation.

## 4. End-to-end procedure

### 4.1 Step 1 — vendor due-diligence packet

1. Author / refresh `specs/_compliance/vendor-dd/DD-<VENDOR>.md` (template
   per `VENDOR-RISK-METHODOLOGY.md §5`).
2. Land DD packet in a feature branch; obtain VP-Sec sign-off.

### 4.2 Step 2 — register update

1. Edit `specs/_compliance/VENDOR-RISK-REGISTER.md` §2 (add / change /
   remove the table row) and bump `version` + `updated`.
2. Recompute residual risk per `VENDOR-RISK-METHODOLOGY.md §4`.
3. Update the failover-map row (§3) if the vendor is Critical.

### 4.3 Step 3 — public-page regeneration (automated)

1. The PR triggers `.github/workflows/subprocessors-sync.yml`
   **drift-check** job which runs `scripts/gen-public-subprocessors.py
   --check` and fails the PR if the public MDX is not regenerated.
2. Regenerate locally:

   ```bash
   python3 scripts/gen-public-subprocessors.py
   ```

3. Commit `apps/docs/docs/trust/subprocessors.mdx` alongside the register
   change.

### 4.4 Step 4 — customer-broadcast event emission

On merge to `main`, the workflow's `regenerate-and-pr` job invokes
`scripts/subprocessor-change-notify.py` which:

1. Diffs `HEAD~1:VENDOR-RISK-REGISTER.md` against
   `HEAD:VENDOR-RISK-REGISTER.md`.
2. For each addition / removal, emits a JSONL CloudEvent of type
   `corelink.privacy.subprocessor.notify_required` with:
   - `change_kind` ∈ `{added, removed}`
   - `detected_at` (UTC now)
   - `effective_at` (= `detected_at + 30 days` UTC-midnight aligned for
     adds; = `detected_at` for removes)
   - `legal_basis = ["LGPD Art. 27 §4º", "LGPD Art. 39",
                     "GDPR Art. 28 §2", "DPA §6.2 / §6.4"]`
   - `purpose = "legal_obligation"` — **NOT** consent-revocable
     (`INV-SUB-PROCESSOR-BROADCAST-ALL-PLANS`, privacy_model §5.6.1).
   - `broadcast_target_plans = [free, pro, team, enterprise, lighthouse]` —
     all five canonical plans receive.
3. Uploads the JSONL artifact for the production
   `corelink-privacy-sub-processor-emit` consumer (R2 audit-`<region>`
   Object Lock 7y + D1 `sub_processor_broadcast_log` fan-out).

### 4.5 Step 5 — DPO confirmation + 30-day grace clock

1. DPO receives the workflow notification (PagerDuty `dpo-low-urgency`
   service + Slack `#privacy-ops`).
2. DPO verifies broadcast text, language locales (pt-BR, en-US, es-MX),
   and DPA citations.
3. DPO triggers fan-out approval inside the admin plane within **24 hours**
   of merge.
4. Customers receive the broadcast email + status-page entry + tenant-side
   "Sub-processor change pending" banner.
5. **Objection window**: 30 calendar days from `detected_at`. Inbound
   objections are routed to DPO via Customer Success per DPA §6.4.

### 4.6 Step 6 — effective date

1. On `effective_at` the new vendor is permitted to begin processing.
2. If any objection remains unresolved at `effective_at - 7d`, DPO
   escalates to Risk Committee (`RB-LIGHTHOUSE-CUSTOMER-INCIDENT.md`
   pattern) and either:
   - Defers `effective_at` until resolution, OR
   - Grants the customer the termination-for-cause right per DPA §6.4
     (refund of unused prepaid term + DSR-driven export).
3. Production wiring (CD pipeline) only flips the vendor's "active" bit
   in the runtime allowlist once DPO sets `objection_window_closed = true`.

### 4.7 Step 7 — post-effective audit + closure

1. Verify the `SubProcessorChangedPayload` CloudEvent ingested into
   audit-`<region>` (Object Lock 7y).
2. Confirm `sub_processor_broadcast_log` rows recorded `delivery_status
   ∈ {Delivered, Bounced, Suppressed}` for every targeted recipient.
3. Close the change ticket in the quarterly-review tracker
   (`RB-VENDOR-RISK-QUARTERLY-REVIEW.md` §5).

## 5. Failure modes & recovery

| Symptom | Cause | Recovery |
|---|---|---|
| Drift-check fails on PR | Author forgot to regen MDX | `python3 scripts/gen-public-subprocessors.py` and re-push |
| Auto-PR not opened on main | `GITHUB_TOKEN` perms or path filter miss | Manual: `workflow_dispatch` re-run; verify `paths:` in workflow |
| Notify events not emitted | Diff baseline missing (e.g. shallow clone) | Workflow uses `fetch-depth: 0`; if hit locally, pass `--prev` explicitly |
| Customer objection received | Material processing change | Route to DPO; pause `effective_at`; consider termination-for-cause path |
| Removal needs grace too (rare; e.g. data-export window) | Vendor offboarding with retained obligations | DPO can manually pass `--effective-at <ISO>` to the notify script to set a custom grace |

## 6. Evidence & audit trail

- **Register history** — Git log on `specs/_compliance/VENDOR-RISK-REGISTER.md`.
- **Public-page history** — Git log on `apps/docs/docs/trust/subprocessors.mdx`
  (auto-generated; every change references the register commit).
- **CloudEvents audit** — R2 `audit-<region>/sub_processor/*` (Object Lock
  7y; CTRL-PRIV-021 + INV-AUDIT-APPEND-ONLY).
- **Broadcast log** — D1 `sub_processor_broadcast_log` (5-arm delivery
  state machine; idempotent on `(broadcast_id, tenant_id,
  recipient_email_hash, notification_type)`).
- **Objection log** — D1 `sub_processor_objection` (5-arm state machine:
  Filed / UnderReview / Resolved / Escalated / TerminationGranted).

## 7. Cross-references

- Register: [`specs/_compliance/VENDOR-RISK-REGISTER.md`](../_compliance/VENDOR-RISK-REGISTER.md)
- Methodology: [`specs/_compliance/VENDOR-RISK-METHODOLOGY.md`](../_compliance/VENDOR-RISK-METHODOLOGY.md)
- Quarterly cycle: [`RB-VENDOR-RISK-QUARTERLY-REVIEW.md`](./RB-VENDOR-RISK-QUARTERLY-REVIEW.md)
- DPA-change companion: [`RB-DPA-CHANGE.md`](./RB-DPA-CHANGE.md)
- Compliance matrix: [`specs/03_architecture/compliance_matrix.md`](../03_architecture/compliance_matrix.md)
  §7 (sub-processor posture) + §4 (LGPD) + §5 (GDPR).
- Roadmap: [`ROADMAP-TO-GA.md`](../../ROADMAP-TO-GA.md) (R-prep regulatory readiness).
- Generator: `scripts/gen-public-subprocessors.py`
- Notify hook: `scripts/subprocessor-change-notify.py`
- Workflow: `.github/workflows/subprocessors-sync.yml`
- Emitter crate: `crates/corelink-privacy-sub-processor-emit/`
- Public page: `apps/docs/docs/trust/subprocessors.mdx`

## 8. Change log

| Version | Date | Author | Notes |
|---|---|---|---|
| 1.0.0 | 2026-05-15 | Gustavo Schneiter (R-prep subprocessor-sync swarm) | Initial runbook. Pairs with generator + notify script + drift-gate workflow. |
