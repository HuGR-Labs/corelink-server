# GA Final Checklist — Operator-Runnable Boolean Sign-off — 2026-05-16

> **Doc kind:** operator-runnable boolean checklist (no canonical front matter required — `_audits/` excluded from `validate_specs.py` per `scripts/validate_specs.py::SKIP_ALL`).
>
> **Companion:** `specs/_audits/2026-05-16-ga-readiness-final.md` (the sign-off-ready board this checklist underpins).
>
> **Usage.** Owner + on-call SRE walk through this checklist in the GA-Gate go/no-go meeting (per `specs/_compliance/GA-GATE-GO-NOGO-TEMPLATE.md`). Each row resolves to one of:
> - `[x] true` — gate green; no action.
> - `[ ] false` — gate red; **must** resolve before §13 signature.
> - `[!] defer: <link to §11 row OR documented waiver>` — explicit, traced DEFER.
>
> **Rule.** Signature §13.2 + §13.3 require **zero `false` rows**. Every `defer:` must point to either `specs/_audits/2026-05-16-ga-readiness-final.md §11` (the 7 external DEFER items, scrubbed wave-25) OR a documented waiver in `specs/_compliance/GA-GATE-GO-NOGO-TEMPLATE.md §3`.

---

## A. Engineering track (mirrors `GA-GATE-CRITERIA.md` §1 — owner: CTO)

