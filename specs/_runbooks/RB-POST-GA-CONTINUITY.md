---
id: "RB-POST-GA-CONTINUITY"
type: "runbook"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-16"
updated: "2026-05-16"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "p1", "ga", "post-ga", "continuity", "thaw", "wave-27", "r-prep"]
---

<!-- forensics-backlink -->
> **Forensics:** see `docs/internal/FORENSICS-GUIDE.md` §2.2.

# RB-POST-GA-CONTINUITY — 30-day Post-GA Operational Continuity Playbook

> **Severity floor:** **P1** — this is the canonical *post-cutover* operational continuity runbook. It picks up at `RB-GA-CUTOVER.md` §3.11 completion (the moment GA posture is declared) and runs until **T+30d** (which closes the wave-27 stabilisation window and authorises the first quarterly framework review per `2026-05-15-framework-v1-0-0-ga-audit.md` §11.5).
>
> **Companion docs.**
> - `specs/_runbooks/RB-GA-CUTOVER.md` §6 — the *cutover* runbook's post-cutover stub; this playbook is the **canonical expansion** of that stub.
> - `specs/_audits/sealed/2026-05-16-ga-1-feature-freeze.md` §6 — feature-freeze **thaw conditions** that this runbook **operationalises** (§5 below is the executable form of that audit's §6).
> - `docs/internal/customer-success-playbook.md` §4 — the daily CS metric set that this runbook **gates** (§2 and §4 below consume those metrics).
> - `specs/_audits/sealed/2026-05-15-framework-v1-0-0-ga-audit.md` §11.5 + §11.8 — wave-22 90-day rolling quarterly cadence anchored on FROZEN cut (this runbook's §6 prepares the first quarterly slot).
> - `scripts/post-ga-monitor.sh` — the executable companion that reads §§1–4 criteria daily and emits a markdown digest under `reports/post-ga/`.
>
> **Window:** T+0 (cutover §3.11 complete) → T+30d. After T+30d, ownership transitions to standard ops (weekly compliance digest + monthly business review).
>
> **Regulatory note (NOT a breach event).** Post-cutover monitoring is the *normal* operating mode. The GDPR Art. 33 / LGPD ANPD 72 h breach notification clock starts only on a SEV-0 / SEV-1 with confirmed personal-data impact — at which point this runbook **steps aside** and `RB-GA-LAUNCH-ROLLBACK.md` D+1 / D+7 / D+30 triggers + `RB-DSR-GDPR.md` / `RB-DSR-LGPD-FULL.md` take primacy.

---

## §1. T+0 to T+24h — Greenlight verification + status page transition

The first 24 hours after `RB-GA-CUTOVER.md` §3.11 are the **highest-attention window**. The 11-step cutover sequence has completed but the system has not yet accumulated 24h of GA-posture telemetry. The objective is to confirm that the greenlight composite recording rule held through the first night.

### 1.1 Greenlight composite re-verification (T+0, T+6h, T+12h, T+24h sampling)

| # | Action | Owner | Verification |
|---|---|---|---|
| 1.1.1 | Re-evaluate the 6 greenlight criteria (G1..G6) per `dashboards/alerts/dash-ga-greenlight.yml` recording rules at T+0, T+6h, T+12h, T+24h | SRE Lead | Greenlight composite == 1 at all 4 sample points; any drop to 0 escalates per §1.4 |
| 1.1.2 | Snapshot p99 latency, audit-chain integrity, SEV-0/1 count, customer-ack tally, Neon shadow lag, DSR cron success — store under `reports/post-ga/T-plus-24h/` | SRE Lead | All 6 metric files present + within green band per `slo_catalog.md` |
| 1.1.3 | Confirm `dash-slo-multi-burn.yml` shows zero burn-rate page in the first 24h | SRE Lead | Grafana dashboard screenshot or `prometheus_query` output |

### 1.2 Initial customer acknowledgement (T+1h to T+12h)

| # | Action | Owner | Verification |
|---|---|---|---|
| 1.2.1 | Send `CUTOVER-T-PLUS-1-PILOT.md` to all 5 pilot tenants per `RB-GA-CUTOVER.md` §7.3 (welcome-to-GA + what changed + how to use new tier features) | VPProduct | Postmark delivery confirms + Slack `#ga-cutover-comms` thread updated |
| 1.2.2 | Collect first-customer-ack signal from at least 3 of 5 pilot tenants (per `RB-GA-CUTOVER.md` §4 G4 ≥ 5 ack threshold — confirm the cutover-window count was met) | VPProduct | Pilot tracker (`docs/internal/customer-success-playbook.md` Appendix A) updated with ack timestamps |
| 1.2.3 | If any pilot tenant signals a P1+ issue within the first 12h, route per `docs/internal/customer-success-playbook.md` §5 escalation tree | CS owner | Escalation logged with tier / time / resolver / outcome / MTTR |

### 1.3 Status page transition to OPERATIONAL

| # | Action | Owner | Verification |
|---|---|---|---|
| 1.3.1 | Confirm `STATUS-PAGE-GA-OPERATIONAL.md` banner has been published at §3.11 complete (per `RB-GA-CUTOVER.md` §7.4) | SRE Lead | Statuspage screenshot showing green banner |
| 1.3.2 | Confirm the green banner is **sticky for 7d** (per `RB-GA-CUTOVER.md` §7.4 row 2) | SRE Lead | Statuspage banner config inspected; sticky-until timestamp ≥ T+7d |
| 1.3.3 | Verify status page subscribers list received `GA-LAUNCH-BLOG.md` cross-link within T+24h | VPProduct | Statuspage subscriber broadcast log |

### 1.4 Escalation triggers in §1 window

A drop on any §1.1 sample point triggers one of:

- **Greenlight composite == 0 at any T+0/+6/+12/+24 sample** → page on-call SRE + invoke `RB-GA-LAUNCH-ROLLBACK.md` D+1 trigger T1-1 evaluation.
- **Any SEV-0 in the window** → `RB-GA-LAUNCH-ROLLBACK.md` D+1 trigger T1-2 path; this runbook pauses until the D+1 outcome is decided.
- **Any pilot tenant SEV-1 attributable to the cutover** → CS owner pages Eng on-call per `docs/internal/customer-success-playbook.md` §5 Tier 2; coordinate with SRE Lead before T+24h baseline snapshot.

### 1.5 §1 exit criteria

To exit §1 and enter §2, **all** of the following must hold at T+24h:

1. Greenlight composite == 1 at all 4 sample points (T+0, +6h, +12h, +24h).
2. Zero SEV-0 in the T+0..T+24h window.
3. At least 3 of 5 pilot tenant acks collected.
4. Status page OPERATIONAL banner published and sticky-confirmed.
5. T+24h baseline snapshot saved under `reports/post-ga/T-plus-24h/`.

A miss on any item is **NOT** automatic rollback — it routes to §1.4 with the appropriate trigger.

---

## §2. T+24h to T+72h — SLO baseline capture + first onboarding ack

The 24h..72h window is the **first full SLO observation period** with the GA-posture telemetry pipeline producing real customer traffic. The objective is to compare against the wave-15 staging SLO baseline and lock the first 7-day baseline envelope.

### 2.1 SLO baseline capture (T+48h, T+72h)

| # | Action | Owner | Verification |
|---|---|---|---|
| 2.1.1 | At T+48h, run `scripts/post-ga-monitor.sh --window=24-72h --emit-baseline` to produce `reports/post-ga/T-plus-72h-baseline.md` | SRE Lead | Markdown digest committed under `reports/post-ga/` |
| 2.1.2 | Compare 72h GA telemetry against `specs/04_sprints/S15/` staging benchmarks (p50/p95/p99 latency per endpoint; throughput; error-budget burn) | SRE Lead | Per-row delta within ±20% of staging baseline; any row > +20% flagged in digest as YELLOW |
| 2.1.3 | If any SLO row trips RED (> +50% vs staging), open a `priority:P1` `track:ga-blocker` issue and route under feature-freeze `§3.b` exception | SRE Lead + Owner | Issue link in digest; freeze-monitor entry created |
| 2.1.4 | Lock the **first 7-day baseline envelope** under `reports/post-ga/baseline-envelope-v1.json` (used by §3.2 weekly summary comparison) | SRE Lead | JSON file with per-SLO p50/p95/p99 + burn-rate envelope |

### 2.2 First customer onboarding acknowledgement

| # | Action | Owner | Verification |
|---|---|---|---|
| 2.2.1 | Confirm at least 1 of 5 pilot tenants has completed a CAS write at GA-posture (per `customer-success-playbook.md` §3 T+24..T+72h milestone) | CS owner | Pilot tracker updated with first-GA-CAS-write timestamp |
| 2.2.2 | Run `RB-AUDIT-EXPORT-VERIFY-FAILED.md` §2 spot-verifier against the first pilot tenant's GA-posture audit chain | Security Lead | Verifier output logged; integrity == OK |
| 2.2.3 | Send onboarding ack survey (1-question lightweight: "Did anything break for you during cutover?") to all 5 pilot tenants | CS owner | Postmark delivery + at least 4 of 5 responses by T+72h |

### 2.3 D+1 trigger window closure (cross-ref `RB-GA-CUTOVER.md` §6.2.3)

| # | Action | Owner | Verification |
|---|---|---|---|
| 2.3.1 | At T+72h, confirm no `RB-GA-LAUNCH-ROLLBACK.md` D+1 trigger fired in the 72h window | SRE Lead + Owner | Trigger sweep doc filed under `reports/post-ga/T-plus-72h-d1-sweep.md` |
| 2.3.2 | Append D+1 closure attestation to `specs/_audits/2026-MM-DD-ga-cutover-execution-attestation.md` (the cutover-time attestation doc per `RB-GA-CUTOVER.md` §8.1) | SRE Lead + Owner | 2-key signature appended |

### 2.4 §2 exit criteria

To exit §2 and enter §3, **all** of the following must hold at T+72h:

1. Greenlight composite == 1 sustained from T+0 to T+72h (no transient drops).
2. SLO baseline envelope v1 locked under `reports/post-ga/`.
3. Zero RED SLO rows vs staging benchmarks (YELLOW acceptable + tracked).
4. At least 1 first-GA-CAS-write per the pilot dashboard.
5. D+1 trigger window closure attestation 2-key signed.

---

## §3. T+72h to T+7d — Weekly summary + adversarial absorption + DEBT prep

The 72h..7d window is the **first full week of GA operation**. The objective is to produce the first weekly compliance digest under GA posture, absorb any adversarial-review findings into the DEBT register, and prepare the quarterly cadence anchor commit.

### 3.1 First weekly compliance digest under GA posture (T+7d Monday slot)

| # | Action | Owner | Verification |
|---|---|---|---|
| 3.1.1 | Run `RB-COMPLIANCE-WEEKLY-REVIEW.md` Monday triage with the first GA-posture digest | Compliance Lead | `specs/_compliance/weekly-digests/YYYY-MM-DD.md` filed |
| 3.1.2 | Run `RB-COMPLIANCE-WEEKLY-REVIEW.md` §10 TLA ratchet floor check — must hold at the wave-26 baseline (no regression) | Compliance Lead | Digest §10 shows no HARD FAIL |
| 3.1.3 | Run `scripts/post-ga-monitor.sh --window=72h-7d --emit-summary` for the customer-facing weekly summary | SRE Lead | `reports/post-ga/T-plus-7d-weekly-summary.md` |
| 3.1.4 | Cross-publish weekly summary to pilot tenants (per `customer-success-playbook.md` §4 daily-review cadence; this is the GA-posture roll-up) | CS owner | Postmark delivery |

### 3.2 Adversarial-review absorption (one-shot at T+5d ± 1d)

| # | Action | Owner | Verification |
|---|---|---|---|
| 3.2.1 | Re-run the most-recent sprint-close adversarial review (e.g., `specs/_audits/sealed/2026-05-14-adversarial-summary-s13.md` template) against GA-posture telemetry — focus on findings tagged `defer-to-post-GA` | Owner + tech-lead persona | New audit doc filed at `specs/_audits/<DATE>-post-ga-adversarial-absorption.md` |
| 3.2.2 | Each finding routes to one of: (a) DEBT register row (P2/P3), (b) §3.b GA-blocker fix under freeze exception, (c) `post-thaw` queue per `2026-05-16-ga-1-feature-freeze.md` §6 | Owner | Routing table in absorption doc; each finding has explicit class |
| 3.2.3 | If any finding routes to §3.b GA-blocker, open the issue + commit-trailer `FREEZE-EXCEPTION: P1-ga-blocker` per freeze §3.b protocol | Owner + on-call SRE | Issue link + 2-key recorded in `reports/ga-freeze-monitor.json` |

### 3.3 DEBT register quarterly cycle prep

| # | Action | Owner | Verification |
|---|---|---|---|
| 3.3.1 | Inventory all DEBT rows added during the wave-17..wave-26 freeze prep period; classify P0/P1/P2/P3 | Owner | `specs/_audits/<DATE>-debt-register-q1-prep.md` filed |
| 3.3.2 | Confirm no P0 DEBT row sits past Target without an explicit waiver or §3 freeze-exception entry | Compliance Lead | `RB-COMPLIANCE-WEEKLY-REVIEW.md` §14 (debt burn-down) row GREEN |
| 3.3.3 | Set the **first quarterly cycle anchor** = the SHA of the GA cutover §3.11 commit (per `framework-v1-0-0-ga-audit.md` §11.5 "90-day rolling quarterly cadence anchored on FROZEN cut") | Owner | Anchor SHA recorded in §6.1 below + in `reports/post-ga/quarterly-anchor.txt` |

### 3.4 D+7 trigger window closure (cross-ref `RB-GA-CUTOVER.md` §6.3.3)

| # | Action | Owner | Verification |
|---|---|---|---|
| 3.4.1 | At T+7d, confirm no `RB-GA-LAUNCH-ROLLBACK.md` D+7 trigger fired in the T+0..T+7d window | SRE Lead + Owner | Trigger sweep doc filed under `reports/post-ga/T-plus-7d-d7-sweep.md` |
| 3.4.2 | Run the cutover retro meeting per `RB-GA-CUTOVER.md` §6.3.4 (60min retro with all signers + on-call SREs) | Owner | Retro doc sealed at `specs/_audits/<DATE>-ga-cutover-retro.md` |

### 3.5 §3 exit criteria

To exit §3 and enter §4 (and to **arm the §5 thaw evaluation**), **all** of the following must hold at T+7d:

1. First weekly compliance digest filed + TLA ratchet floor held.
2. Adversarial absorption doc filed + every finding classified.
3. DEBT register quarterly prep doc filed + no P0 row past Target.
4. Cutover retro sealed.
5. D+7 trigger window closure attestation 2-key signed.
6. Greenlight composite sustained == 1 across T+0..T+7d (no transient drops longer than 5 min).

---

## §4. T+7d to T+30d — Pilot-to-GA conversion + retrospective + post-mortem cadence

The 7d..30d window is the **stabilisation period** that closes the wave-27 absorption arc. The objective is to convert the pilot cohort to GA-paying tenants where eligible, run the 30-day retrospective, and stand up the steady-state post-mortem cadence.

### 4.1 Pilot-to-GA-tenant conversion

| # | Action | Owner | Verification |
|---|---|---|---|
| 4.1.1 | At T+25d ± 2d, hold the conversion call per `customer-success-playbook.md` §6 for each pilot tenant whose pilot-start anchor predates T-30d | CS owner | One-page "conversion brief" per pilot in pilot tracker |
| 4.1.2 | Apply the §6 10-criterion conversion checklist verbatim; pilots failing ≥ 3 criteria route to §7 termination or single 30-day extension | CS owner + VPProduct | Conversion brief shows pass/fail per criterion |
| 4.1.3 | Confirm **at least 1 conversion** occurs by T+30d (the §5 thaw condition requires ≥ 1 conversion) | Owner | Pilot tracker shows ≥ 1 row in `state: GA` |
| 4.1.4 | For each converted tenant, run `customer-success-playbook.md` §6 day-30 SLO baseline lock + GA conversion order-form filing | CS owner | Order form filed + per-tenant SLO baseline locked under `reports/post-ga/per-tenant/` |

### 4.2 30-day retrospective

| # | Action | Owner | Verification |
|---|---|---|---|
| 4.2.1 | At T+28d ± 1d, hold a 90-min cross-functional retro covering: cutover execution, first-30d incidents, customer feedback, DEBT burn-down, framework readiness | Owner | Retro doc sealed at `specs/_audits/<DATE>-post-ga-30d-retro.md` |
| 4.2.2 | Produce a "lessons applied" companion doc that maps retro findings into the next-quarter roadmap + DEBT-register updates | Owner | `specs/_audits/<DATE>-post-ga-30d-lessons-applied.md` |
| 4.2.3 | Pre-stage the first quarterly framework review agenda per `framework-v1-0-0-ga-audit.md` §11.5 (the 90-day window anchored on §3.3.3 anchor SHA) | Owner | Agenda doc at `specs/_audits/<DATE>-q1-framework-review-prep.md` |

### 4.3 Post-mortem cadence stand-up

| # | Action | Owner | Verification |
|---|---|---|---|
| 4.3.1 | Stand up the steady-state post-mortem cadence per `RB-POSTMORTEM-PROCESS.md`: every SEV-0/1 produces a blameless post-mortem within 5 BD of resolution | SRE Lead | Cadence published; first cycle covers any §1..§3 SEV events |
| 4.3.2 | Cross-link the post-mortem template to the `customer-success-playbook.md` §5 escalation tree (Tier 2 + on-call SRE rows) | CS owner + SRE Lead | Cross-ref added to playbook §5 |
| 4.3.3 | Activate the weekly compliance digest steady-state (Monday 09:00 UTC per `RB-COMPLIANCE-WEEKLY-REVIEW.md`) — confirm `.github/workflows/compliance-weekly.yml` is enabled | Compliance Lead | Workflow run history shows weekly green |

### 4.4 D+30 trigger window closure (cross-ref `RB-GA-CUTOVER.md` §6 + `RB-GA-LAUNCH-ROLLBACK.md` D+30)

| # | Action | Owner | Verification |
|---|---|---|---|
| 4.4.1 | At T+30d, confirm no `RB-GA-LAUNCH-ROLLBACK.md` D+30 trigger fired in the T+0..T+30d window | SRE Lead + Owner | Trigger sweep doc filed under `reports/post-ga/T-plus-30d-d30-sweep.md` |
| 4.4.2 | Append D+30 closure attestation to `specs/_audits/<DATE>-ga-cutover-execution-attestation.md` | SRE Lead + Owner | 2-key signature appended |

### 4.5 §4 exit criteria

To exit §4 (i.e., to **complete the 30-day post-GA continuity window**), **all** of the following must hold at T+30d:

1. At least 1 pilot tenant converted to GA-paying status (`state: GA`).
2. 30-day retro sealed + lessons-applied companion filed.
3. Post-mortem cadence stood up + weekly compliance digest steady-state confirmed.
4. D+30 trigger window closure attestation 2-key signed.
5. First quarterly framework review agenda pre-staged.
6. Q1 DEBT burn-down report shows no P0 row past Target.

---

## §5. Freeze-thaw conditions (cross-ref `2026-05-16-ga-1-feature-freeze.md §6`)

This section is the **executable form** of the freeze-thaw audit's §6 conditions. The freeze can only thaw when **all 5** of the following hold; the §6 audit's §6 enumerates 4 conditions — this runbook adds a fifth operational condition (≥ 1 conversion) consistent with §4.1.3 above.

| # | Thaw condition | Source | Verification artefact |
|---|---|---|---|
| 5.1 | **GA cutover executed.** `RB-GA-CUTOVER.md` §3.11 complete + `specs/_audits/<DATE>-ga-cutover-postmortem.md` SEALED. | `2026-05-16-ga-1-feature-freeze.md §6` item 1 | Sealed post-mortem doc + cutover-time attestation 2-key signed |
| 5.2 | **T+7d clean SLO window.** Greenlight composite == 1 sustained across T+0..T+7d (§3.5 item 6); zero SEV-0 across T+0..T+7d. | `2026-05-16-ga-1-feature-freeze.md §6` item 2 + this runbook §3.5 | `reports/post-ga/T-plus-7d-weekly-summary.md` digest |
| 5.3 | **Zero SEV-0/SEV-1 across T+0..T+7d.** PagerDuty incident query attributable to freeze surfaces (§2.1..§2.9 of the freeze audit) returns empty. | `2026-05-16-ga-1-feature-freeze.md §6` item 2 (expanded) | PagerDuty export attached to thaw declaration |
| 5.4 | **At least 1 pilot-to-GA conversion confirmed.** Per this runbook §4.1.3. | This runbook §4.1.3 | Pilot tracker row in `state: GA` |
| 5.5 | **Owner formal thaw declaration filed.** `specs/_audits/<DATE>-ga-1-feature-thaw.md` SEALED with: (a) anchor commit SHA cross-ref to freeze audit §1; (b) §5.1..§5.4 evidence pointers; (c) `scripts/check-ga-freeze-allowed.py` retirement decision (delete vs `--post-thaw` mode). | `2026-05-16-ga-1-feature-freeze.md §6` item 3 + item 4 | Thaw declaration doc + cosmetic CONTRIBUTING.md §0 removal commit |

### 5.1 Thaw declaration sequence

Once §5.1..§5.4 evaluate to TRUE (typically at T+7d but no earlier than the §3.5 exit), the Owner executes the thaw declaration:

1. Author `specs/_audits/<DATE>-ga-1-feature-thaw.md` per `2026-05-16-ga-1-feature-freeze.md §6` item 3.
2. Decide `scripts/check-ga-freeze-allowed.py` disposition (delete OR `--post-thaw` mode); record decision in thaw doc.
3. Open a **cosmetic-doc** PR under freeze §3.c that removes the `## §0 GA-1 FEATURE FREEZE IS ACTIVE` blockquote from `CONTRIBUTING.md` (single-reviewer CODEOWNERS sufficient; commit subject `docs: ` + trailer `FREEZE-EXCEPTION: cosmetic-doc`).
4. Append a final row to `specs/_audits/sealed/2026-05-16-ga-1-feature-freeze.md §5` freeze monitor referencing the thaw doc SHA.
5. Update `reports/ga-freeze-monitor.json` `freeze_status: "THAWED"` with the thaw declaration SHA.

### 5.2 Pre-thaw posture (T+7d to thaw declaration)

If §5.1..§5.4 are satisfied but the Owner has not yet filed §5.5, the **freeze remains ACTIVE** per the freeze audit's §6 (all four conditions required). New work continues to queue in `TODO.md` with the `post-thaw` tag.

### 5.3 Partial thaw not permitted

A partial thaw (e.g., "API surface only") is explicitly disallowed per `2026-05-16-ga-1-feature-freeze.md §6`. If the Owner needs a §2.1..§2.9 surface change before §5.5 fires, the `§3.b P1-ga-blocker` exception class applies (and the change goes through the 2-key authorisation path).

---

## §6. First-quarter framework v1.0.1 review prep

Per `specs/_audits/sealed/2026-05-15-framework-v1-0-0-ga-audit.md §11.5` (wave-22 90-day rolling quarterly cadence anchored on FROZEN cut), the first quarterly review fires at **anchor SHA + 90d**. This runbook prepares the slot.

### 6.1 Anchor SHA capture

| # | Action | Owner | Verification |
|---|---|---|---|
| 6.1.1 | At §3.3.3, record the **GA cutover §3.11 commit SHA** as the quarterly cycle anchor in `reports/post-ga/quarterly-anchor.txt` | Owner | File contains: `anchor_sha=<sha7>` + `anchor_date=<YYYY-MM-DD>` + `q1_due_date=<anchor + 90d>` |
| 6.1.2 | Cross-publish the q1_due_date into `specs/00_framework.md` §41 review-cadence table (currently `(a definir)`) | Owner | Framework doc updated; commit transits `§3.c cosmetic-doc` freeze exception |
| 6.1.3 | Schedule the q1 review meeting in calendar at `q1_due_date - 14d` (lead time for reviewer prep per addendum §3 SLA STANDARD = 3 BD; HIGH_RISK = 7 BD) | Owner | Calendar invite filed |

### 6.2 v1.0.1 candidate-change inventory

| # | Action | Owner | Verification |
|---|---|---|---|
| 6.2.1 | At T+28d (during §4.2 30-day retro), inventory candidate framework changes routed to v1.0.1 (typo fixes per `framework-v1-0-0-ga-audit.md §10` v1.0.1+ patch row) | Owner | List filed in `specs/_audits/<DATE>-q1-framework-review-prep.md` §1 |
| 6.2.2 | Classify each candidate as: (a) v1.0.1 patch (typo / clarification without semantic change — no new ADR), (b) v1.1.0 minor (additive change — new ADR + reviewer sign-off), (c) v2.0.0 major (semantic break — full re-review) | Owner | Classification table in prep doc §2 |
| 6.2.3 | Confirm at least one CODEOWNERS-approved patch is queued so the v1.0.1 cut has something to ship (avoid a vacuous quarterly cycle) | Owner | At least one candidate row in §2.a |

### 6.3 Reviewer onboarding precheck

| # | Action | Owner | Verification |
|---|---|---|---|
| 6.3.1 | Per `framework-v1-0-0-ga-audit.md §11.6` addendum §4 (Reviewer Training Pack 10-hour floor), confirm reviewer pool status (either Option A 4-distinct or Option C dual-hat per ADR-0034b) | Owner | Pool status row updated in `specs/00_framework.md §43.1` |
| 6.3.2 | If reviewer onboarding is still pending, q1 review fires under ADR-0034b dual-hat fallback (Pairing-Alpha or Pairing-Beta per Owner's wave-26 decision) | Owner | `§42` change-log row in `specs/00_framework.md` records pairing |

### 6.4 §6 exit criteria

To **arm** the q1 quarterly review (i.e., to be ready to fire at anchor SHA + 90d), **all** of the following must hold by T+30d:

1. Anchor SHA recorded + q1_due_date published.
2. Candidate-change inventory filed + classified.
3. Reviewer pool status row in `specs/00_framework.md §43.1` is `NAMED` or `DUAL-HAT-PAIRING-<A|B>`.
4. Q1 review meeting on the calendar at `q1_due_date - 14d`.

---

## §7. Comms templates (post-GA)

All templates live alongside `RB-GA-CUTOVER.md §7` under `marketing/launch/COMMS/` and `docs/internal/pilot-comms-templates.md`. This runbook **reuses** them; no new templates required for §1..§4 because the cutover runbook already pre-staged the §7.3 post-cutover set.

| Template | Audience | Trigger | Owner |
|---|---|---|---|
| `CUTOVER-T-PLUS-1-PILOT.md` | 5 pilot tenants | §1.2.1 (T+1h..T+12h) | VPProduct |
| Onboarding ack survey (lightweight, 1-question) | 5 pilot tenants | §2.2.3 (T+72h) | CS owner |
| Weekly summary (auto-generated) | 5 pilot tenants | §3.1.4 (T+7d Monday) | SRE Lead |
| Conversion call template (per `customer-success-playbook.md §6`) | Per-tenant call | §4.1.1 (T+25d ± 2d) | CS owner |
| Thaw announcement (internal Slack `#all-hands`) | All-hands | §5.1 (post Owner thaw decl) | Owner |

---

## §8. Sign-off + attestation

This runbook does **not** require its own 2-key sign-off to begin (it runs as the natural continuation of `RB-GA-CUTOVER.md` §3.11 completion). However, the per-window **exit attestations** (§1.5, §2.4, §3.5, §4.5) require the same 2-key pairings as the cutover runbook §8:

| Exit gate | Key 1 | Key 2 |
|---|---|---|
| §1.5 (T+24h) | Owner | On-call SRE Lead |
| §2.4 (T+72h) | Owner | On-call SRE Lead |
| §3.5 (T+7d) | Owner | On-call SRE Lead |
| §4.5 (T+30d) | Owner | On-call SRE Lead |
| §5.5 (Thaw decl) | Owner | Owner (solo per `ADR-0034b` dual-hat fallback if reviewer pool not yet named; otherwise Reviewer per pairing) |

All attestations append to `specs/_audits/<DATE>-ga-cutover-execution-attestation.md` (the cutover attestation doc per `RB-GA-CUTOVER.md §8.1`); the post-GA continuity window does NOT spawn a separate attestation chain — it extends the cutover one.

---

## §9. Verification + drill cadence

- **Dry-run** of this playbook executes via `scripts/post-ga-monitor.sh --dry-run` (in-process, no production endpoints touched). Output validates: §1.1 sample-point structure, §2.1 baseline-envelope shape, §3.1 weekly-digest schema, §5 thaw-condition truth table.
- **First real execution** at T+0 (immediately after `RB-GA-CUTOVER.md §3.11` complete) — no separate drill needed; the runbook IS the drill.
- **Self-cadence after T+30d:** weekly compliance digest (per `RB-COMPLIANCE-WEEKLY-REVIEW.md`) absorbs the daily post-GA monitor output; this runbook's daily cadence retires at T+30d.

---

## §10. References

- `specs/_runbooks/RB-GA-CUTOVER.md` §6 — post-cutover stub; this runbook is the canonical expansion.
- `specs/_runbooks/RB-GA-LAUNCH-ROLLBACK.md` — sister *reverse* runbook; D+1 / D+7 / D+30 triggers consumed by §1.4 + §2.3 + §3.4 + §4.4.
- `specs/_runbooks/RB-COMPLIANCE-WEEKLY-REVIEW.md` — Monday compliance digest consumed in §3.1 + §4.3.3.
- `specs/_runbooks/RB-AUDIT-EXPORT-VERIFY-FAILED.md` — audit-chain spot-verifier consumed in §2.2.2.
- `specs/_runbooks/RB-POSTMORTEM-PROCESS.md` — post-mortem template stood up in §4.3.
- `specs/_runbooks/ONCALL-ESCALATION-MATRIX.md` — paging tiers used by §1.4 escalation triggers.
- `specs/_audits/sealed/2026-05-16-ga-1-feature-freeze.md` §6 — freeze-thaw conditions operationalised by §5.
- `specs/_audits/sealed/2026-05-15-framework-v1-0-0-ga-audit.md` §11.5 + §11.8 — quarterly cadence + reviewer pool status consumed in §6.
- `specs/_audits/sealed/2026-05-14-soc2-readiness-score.md` — D+30 readiness milestone consumed in §4.4.
- `docs/internal/customer-success-playbook.md` §4 + §5 + §6 + §7 — daily CS metrics + escalation tree + conversion criteria + termination protocol consumed across §1..§4.
- `docs/internal/pilot-comms-templates.md` — templates referenced in §7.
- `docs/internal/pilot-dashboard-checklist.md` — 10-metric pilot dashboard consumed in §1 + §2.
- `dashboards/alerts/dash-ga-greenlight.yml` — composite greenlight recording rules sampled in §1.1.
- `dashboards/alerts/dash-slo-multi-burn.yml` — burn-rate alerts referenced in §1.1.3 + §2.1.
- `scripts/post-ga-monitor.sh` — daily monitor companion that emits the §§1–4 digests under `reports/post-ga/`.
- `scripts/check-ga-freeze-allowed.py` — freeze gate retired or made permissive at §5.5.
- `CONTRIBUTING.md §0` — freeze notice removed at §5.1 step 3 (cosmetic-doc PR).
- `ROADMAP-TO-GA.md` §8 — Wave R-GA + post-GA context.
- `ADR-0034b-framework-reviewer-dual-hat-fallback.md` — dual-hat path consumed in §6.3.2 + §8.

---

## §11. Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-05-16 | Gustavo (via Claude Opus wave-27 builder, worktree `wt/r-prep-post-ga-continuity-playbook`) | Initial canonical 30-day post-GA operational continuity playbook — §1 T+0..T+24h greenlight composite re-verification (4 sample points T+0 / T+6h / T+12h / T+24h) + initial pilot ack ≥ 3 of 5 + status page transition OPERATIONAL sticky-7d + escalation triggers wiring `RB-GA-LAUNCH-ROLLBACK.md` D+1; §2 T+24h..T+72h SLO baseline capture vs staging benchmarks (±20% green / +50% RED) + first-GA-CAS-write + audit-chain spot-verifier + D+1 trigger closure attestation; §3 T+72h..T+7d weekly compliance digest + adversarial-review absorption (route to DEBT / §3.b / post-thaw) + DEBT register quarterly cycle prep + cutover retro sealed + D+7 trigger closure; §4 T+7d..T+30d pilot-to-GA conversion (≥ 1 conversion required for thaw) + 30-day retro + post-mortem cadence stand-up + D+30 trigger closure; §5 **5 freeze-thaw conditions** operationalising `2026-05-16-ga-1-feature-freeze.md §6` (cutover-executed + T+7d-clean-SLO + zero-SEV-0/1 + ≥1-conversion + Owner-thaw-declaration) with thaw declaration sequence wiring CONTRIBUTING.md §0 removal via §3.c cosmetic-doc PR; §6 first-quarter framework v1.0.1 review prep per wave-22 §5 90-day rolling quarterly cadence anchored on FROZEN cut + reviewer pool status precheck; §7 comms templates (reuse of `RB-GA-CUTOVER.md §7` set + 1-question onboarding ack survey + conversion call template); §8 per-window 2-key exit attestations appended to cutover attestation chain; §9 dry-run via `scripts/post-ga-monitor.sh --dry-run`; §10 21-row reference graph; §11 change log. Companion script `scripts/post-ga-monitor.sh` emits markdown digests under `reports/post-ga/`. Regulatory: post-cutover monitoring is normal operating mode; GDPR Art. 33 / LGPD ANPD 72h clock starts only on SEV-0/1 with confirmed personal-data impact, at which point this runbook steps aside in favour of `RB-DSR-*` + `RB-GA-LAUNCH-ROLLBACK.md`. |

---

**Status:** ACTIVE. Runs from `RB-GA-CUTOVER.md §3.11` completion through T+30d. After T+30d, ownership transitions to standard ops cadence (weekly compliance digest + monthly business review).
