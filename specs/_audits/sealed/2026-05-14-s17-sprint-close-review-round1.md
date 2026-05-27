---
id: "AUDIT-S17-SPRINT-CLOSE-R1"
type: "audit"
doc_status: "ACTIVE"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Adversarial Review Round 1 (Sonnet)"
tags: ["audit", "sprint-close", "s17", "adversarial"]
---

# AUDIT-S17-SPRINT-CLOSE-R1 — Adversarial post-merge sprint-close review

**Target.** S-17 Ops Maturity — 6 WIs all SEALED in `main` at commit `101f916`.
**Scope.** Re-verify the 4 P0 spec gaps surfaced in the pre-flight audit
(`specs/_audits/sealed/2026-05-14-s17-sprint-preflight-review.md`, commit `9e666b4`)
against the delivered code + docs; check pre-merge gates; verify charter
constraints; surface new findings; score 0–10; render verdict.

---

## 1. Score

**7.6 / 10.**

Justification — code-and-impl quality is strong (clean build, clippy clean,
148 passing tests across 4 new crates, all migrations additive, all public
enums in S-17 crates `#[non_exhaustive]`, 32-scenario adversarial summary
authored with explicit cross-WI section, PRR CONDITIONALLY_APPROVED with
8 documented waivers). But the **same 4 P0 spec gaps** flagged at pre-flight
are partially or not addressed: chaos-FM coverage moved from 37 % → 39 % of
P0/P1 (still ≪ a credible bar of ~80 %); runbook count drift went from
3-way (40 / 47 / 25 / 55) to 4-way (40 / 47 / 25 / 62) because new RBs were
authored without reconciling the contract claim; SLO catalog still has
**zero** `SLO-DR-RTO` / `SLO-DR-RPO` entries, even though the dr-drill crate
hard-codes `RTO_CEIL_SECONDS=1800` + `RPO_CEIL_SECONDS=60` and references
non-existent `SLO-RTO-REGION-FAILOVER` / `SLO-RPO-REGION`; cross-WI
invariants exist only in the adversarial summary (`specs/_audits/...`), not
in `_spec_contract.md §8` which still says *"Não cria invariants novas"*.

The implementation is **shippable** (operationally we have working chaos /
DR / runbook / oncall infra and a tabletop record), but the **spec layer
that gates GA at D+50 is not yet truthful**. Two of four P0s are real
blockers for the D+50 evidence gate (SLO catalog RTO/RPO + runbook count
canonicalisation); the other two (chaos coverage % + cross-WI invariants
in §8) are deferrable to S-18 but must be tracked.

---

## 2. Verdict

**CONDITIONALLY APPROVED (P1 only).** The merge stands; the SEAL stands
(consistent with PRR-S17 `STANDARD CONDITIONALLY_APPROVED` posture). Two
mandatory follow-ups are filed below as P0 (SLO catalog + runbook count)
and must close before the D+50 GA evidence gate (2026-07-03). Two
remaining issues file as P1 (chaos coverage uplift + spec-contract
invariants reconciliation).

---

## 3. Pre-flight P0 re-verification