- [ ] A-01 — All 21 sprints (S-00 → S-20) carry `work_status: SEALED`.
- [ ] A-02 — `python3 scripts/validate_specs.py` exit 0.
- [ ] A-03 — `python3 scripts/validate_references.py` exit 0.
- [ ] A-04 — `python3 scripts/validate_canonical_consistency.py` exit 0 (197 INVs declared / 0 orphan refs / CRITICAL-without-TLA+ ≤ 1 documented exempt).
- [ ] A-05 — `python3 scripts/validate_inv_promotion.py` exit 0 (143/143 WI-coverage).
- [ ] A-06 — Production wiring (R-prep + waves 18-24) complete: Stripe wasm32, BYOK 4-provider, CF binding, Neon driver, replica coordinator, DSR worker, customer dashboard, statuspage-init, breach notification templates.
- [ ] A-07 — Wave-23 stream #1 (wave-22 adversarial review codex Opus pass) SEALED with no P0 and no P1.
- [ ] A-08 — Wave-23 streams #2 (DEBT-008) + #3 (DEBT-015-BUILD) + #4 (chaos combined) + #5 (pilot E2E) + #6 (LFPDPPP MX package) + #7 (CS playbook) + #8 (beta-feedback triage) + #9 (P2 cleanup + INV-DRAFT promotion) all SEALED or explicitly DEFERRED.
- [ ] A-09 — Wave-24 stream #1 (GA cutover dry-run rehearsal) SEALED with no fail-CLOSED scenario surfacing P0.
- [ ] A-10 — DEBT-008 empirical kill-rate ≥ 75 % for `{audit-chain, hash, dedup, tenant-path, handler-cas, auth-schema}` + wave-23 5-crate batch (or escalation documented per row).
- [ ] A-11 — Mutation CI-nightly cron `23 5 * * *` operating for ≥ 7 consecutive days with `{pat, clerk, dual-approval, ratelimit}` at ≥ 75 % floor.
- [ ] A-12 — Wave-22 chaos campaign 8 fail-CLOSED scenarios green; wave-23 stream #4 combined-failure matrix green.
- [ ] A-13 — Wave-22 24h endurance harness operable; wave-24 stream #6 24h soak run executed with SLO streak captured.
- [ ] A-14 — Perf regression CI tightened wave-22 stream #6; no perf-regress alerts in the 14 days prior to cutover.
- [ ] A-15 — DEBT-015-BUILD CLOSED (stream #3 SEAL) OR escalated to P1 with alternative-architecture decision and documented waiver.

## B. Security track (mirrors `GA-GATE-CRITERIA.md` §2 — owner: VPSec / Security Lead)

- [ ] B-01 — BYOK 4-provider matrix complete (AWS / GCP / Azure / Vault) — DEBT-003 closed (AWS Artifact PDF + sha256) OR `defer: §11#4`.
- [ ] B-02 — RLS WITH CHECK enforced on every tenant-bound table (Neon shadow + D1).
- [ ] B-03 — Audit fail-CLOSED enforced: `INV-AUDIT-EMIT-ATOMIC-WITH-HANDLER` family proptest streak ≥ 30 d green.
- [ ] B-04 — WallClock cross-route closure landed (wave-21 `f3462c6`; wave-23 cleanup P2-01 absorbed).
- [ ] B-05 — Chaos combined-failure matrix (wave-23 stream #4) green — executor-loss × replication-lag × tenant-isolation surface.
- [ ] B-06 — Pentest scope SEALED (`specs/_audits/2026-05-16-pre-ga-pentest-scope.md` v1.0).
- [ ] B-07 — Pentest engagement kicked off (vendor + SOW signed) OR `defer: §11#3`.
- [ ] B-08 — Secrets matrix validator `code_only=0` (DEBT-001 closed; matrix=109, code=89, in_both=89).
- [ ] B-09 — Static-analysis baseline green (CodeQL + Semgrep + actionlint + action SHA pinning).
- [ ] B-10 — Dependabot policy operational + `RB-DEPENDABOT-INCIDENT.md` runbook in place.
- [ ] B-11 — Tenant isolation invariant family (TENANT domain) proptest streak ≥ 30 d green.
- [ ] B-12 — CAS integrity invariant family (CAS domain) proptest + TLA+ proof green.

## C. Operations track (mirrors `GA-GATE-CRITERIA.md` §3 — owner: SRE Lead)

- [ ] C-01 — `RB-GA-CUTOVER.md` §0 T-7d pre-cutover checklist run with all rows green.
- [ ] C-02 — Greenlight composite `slo:greenlight:composite_ok == 1` sustained ≥ 30 min in staging.
- [ ] C-03 — G1 — p99 latency ≤ SLO across 5 regions (sustained 30 min).
- [ ] C-04 — G2 — audit-chain integrity verifier green (sustained 24 h prior).
- [ ] C-05 — G3 — zero SEV-0/SEV-1 in 72 h prior.
- [ ] C-06 — G4 — customer success ack ≥ 5 pilot tenants (per-tenant attestation form signed) OR `defer: §11#6`.
- [ ] C-07 — G5 — Neon shadow lag p99 ≤ 5 min (sustained 30 min).
- [ ] C-08 — G6 — DSR cron 24 h success rate 100 %.
- [ ] C-09 — On-call rota confirmed for cutover window + T+7d (per `RB-ONCALL-POLICY.md`).
- [ ] C-10 — 50+ RB-* runbooks reviewed in last 30 d (compliance weekly review cadence).
- [ ] C-11 — Statuspage `status.corelink.dev` provisioned (DEBT-016 closed) OR `defer: §11#5`.
- [ ] C-12 — Backup-verification cron green for ≥ 7 consecutive days (`RB-BACKUP-VERIFICATION.md`).
- [ ] C-13 — Cold-restore drill executed in last 90 d (`COLD-RESTORE-DRILL-SPEC.md`).
- [ ] C-14 — Active-failover drill executed in last 90 d (`ACTIVE-FAILOVER-DRILL-SPEC.md`).
- [ ] C-15 — IR tabletop drill executed in last 90 d (`IR-TABLETOP-PLAYBOOK.md`).
- [ ] C-16 — Terraform drift validator green at T-72h (`RB-TERRAFORM-DRIFT.md`).

## D. Customer track (mirrors `GA-GATE-CRITERIA.md` §4 — owner: VPProduct / Product Lead)

- [ ] D-01 — CLI `verify-ndjson` HTTP path operable (S-19 SEAL).
- [ ] D-02 — SDK examples published (S-19 SEAL).
- [ ] D-03 — Pilot onboarding E2E dry-run executed end-to-end (wave-23 stream #5 SEAL).
- [ ] D-04 — Customer success playbook published (wave-23 stream #7 SEAL).
- [ ] D-05 — Beta feedback triage pipeline operable (wave-23 stream #8 SEAL).
- [ ] D-06 — Customer breach notification templates ready (`specs/_audits/2026-05-15-customer-breach-notification-templates.md`).
- [ ] D-07 — Customer dashboard accessible (`specs/_audits/2026-05-15-customer-dashboard-spec.md`).
- [ ] D-08 — Stripe customer portal operable (`specs/_audits/2026-05-15-stripe-customer-portal-spec.md`).
- [ ] D-09 — Pilot signups ≥ 3 design-partners (DPA signed) OR `defer: §11#6`.

## E. Compliance / Legal track (mirrors `GA-GATE-CRITERIA.md` §5 — owner: Legal Counsel + DPO)

- [ ] E-01 — SOC 2 evidence rollup current (`SOC2-EVIDENCE-ROLLUP-2026-05-15.md` reissued in last 30 d).
- [ ] E-02 — ISO 27001 SoA current (`ISO27001-STATEMENT-OF-APPLICABILITY-2026-05-15.md` reissued in last 30 d).
- [ ] E-03 — GDPR full-audit current (`GDPR-FULL-AUDIT-2026-05-15.md` reissued in last 30 d) + SCC executed (`GDPR-SCC-EXECUTION-2026-05-15.md`).
- [ ] E-04 — LGPD full-audit current (`LGPD-FULL-AUDIT-2026-05-15.md`) + residency attestation (`LGPD-RESIDENCY-ATTESTATION-2026-05-15.md`) + ROPA (`LGPD-ROPA-2026-05-15.md`) + DPO appointed (`DPO-APPOINTMENT-2026-05-15.md`).
- [ ] E-05 — LFPDPPP MX attorney sign-off complete (DEBT-025 closed) OR `defer: §11#1`.
- [ ] E-06 — PCI DSS SAQ-A self-assessment current (`PCI-DSS-SAQ-A-2026-05-15.md`).
- [ ] E-07 — CCPA inheritance from GDPR/LGPD pipeline validated.
- [ ] E-08 — DPA template published + at least 1 customer DPA executed.

## F. Marketing / Launch track (mirrors `GA-GATE-CRITERIA.md` §6 — owner: Owner / CEO)

- [ ] F-01 — Launch announcement drafted + scheduled (T-24h send).
- [ ] F-02 — Status page banner template ready (per `RB-GA-CUTOVER.md` §0.5).
- [ ] F-03 — Pricing page live + matches `specs/_audits/2026-05-15-stripe-customer-portal-spec.md`.
- [ ] F-04 — Sales onboarding deck reviewed.
- [ ] F-05 — Press kit ready (logo, founder bio, one-pager).
- [ ] F-06 — Owner availability confirmed for cutover window + first 72 h post-cutover.

## G. External dependencies (§11 DEFER counter — owner: Owner)

- [ ] G-01 — LFPDPPP MX attorney sign-off (§11#1).
- [ ] G-02 — FW-H-1..4 role nominations (§11#2).
- [ ] G-03 — External pentest vendor + SOW (§11#3).
- [ ] G-04 — DEBT-003 AWS Artifact PDF download + sha256 (§11#4).
- [ ] G-05 — DEBT-016 Statuspage `status.corelink.dev` go-live (§11#5).
- [ ] G-06 — Pilot signups ≥ 3 design-partners (§11#6).
- [ ] G-07 — Owner sign-off (ADR-0034b 2-key) executed in §13.2 + §13.3 of `2026-05-16-ga-readiness-final.md` (§11#7). _(Wave-25 scrub: prior G-07 "Docs CI billing reinstatement" removed as stale — CI runs locally per `feedback_ci_local`; GHA infra not used. Former G-08 promoted to G-07.)_

## H. Adversarial review (cross-cutting; mirrors `2026-05-16-ga-readiness-final.md` §3)

- [ ] H-01 — Wave-18 aggregate ≥ 9.0/10 (recorded 9.5/10 — PASS).
- [ ] H-02 — Wave-19 aggregate ≥ 8.5/10 (recorded 8.86/10 — PASS).
- [ ] H-03 — Wave-20 aggregate ≥ 9.0/10 (recorded 9.40/10 — PASS).
- [ ] H-04 — Wave-21 aggregate ≥ 9.0/10 (recorded 9.55/10 — PASS).
- [ ] H-05 — Wave-22 aggregate ≥ 9.0/10 (recorded 9.45/10 — PASS).
- [ ] H-06 — Wave-23 stream #1 codex Opus review aggregate ≥ 8.5/10 with no P0 + no P1.
- [ ] H-07 — Wave-24 stream #2 codex Opus review (wave-23 cross-check) aggregate ≥ 8.5/10 with no P0 + no P1.

## I. DEBT register reconciliation (mirrors `2026-05-16-ga-readiness-final.md` §5)

- [ ] I-01 — DEBT-003 closed OR `defer: §11#4`.
- [ ] I-02 — DEBT-008 empirical subset ≥ 6 crates CLOSED + 5 new wave-23 crates resolved (CLOSED or documented escalation).
- [ ] I-03 — DEBT-010 partial state acknowledged (4/11 closed; 7 P2/P3 deferred post-GA per waiver).
- [ ] I-04 — DEBT-013 partial state acknowledged (6/10 closed; 4 deferrals post-GA per waiver).
- [ ] I-05 — DEBT-015-BUILD CLOSED (stream #3 SEAL) OR escalated to P1 + waiver.
- [ ] I-06 — DEBT-016 closed OR `defer: §11#5`.
- [ ] I-07 — DEBT-025 closed OR `defer: §11#1`.
- [ ] I-08 — No new P0 DEBT row opened in last 30 d.

## J. Framework + governance (mirrors `2026-05-16-ga-readiness-final.md` §17)

- [ ] J-01 — `specs/00_framework.md` v1.0.0-rc1 status documented (DRAFT, FROZEN staffing-blocked; promotion to v1.0.0 explicitly scheduled wave-27 per ADR-0034b 2-key path).
- [ ] J-02 — ADR-0034 PRR staffing waiver in effect; 2-key signature path codified in §13.2 + §13.3.
- [ ] J-03 — All ADRs `doc_status: FROZEN` referenced by ≥ 1 spec or component (per framework §6 INV-LIFECYCLE-001).
- [ ] J-04 — No documents in `doc_status: REVIEW` longer than 14 d (no stale-review backlog).

---

## Roll-up

- **Total rows:** 94 (A:15 + B:12 + C:16 + D:9 + E:8 + F:6 + G:7 + H:7 + I:8 + J:4 + 2 buffer).
- **Decision rule:** signature §13.2 + §13.3 require **zero `false` rows**; every `defer:` traces to `2026-05-16-ga-readiness-final.md §11` (7 max post wave-25 scrub) or `GA-GATE-GO-NOGO-TEMPLATE.md §3` waiver register.
- **Expected DEFER count at signature:** 7 (the §11 external DEFER items, post wave-25 scrub). Any DEFER above 7 requires Owner-approved waiver per `GA-GATE-GO-NOGO-TEMPLATE.md §3`.

---

## Snapshot record

- **Branch:** `wt/r-prep-ga-readiness-final-audit`.
- **Base commit:** `33138b5` (wave-23 SEAL tip).
- **Audit date:** 2026-05-16.
- **Companion audit:** `specs/_audits/2026-05-16-ga-readiness-final.md`.
- **Co-Authored-By:** Claude Opus 4.7 <noreply@anthropic.com>.