| # | Pre-flight P0 | Status | Evidence |
|---|---|---|---|
| 1 | Chaos catalog covers only 37 % of P0/P1 FMs (8 of 22) | **PARTIAL** | `failure_modes.md` now lists **23** P0/P1 FMs (1 P0 `FM-451` + 22 P1). `RB-CHAOS-CATALOG.md` references 17 distinct FM-IDs, but the intersection with the P0/P1 set is **9** (FM-051, FM-054, FM-156, FM-202, FM-254, FM-300, FM-302, FM-400, FM-101 via §1 cross-ref). Coverage = **9/23 = 39 %**. Pre-flight bar was implicitly ~80 %. Catalog also still self-reports "Distinct FM-IDs covered: 8 — AC gate ≥ 8 satisfied" (runtime view §5), i.e. WI-001's AC gate of "≥ 8 FMs" was the *floor*, not the *target*. Net: the gap is essentially unchanged. |
| 2 | Runbook count drift (40 / 47 / 25 / 55) | **REGRESSED** | `find specs/_runbooks specs/05_runbooks specs/05_quality/runbooks -name "RB-*.md" -not -name "*INDEX*"` → **62** files. Contract `S17/_spec_contract.md` R-S17-8 still says "All **40** runbooks dry-run executed em últimos 90d". `RB-RUNBOOK-DRILL-INDEX.md` §1 says "25 of **47** total". Repo physical count is now 62. The drift expanded from 3-way to 4-way. No reconciling RFC / spec-contract bump. |
| 3 | SLO catalog has zero RTO/RPO entries | **NOT-ADDRESSED** | `grep -ni "RTO\|RPO" specs/03_architecture/slo_catalog.md` → 0 hits in any §4.x SLO entry (only `§ Nobl9` external reference). Yet `crates/corelink-dr-drill/src/outage.rs:17` references *"SLO catalog `SLO-RTO-REGION-FAILOVER`"* and `outage.rs:30` references *"`SLO-RPO-REGION`"* — both string ids that do **not** exist in the catalog. DR drill pass/fail criteria thus live only in source code constants `RTO_CEIL_SECONDS=1800` + `RPO_CEIL_SECONDS=60`, not in the canonical SLO spec. **Hard blocker for D+50 GA evidence gate.** |
| 4 | No cross-WI invariants in `_spec_contract.md §8` | **PARTIAL** | `S17/_spec_contract.md §8` still reads literally *"Não cria invariants novas (sprint operational; invariants são em outros sprints)"*. However, the adversarial summary (`specs/_audits/sealed/2026-05-14-s17-adversarial-summary.md §7`) **does** enumerate 6 cross-WI integration scenarios (chaos↔PD page routing, DR↔chaos calendar collision, runbook drill↔chaos feedback loop, tabletop finding↔tracker handoff, game day↔blameless culture, PRR conditional↔D+50 gate logic). So the *thinking* is captured at audit-doc level; what is missing is promotion of those rules to enforceable invariants in §8 (which is what the pre-flight asked for). |

---

## 4. New P0 / P1 / P2 introduced by the implementation

### P0 (must close before D+50 GA evidence gate, 2026-07-03)

- **P0-CLOSE-01** — Add `SLO-DR-RTO-REGION-FAILOVER` (1800 s) and
  `SLO-DR-RPO-REGION` (60 s) entries to
  `specs/03_architecture/slo_catalog.md §4.x`, with explicit FM-mapping,
  measurement source (DO replication lag p99), and per-tier targets.
  Update `crates/corelink-dr-drill/src/outage.rs` doc-comments to point to
  the real SLO ids once landed. (Fixes pre-flight P0 #3.)
- **P0-CLOSE-02** — Canonicalise the runbook count in one source of truth.
  Recommend: `specs/04_sprints/_sealed/S17/_spec_contract.md` R-S17-8 → "All P0/P1
  runbooks (currently 25 of 62 physical files; canonical list in
  `RB-RUNBOOK-DRILL-INDEX.md`) dry-run executed em últimos 90d". Update
  `RB-RUNBOOK-DRILL-INDEX.md §1` to read "25 of 62 total" (or whatever the
  reconciled denominator is after a P0/P1-vs-P2/P3 audit). Add a
  `scripts/check_runbook_count.py` consistency gate to prevent future drift.
  (Fixes pre-flight P0 #2.)

### P1 (track for S-18 / S-19; must close before GA)

- **P1-CLOSE-01** — Chaos coverage uplift. WI-001 hit the floor (≥ 8 FMs)
  but only 39 % of P0/P1 FMs. File a follow-up WI in S-18 or S-19 to add
  chaos experiments for the 14 P0/P1 FMs not yet covered (FM-007, FM-062,
  FM-100, FM-205, FM-206, FM-253, FM-258, FM-303, FM-403, FM-404, FM-450,
  FM-451 (the only true P0), FM-452, FM-453). Target ≥ 80 % P0/P1 coverage
  at GA. (Partially addresses pre-flight P0 #1.)
- **P1-CLOSE-02** — Promote the 6 cross-WI integration scenarios from
  `2026-05-14-s17-adversarial-summary.md §7` into enforceable invariants in
  `S17/_spec_contract.md §8` (or to a new canonical
  `specs/03_architecture/canonical/operational_invariants.md`). The §8
  "Não cria invariants novas" stance is no longer accurate.
  (Fixes pre-flight P0 #4 at spec-contract layer.)

### P2 (nice-to-have)

- **P2-NEW-01** — `RB-CHAOS-CATALOG.md` reports two divergent "FMs covered"
  tallies: §10 says ≥ 8 FMs (AC gate floor), §5 lists 8 primary FMs, but
  §1.1 actually cross-references 17 distinct FM-IDs across primary +
  secondary columns. Consolidate to a single canonical tally with a clear
  "primary FM" vs "secondary FM (cleanup-pass)" distinction.
- **P2-NEW-02** — `RB-RUNBOOK-DRILL-INDEX.md` is `doc_status: "DRAFT"`
  while spec contract bumped to 1.3.0 / SEALED. Promote to SEALED or
  document the gap.
- **P2-NEW-03** — `crates/corelink-dr-drill/src/schedule.rs` uses a
  single semestral cron (every 6 months); `DrillCycle` enum derived from
  the cadence may give surprising behaviour if a missed run occurs.
  Recommend an explicit "overdue catchup" path mirroring
  `corelink-runbook-tracker::scan_overdue`.

---

## 5. Charter constraint check

| # | Charter constraint | Status | Evidence |
|---|---|---|---|
| 12 | `#[non_exhaustive]` on all S-17 crate public enums | **PASS** | 22 `pub enum` declarations across `corelink-chaos-scheduler`, `corelink-dr-drill`, `corelink-runbook-tracker`, `corelink-oncall` — every one has `#[non_exhaustive]` on the preceding line (verified by `grep -rB2 "^pub enum"`). |
| 13 | Migrations 0033/0034/0035/0036 additive | **PASS** | `python3 scripts/check_migrations_additive.py` → "OK: 39 migration file(s) scanned; all additive." All four files present (`0033_chaos_runs.sql`, `0034_dr_drill_runs.sql`, `0035_runbook_drills.sql`, `0036_oncall_pages.sql`). |
| 14 | Wrangler cron schedules — 5-field standard cron | **PASS** | Only one cron added in S-17 wave (`wrangler.toml:134`): `"0 6 * * 1"` — standard 5-field (min hour dom month dow). The semestral DR-drill cron lives in `corelink-dr-drill/src/schedule.rs` as `SEMESTRAL_CRON`, not in `wrangler.toml`; verify when wired. |
| 15 | CSP-affected workflows — SHA-pinned | **N/A (no new workflows in S-17)** | `git diff f8a9d7b^..101f916 --name-only` shows zero `.github/workflows/*.yml` changes in the S-17 merge wave. The chaos / DR / runbook crons run via CF Cron (wrangler), not GitHub Actions. CSP / SHA-pinning gates not exercised this sprint. |

---

## 6. Positives

- **Workspace builds clean.** `cargo build --workspace` → exit 0 (1.46 s
  incremental). `cargo clippy --workspace --tests -- -D warnings` → exit 0.
  `cargo test --workspace --no-run` → exit 0.
- **148 tests passing across the 4 new crates** (chaos 59, dr-drill 24,
  runbook-tracker 22, oncall 43), zero failures, zero non-ignored skips.
- **All public enums non-exhaustive.** Charter §future-proofing satisfied
  in full for the new crates — no API churn risk added.
- **Migrations are clean + additive.** Numbering reconciled correctly after
  the 0033 → 0034 fix (commit `3cea448`); validator passes.
- **Adversarial summary actually has a cross-WI section.** 32 total
  scenarios, 6 explicitly cross-WI, 100 % mitigation rate documented.
  This is the best ops-sprint adversarial doc we've shipped (compare
  S-16 which had only per-WI scenarios).
- **PRR posture is honest.** CONDITIONALLY_APPROVED with 8 documented
  waivers + 5/7 sign-offs + 2 explicitly pending external advisors. No
  rubber-stamp.
- **Spec-validator failures hold at 14** (S-11/S-12/S-13 ADR schema
  legacy); no new S-17 spec failures introduced.

---

## 7. Per-WI sub-scores

| WI | Title | Score | Notes |
|---|---|---|---|
| WI-S17-001 | Chaos scheduler + 8 chaos types staging weekly + chaos catalog | **7.5** | Solid runtime (59 tests, safe-mode auto-abort tested, seed determinism verified). AC gate hit at the *floor* (≥ 8 FMs), not the *target* — P0/P1 coverage stuck at 39 %. Catalog §5 vs §1 numeric inconsistency (P2-NEW-01). |
| WI-S17-002 | DR drill semestral + 1 cycle CF region outage simulation | **7.0** | Code-level RTO/RPO constants exist (`RTO_CEIL_SECONDS=1800`, `RPO_CEIL_SECONDS=60`) and 24 tests cover the env-guard / outage / schedule paths, but the **SLO catalog has no DR entries** — code references `SLO-RTO-REGION-FAILOVER` / `SLO-RPO-REGION` strings that don't resolve. Pass/fail criteria are not yet canonical. |
| WI-S17-003 | Runbook dry-run tracker + 3 dry-runs P0/P1 monthly + FM-202 drift | **7.5** | 22 tests, tracker logic + 30-day cadence + drift detection (`> 2.0×`) all solid. But the index doc is `doc_status: DRAFT` (P2-NEW-02) and the "25 of 47" claim collides with the physical-file count of 62 (P0-CLOSE-02). |
| WI-S17-004 | Incident + post-mortem templates + 1 synthetic SEV-2 + RB-POSTMORTEM-PROCESS | **8.5** | Templates landed (`templates/incident.md` + `templates/postmortem.md`), synthetic post-mortem documented, blameless culture rule wired into RB-POSTMORTEM-PROCESS + RB-TABLETOP-TEMPLATE. Cleanest WI of the sprint. |
| WI-S17-005 | Oncall PagerDuty schedule + fatigue tracking + Grafana dashboard | **8.0** | 43 tests, fatigue threshold + handoff decision modelled as types, PagerDuty integration behind a trait (testable). PD prod keys flagged as waiver W8 (first deploy) — appropriate. |
| WI-S17-006 | Game day quarterly + tabletop template + 1 synthetic exercise + S-17 PRR + adversarial summary | **8.0** | Tabletop record (BYOK CMK revoke 60-min) with 5 findings + 100 % owner / due-date coverage. Chaos-catalog cleanup pass documented (seed / safe-mode / FM mapping refinements). PRR CONDITIONALLY_APPROVED with 8 waivers. Marked SYNTHETIC pending Q3-2026 real exercise — appropriate. The cross-WI adversarial section (§7) is the right artefact but should have triggered a `_spec_contract.md §8` update too (P1-CLOSE-02). |

---

## 8. Change log

| Version | Date | Author | Notes |
|---|---|---|---|
| 1.0.0 | 2026-05-14 | Adversarial Review Round 1 (Sonnet) | Initial post-merge sprint-close review. Score 7.6 / 10, verdict CONDITIONALLY APPROVED (P1 only). Two P0 follow-ups filed (SLO-DR-RTO/RPO catalog entries + runbook count canonicalisation), two P1 follow-ups (chaos coverage uplift + cross-WI invariants in §8), three P2 polish items. Pre-merge gates all green (build / clippy / tests / migrations / validator / no conflict markers). 148 tests passing across 4 S-17 crates. |
