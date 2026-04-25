---
id: "WI-S06-007"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-25"
updated: "2026-04-25"
lane: "HIGH_RISK"
lane_forcing_factors: ["FF-HR-011", "FF-HR-005", "FF-HR-009"]
parent: "S-06"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "REMOTE-CACHE-PRODUCT-PROFILE"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "INVARIANT-REGISTRY"
  - "SLO-CATALOG"
tags: ["wi", "s06", "gc", "dashboard", "rb-dry-run", "prr", "ship-gate", "high-risk"]
---

# WI-S06-007 — DASH-GC Dashboards (10 panels) + Customer-Visible `bytes_reclaimed_last_30d` Métrica + RB-FM-300/404/305 Dry-Runs Executados + PRR HIGH_RISK 13 Sign-offs Ship Gate + 30d Sustained TLA+ Verde + Cumulative INV §3.17 Promotion + ADR-0042 Ratificação

> **doc_status:** DRAFT · **work_status:** READY · **lane:** HIGH_RISK
> **Parent:** [S-06](../sprint.md) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S06-007 |
| Título | DASH-GC dashboards (10 panels: mark/sweep/physical-delete rates per region; reclaim bytes per tenant_tier; INV-GC-004 violations counter; refcount drift; sweeper tick rate; orphan candidate count; phase budget exceeded; degrade-mode active; TLA+ CI 30d sustained); customer-visible `bytes_reclaimed_last_30d` business metric (S-16 forward); 3 RB dry-runs executados em staging (RB-FM-300 refcount bug; RB-FM-404 gc-write-race; RB-FM-305 tombstone lost); PRR HIGH_RISK 13 sign-offs ship gate; 30d sustained TLA+ verde + chaos sob load (sprint contract DoD); cumulative INV §3.17 promotion; ADR-0042 ratificação confirmation |
| Sprint | S-06 |
| Lane | HIGH_RISK |
| Forcing factors | FF-HR-011 (ship gate prevents catastrophic FM-300/305/404), FF-HR-005 (PRR enforces all CTRL-GC controls), FF-HR-009 (defense-in-depth final validation) |

## 1. Intent

Este WI é o **ship gate** de S-06; entra production-ready apenas após:

1. **DASH-GC dashboards** (10 panels):
   - Mark/sweep/physical-delete rates per region.
   - Reclaim bytes per tenant_tier (customer-visible business métrica via S-16 forward).
   - INV-GC-004 violations counter (CRITICAL alert se > 0; must always = 0).
   - Refcount drift % global + per-tenant max (alert > 0.1% global; > 1% per-tenant).
   - Sweeper tick rate per region (alert if cron stale > 1h).
   - Orphan candidate count per tenant.
   - Phase budget exceeded counter (mark / sweep / physical-delete / reconcile).
   - Degrade-mode `gc-pause` active gauge (alert if sustained > 1h sem ADR).
   - TLA+ CI 30d sustained verde gauge (S-20 GA promotion gate).
   - REAPI conformance (não applicable for GC; replaced com SLO-CORRECT-GC sustained metric).

2. **Customer-visible reclaim metric** (sprint contract §14.s06.7):
   - `corelink.cache.bytes_reclaimed_last_30d{tenant_id, tier}` (gauge).
   - Customer dashboard S-16 (forward) displays.
   - Per-tier breakdown: free / solo / team / business / enterprise.

3. **3 Runbook dry-runs executados em staging**:
   - **RB-FM-300** (refcount bug): chaos PR introduces refcount drift; engineer follows runbook; detection ≤ 5min via reconcile alerts; remediation ≤ 30min; post-mortem.
   - **RB-FM-404** (gc-write-race): chaos PR introduces concurrent UpdateActionResult mid-sweep; engineer follows runbook; INV-GC-004 caught (or not — runbook iteration); detection ≤ 1min via property test alert; remediation ≤ 30min.
   - **RB-FM-305** (tombstone lost): chaos PR simulates GC not running for 7d; engineer follows runbook; storage growth detected; manual mark+sweep trigger via admin API.

4. **30d sustained TLA+ verde** (sprint contract §10.s06.4 + Critério Promoção):
   - CI history checked daily via `tla-30d-sustained.yml` (WI-S06-006).
   - All 30 days verde required pre-S-20 GA promotion.
   - Concurrent S-07/S-08 sprints (post-S-06 sprint timeline).

5. **Chaos test 30d staging** (sprint contract DoD):
   - GC + write load 1k QPS sustained 4h; INV-GC-001/004 violations = 0.
   - Extended 30d staging com synthetic workload (Bazel + multipart) production-like.
   - Pause clock on P0/P1 incidents (4-tier classification Lote 10.4bis lesson).

6. **PRR HIGH_RISK 13 sign-offs**:
   - 12 mandatory (Owner + Final Approver + SRE Lead + Security Lead + Engineer×2 + QA + Product + Compliance + Privacy + Architect + AppSec) + Crypto SME mandatory (TLA+ obligation alignment; Lote 10.4bis lesson Crypto SME mandatory non-waivable).
   - Each role's checklist + sign-off; rubber-stamp prohibido (Lote 10.4bis lesson).
   - 2h structured PRR meeting.

7. **Cumulative INV §3.17 promotion** (~14 INVs):
   - INV-GC-IDEMPOTENT-RERUN, INV-GC-SINGLE-RUNNING-PER-TENANT-REGION, INV-GC-PHASE-MONOTONIC, INV-GC-MARK-STARTED-AT-IMMUTABLE, INV-GC-DEGRADE-MODE-PROBE-PER-BATCH (WI-S06-001).
   - INV-GC-MARK-STARTED-AT-ATOMIC, INV-GC-REACHABLE-SET-COMPLETE, INV-GC-MARK-TENANT-SCOPED, INV-GC-MARK-PHASE-BUDGETED, INV-GC-MARK-D1-BOUNDED-BATCH (WI-S06-002).
   - INV-GC-SWEEP-AUDIT-FAIL-CLOSED, INV-GC-SWEEP-IDEMPOTENT, INV-GC-SWEEP-TENANT-SCOPED, INV-GC-GRACE-RESPECTED (WI-S06-003).
   - INV-GC-PHYSICAL-DELETE-IDEMPOTENT, INV-GC-GRACE-BOUNDARY-STRICT, INV-GC-R2-D1-ORDERING, INV-GC-DSR-BYPASS-AUTHORIZED (WI-S06-004).
   - INV-GC-RECONCILE-AUTO-FIX-BOUNDED, INV-GC-RECONCILE-AUDIT-FAIL-CLOSED (WI-S06-005).
   - INV-GC-CI-GATE-ENFORCED, INV-GC-PROPERTY-TEST-CROSS-VALIDATED, INV-GC-30D-SUSTAINED-VERIFICATION (WI-S06-006).
   - **CI gate** validate_inv_promotion.py validates (lesson Lote 10.4bis introduced gate).

8. **ADR ratificação confirmation**:
   - ADR-0042 (worker scheduler design + degrade-mode contract): ratificada em WI-S06-001.
   - All whitelisted em validate_references.py.

9. **Cost regression gate all-WIs**:
   - Per-cron-tick + checkpoint (WI-001) ≤ targets.
   - Per-mark-batch (WI-002) ≤ $0.000005.
   - Per-sweep (WI-003) ≤ $0.000005.
   - Per-physical-delete (WI-004) ≤ $0.000005.
   - Per-reconcile-batch (WI-005) ≤ $0.000005.
   - CI gate ±10% tolerance.

10. **Customer-facing communication ready**:
    - SLA addendum draft `docs/customer/gc-sla-addendum-s06-ga.md`.
    - Release notes `docs/customer/release-notes-s06.md`.
    - "How CoreLink reclaims storage safely" customer doc.

11. **Production rollout plan** (Lote 10.4bis lesson): 10% → 50% → 100% gradual.

## 2. Narrative (HIGH_RISK ≥ 300 palavras)

S-06 ship gate é go/no-go review pre-S-20 GA pendency. Bug em ship = customer trust loss permanente (FM-300 reachable deleted = loss of customer data + reputational damage). Aplicar lições preventivamente Lote 10.4bis + 10.5bis:

1. **Sprint contract drift defense**: §19 já não tem 95% TLA+ verde waiver (lesson Lote 10.4bis aplicada).

2. **PRR sign-off rubber-stamp prevention**: each role's checklist + sign-off requires "I read XYZ and verified A/B/C"; CI gate validates non-empty entries.

3. **Crypto SME mandatory consistency**: WI-S06-002/003/006 (cripto WIs) Crypto SME MANDATORY EMPHATIC (TLA+ obligation alignment).

4. **30d sustained TLA+ verde**: post-sprint observation period; concurrent S-07/S-08 sprints; CI history validates.

5. **30d sustained chaos test**: synthetic workload Bazel + multipart 30d; INV-GC-001/004 0 violations.

6. **3 RB dry-runs executados**: RB-FM-300/404/305; runbook gaps iterated; post-mortem written.

7. **4-tier incident classification** (Lote 10.4bis lesson):
   - **P0**: INV-GC-001/004 violation; refcount drift > 1% per tenant; TLC red sustained > 4h. Resets clock; SEV-0 incident response.
   - **P1**: refcount drift > 0.1% global; phase budget exceeded sustained; degrade-mode `gc-pause` sustained > 1h. Resets clock.
   - **P2**: orphan candidate count spike (mark phase slow); reconcile auto-fix > threshold. Resets clock.
   - **P3**: bytes_reclaimed metric anomaly; informational alerts. Does NOT reset.

8. **Customer-facing comm pre-ship**: SLA addendum + release notes + "How CoreLink reclaims storage safely" doc.

9. **DSR erasure interaction tested** (sprint contract §6 DoD): erasure trigger immediate physical-delete (bypass grace) sem violar INV-GC-001 dos outros tenants.

**Risk justification HIGH_RISK**: ship gate prevents catastrophic FM-300/305/404; PRR enforces all CTRL-GC controls; defense-in-depth final validation across 6 prior WIs.

13 sign-offs.

## 3. Customer Impact & Journey

**Persona 1 — Customer**: post-S-06 SEALED, GC reclama storage automaticamente; customer-visible via `bytes_reclaimed_last_30d` em dashboard S-16; "minha conta está mais limpa esse mês".

**Persona 2 — DevOps reviewing operational readiness**: DASH-GC live; INV-GC-004 violations counter = 0 sustained; refcount drift < 0.1% sustained; 3 RBs dry-run documentados.

**Persona 3 — Compliance reviewer (LGPD/GDPR + SOC 2)**: reviews TLA+ formal verification em 30d sustained; reviews customer trust claim "nunca perdi blob reachable em 30d staging"; reviews DSR erasure interaction (S-11 forward).

**SLA addendum**:
- Mark p99 ≤ 10 min @ 1M blobs.
- Sweep p99 ≤ 5 min @ 100k candidates.
- Physical-delete p99 ≤ 30 min @ 100k candidates.
- Reconcile p99 ≤ 1h @ 1M blobs.
- Refcount drift < 0.1% global sustained.
- INV-GC-001/004 0 violations sustained 30d.
- TLA+ verde sustained 30d (CI history).
- Customer-visible `bytes_reclaimed_last_30d` per tenant_tier.

## 4. Capability Mapping

- All CAP-GC-001..008 — VALIDATED end-to-end.
- Trace: `slo_catalog.md SLO-CORRECT-GC + SLO-FRESH-GC` + `failure_modes.md FM-300/305/404 + RB-FM-*` + `observability_model.md DASH-GC` + `invariant_registry.md INV-GC-001..004`.

## 5. Tipo

PRR ship gate; HIGH_RISK; FF-HR-011 + FF-HR-005 + FF-HR-009.

## 6. Escopo (compact mas completo)

### 6.1 In-scope

1. **DASH-GC Grafana JSON** (10 panels):
   - Panel 1: Mark/Sweep/PhysicalDelete rates per region (line graph).
   - Panel 2: Reclaim bytes per tenant_tier (customer-visible business; emit S-09 → S-16).
   - Panel 3: INV-GC-004 violations counter (CRITICAL alert > 0).
   - Panel 4: Refcount drift % global + per-tenant max (alert > 0.1% / 1%).
   - Panel 5: Sweeper tick rate per region (alert if stale > 1h).
   - Panel 6: Orphan candidate count per tenant.
   - Panel 7: Phase budget exceeded counter (4 phases).
   - Panel 8: Degrade-mode `gc-pause` active gauge.
   - Panel 9: TLA+ CI 30d sustained verde gauge (S-20 GA gate).
   - Panel 10: SLO-CORRECT-GC + SLO-FRESH-GC sustained.

2. **Alert rules**:
   - INV-GC-004 violations > 0 → SEV-0 PagerDuty.
   - Refcount drift > 1% per-tenant → SEV-1.
   - Refcount drift > 0.1% global → SEV-2.
   - Sweeper cron stale > 1h → SEV-1.
   - Degrade-mode `gc-pause` sustained > 1h sem ADR → SEV-2.
   - Phase budget exceeded sustained → SEV-2.
   - TLC CI red sustained > 4h → SEV-1.

3. **Customer-visible métrica** `bytes_reclaimed_last_30d`:
   - Emit via S-09 aggregation; export to S-16 customer dashboard.
   - Per-tier breakdown.
   - SLO target: > 0 bytes em workload sintético (sprint contract §6 DoD).

4. **3 Runbook dry-runs em staging**:
   - **RB-FM-300** (refcount bug):
     - Setup: chaos PR introduces refcount drift (UpdateAR não mantém refcount).
     - Engineer follows runbook step-by-step.
     - Time-track: detection ≤ 5min via reconcile SEV-2 alert; remediation ≤ 30min via auto-fix OR manual; customer notif ≤ 1h.
     - Post-mortem `specs/_audits/2026-XX-XX-rb-fm-300-dry-run.md`.
   - **RB-FM-404** (gc-write-race):
     - Setup: chaos PR introduces concurrent UpdateActionResult mid-sweep.
     - Engineer follows runbook.
     - INV-GC-004 caught via property test 100k race + chaos test under load.
     - Detection ≤ 1min via prop test alert; remediation ≤ 30min.
     - Post-mortem.
   - **RB-FM-305** (tombstone lost):
     - Setup: chaos PR simulates GC not running 7d (cron paused).
     - Engineer follows runbook.
     - Detection via storage growth metric + sweeper tick rate alert.
     - Remediation: manual mark+sweep trigger via admin API stub.
     - Post-mortem.

5. **30d sustained TLA+ verde gate**: WI-S06-006 workflow `tla-30d-sustained.yml` daily aggregate verde 30d; required pre-S-20 GA.

6. **30d sustained chaos test**: 30d staging with concurrent Bazel + multipart synthetic workload; INV-GC-001/004 violations metric = 0; SLO sustained metrics tracked.

7. **PRR HIGH_RISK 13 sign-off ceremony** (2h structured):
   - Per-role checklist (Lote 10.4bis pattern).
   - Crypto SME MANDATORY non-waivable (TLA+ obligation; cripto WIs).
   - Sign-offs via GitHub PR review + git commit signing.

8. **Cumulative INV §3.17 promotion** (~22 INVs):
   - Update `specs/03_architecture/invariant_registry.md` §3.17 NEW.
   - CI gate `validate_inv_promotion.py` validates (Lote 10.4bis introduced).

9. **ADR ratificação confirmation**: ADR-0042 DRAFT → ACCEPTED + whitelist em validate_references.py.

10. **Cost regression gate** validation across all WIs.

11. **Customer-facing communication**: SLA addendum + release notes + safety doc.

12. **Production rollout plan**: 10% → 50% → 100% gradual (Lote 10.4bis lesson).

13. **Property test 100k tenant isolation cumulative** (sprint contract DoD §6): extends WI-S06-003 prop test; 100k iter nightly CI; 0 cross-tenant violations.

### 6.2 Out-of-scope (deferred)

- DSR erasure full integration (S-11 forward; this WI receives signal stub).
- Multi-region failover testing (S-14).
- Customer onboarding automation (S-15).
- Per-tenant SLA customization (S-13).

## 7. Anti-Scope

- ❌ Ship sem 30d sustained TLA+ verde.
- ❌ Ship sem 13 sign-offs.
- ❌ Ship sem 3 RB dry-runs executados.
- ❌ Ship com 1+ INV-GC-001/004 violation em 30d.
- ❌ Ship com refcount drift > 0.1% sustained.
- ❌ Ship com cost regression > 10%.
- ❌ Ship com ADR-0042 não ratificada.
- ❌ Ship com ~22 INVs §3.17 não promovidas.
- ❌ Ship com sign-off rubber-stamp.
- ❌ Production rollout direct 100% (gradual mandatory; Lote 10.4bis).
- ❌ Skip 30d chaos sustained.
- ❌ Skip post-ship review.

## 8. Acceptance Criteria (Gherkin) (compact 12 scenarios)

```gherkin
Feature: S-06 PRR HIGH_RISK 13 sign-off ship gate

  Scenario: DASH-GC dashboards live + alerts validated
    Given Grafana 10 panels provisioned
    Then alerts route to PagerDuty + Slack
    And INV-GC-004 violations panel shows 0 sustained
    And refcount drift panel shows < 0.1% sustained

  Scenario: Customer-visible bytes_reclaimed_last_30d emitted
    Given S-09 aggregation emits corelink.cache.bytes_reclaimed_last_30d{tenant_id, tier}
    Then S-16 customer dashboard displays per-tier reclaim
    And metric > 0 em workload sintético (sprint contract DoD)

  Scenario: RB-FM-300 dry-run successful
    Given chaos PR introduces refcount drift
    When engineer follows RB-FM-300
    Then detection ≤ 5min via reconcile SEV-2; remediation ≤ 30min; customer notif ≤ 1h
    And post-mortem written

  Scenario: RB-FM-404 dry-run successful
    Given chaos PR concurrent UpdateAR mid-sweep
    When engineer follows RB-FM-404
    Then INV-GC-004 caught via property test alert ≤ 1min
    And remediation ≤ 30min

  Scenario: RB-FM-305 dry-run successful
    Given chaos PR GC paused 7d
    When engineer follows RB-FM-305
    Then storage growth detected; manual trigger via admin API resumes
    And post-mortem written

  Scenario: 30d sustained TLA+ verde gate
    Given CI history `tla-30d-sustained.yml` daily checks
    When 30d window passes verde
    Then metric corelink.ci.tla.30d_sustained_verde = TRUE
    And S-20 GA promotion unblocked

  Scenario: 30d sustained chaos zero violations
    Given continuous staging Bazel + multipart workload 30d
    When INV-GC-001/004 violations metric monitored
    Then violations_total = 0 sustained 30d
    And SLO-CORRECT-GC + SLO-FRESH-GC verde

  Scenario: PRR HIGH_RISK 13 sign-offs collected
    Given PRR meeting 2h structured executed
    When 13 roles sign + checklist evidence
    Then 12 mandatory + Crypto SME (mandatory; not advisory) collected
    And rubber-stamp prevented (CI gate validates entries)

  Scenario: ~22 INVs §3.17 promotion CI gate
    Given invariant_registry.md §3.17 NEW with ~22 INVs
    When validate_inv_promotion.py runs
    Then all WI-declared INVs match registry
    And CI green

  Scenario: ADR-0042 ratificada
    Given ADR-0042 file content
    Then doc_status = ACCEPTED (FROZEN per schema)
    And whitelist em validate_references.py

  Scenario: Cost regression gate green all WIs
    Given CI bench runs criterion benchmarks
    Then per-WI cost gates green
    And no > 10% regression

  Scenario: Ship gate green light
    Given all above scenarios green + 30d sustained TLA+ verde + 30d chaos sustained zero violations
    Then ship decision = GO
    And gradual rollout 10% → 50% → 100%
```

## 9. Design Decisions (compact)

- 9.1: DASH-GC 10 panels (parity com S-04 DASH-AC + S-05 DASH-MULTIPART).
- 9.2: Customer-visible reclaim metric (CAP-GC-006; sprint contract §14.s06.7).
- 9.3: 3 RB dry-runs mandatory (sprint contract §6 DoD).
- 9.4: 30d sustained gate (post-sprint observation; concurrent S-07/S-08).
- 9.5: 4-tier incident classification (Lote 10.4bis lesson).
- 9.6: Crypto SME MANDATORY non-waivable (TLA+ obligation; lesson Lote 10.4bis cripto WIs).
- 9.7: validate_inv_promotion.py CI gate (Lote 10.4bis lesson).
- 9.8: Production rollout gradual (Lote 10.4bis).
- 9.9: ADR-0042 ratificação confirmation.
- 9.10: Sprint contract drift defense (Lote 10.5bis lesson; 95% waiver REMOVED).

## 10. Completeness Criteria SOTA

- [ ] **10.s06.007.1** DASH-GC 10 panels live; alerts PagerDuty + Slack.
- [ ] **10.s06.007.2** Customer-visible bytes_reclaimed metric emit.
- [ ] **10.s06.007.3** **3 RB dry-runs executados** (RB-FM-300/404/305) com post-mortem.
- [ ] **10.s06.007.4** **30d sustained TLA+ verde gate** (sprint contract §10.s06.4 + Critério Promoção).
- [ ] **10.s06.007.5** **30d sustained chaos zero INV-GC-001/004 violations** (sprint contract DoD).
- [ ] **10.s06.007.6** PRR 13 sign-offs collected; rubber-stamp prevented.
- [ ] **10.s06.007.7** ~22 INVs §3.17 promovidas + CI gate green (validate_inv_promotion.py).
- [ ] **10.s06.007.8** ADR-0042 ratificada + whitelist.
- [ ] **10.s06.007.9** Cost regression gate green all WIs.
- [ ] **10.s06.007.10** Customer comm ready (SLA addendum + release notes + safety doc).
- [ ] **10.s06.007.11** Production rollout plan 10%→50%→100%.
- [ ] **10.s06.007.12** Property test 100k tenant isolation cumulative green.
- [ ] **10.s06.007.13** Refcount drift sustained < 0.1% em 7d staging (sprint contract §10.s06.5).
- [ ] **10.s06.007.14** **DSR erasure interaction tested** (sprint contract §6 DoD; bypass grace sem violar outros tenants).

## 11. DoD

- [ ] WI-S06-001..006 SEALED.
- [ ] DASH-GC 10 panels live; alerts validated.
- [ ] Customer-visible reclaim metric emit.
- [ ] 3 RB dry-runs + post-mortem.
- [ ] 30d sustained TLA+ verde gate.
- [ ] 30d sustained chaos zero violations.
- [ ] PRR 13 sign-offs collected.
- [ ] ~22 INVs §3.17 promovidas.
- [ ] ADR-0042 ratificada.
- [ ] Customer comm ready.
- [ ] Production rollout plan documented.
- [ ] Architect + AppSec + Security Lead + Crypto SME final review.
- [ ] Final approver (Gustavo) ship decision = GO.

## 12. Invariants Validated (cumulative S-06)

End-to-end validation of all S-06 invariants em §3.17 NEW (~22 INVs cumulative; listed §1.7).

Plus mantidas:
- INV-GC-001 (CRITICAL, registry §3.4 + TLA+).
- INV-GC-002 (MEDIUM, registry §3.4).
- INV-GC-003 (HIGH, registry §3.4).
- INV-GC-004 (CRITICAL, registry §3.4 + TLA+).
- INV-CAS-IMMUTABILITY (CRITICAL, registry §3.3).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| DASH-GC Grafana JSON | `dashboards/dash-gc.json` | JSON |
| Alert rules | `dashboards/alerts/dash-gc-alerts.yml` | YAML |
| RB-FM-300 dry-run report | `specs/_audits/2026-XX-XX-rb-fm-300-dry-run.md` | Markdown |
| RB-FM-404 dry-run report | `specs/_audits/2026-XX-XX-rb-fm-404-dry-run.md` | Markdown |
| RB-FM-305 dry-run report | `specs/_audits/2026-XX-XX-rb-fm-305-dry-run.md` | Markdown |
| PRR meeting notes | `specs/_audits/2026-XX-XX-s06-prr-meeting.md` | Markdown |
| 30d SLO compliance report | `specs/_audits/2026-XX-XX-s06-slo-30d.md` | Markdown |
| SLA addendum customer | `docs/customer/gc-sla-addendum-s06-ga.md` | Markdown |
| Release notes | `docs/customer/release-notes-s06.md` | Markdown |
| "Reclaiming storage safely" doc | `docs/customer/gc-feature-overview.md` | Markdown |
| CI ship-gate workflow | `.github/workflows/gc-ship-gate.yml` | YAML |
| Production rollout plan | `docs/internal/gc-prod-rollout-plan.md` | Markdown |

## 14. Quality Standards SOTA (compact)

- 14.s06.007.1: 30d sustained TLA+ verde non-negotiable.
- 14.s06.007.2: 30d sustained chaos zero violations cripto-grade gate.
- 14.s06.007.3: 3 RB dry-runs ≤ 5/30/60 min targets.
- 14.s06.007.4: Dashboard alerts PagerDuty + Slack.
- 14.s06.007.5: PRR 2h structured; 13 sign-offs.
- 14.s06.007.6: Cost gate ±10%.
- 14.s06.007.7: Customer comm pre-ship.
- 14.s06.007.8: ADR-0042 ratificada.
- 14.s06.007.9: Production rollout gradual.
- 14.s06.007.10: Refcount drift < 0.1% sustained 7d.

## 15. Chaos Experiments (12)

1. DASH-GC alert noisy → threshold tuning.
2. RB-FM-300 dry-run gap → iterate runbook.
3. RB-FM-404 dry-run INV-GC-004 not caught → CRITICAL post-mortem.
4. RB-FM-305 dry-run GC stuck → manual trigger admin.
5. 30d sustained TLA+ slip (CI red 1h sustained) → SEV-1; clock reset.
6. 30d chaos violation injected → SEV-0; ship blocked.
7. PRR sign-off rubber-stamp regression → CI gate red.
8. Cost regression > 10% → CI bench detects.
9. ADR-0042 ratificação rollback → CI gate red.
10. Customer comm gap → review process catches.
11. CI ship-gate workflow regression → CI red.
12. ~22 INVs §3.17 not promovidas → validate_inv_promotion.py CI red.

## 16. PRR

THE PRR. Esta WI é a PRR.

## 17. Sub-tasks

| ID | h |
|---|---|
| ST-001 DASH-GC Grafana 10 panels | 6 |
| ST-002 Alert rules + PagerDuty + Slack | 4 |
| ST-003 Customer-visible reclaim metric S-09 hook | 3 |
| ST-004 RB-FM-300 dry-run + post-mortem | 6 |
| ST-005 RB-FM-404 dry-run + post-mortem | 6 |
| ST-006 RB-FM-305 dry-run + post-mortem | 5 |
| ST-007 30d sustained TLA+ verde gate validation | 2 |
| ST-008 30d sustained chaos suite + workload | 6 |
| ST-009 PRR meeting prep + checklists per role | 4 |
| ST-010 PRR meeting execution (2h) | 2 |
| ST-011 13 sign-off collection | 3 |
| ST-012 Cost regression gate setup all WIs | 3 |
| ST-013 ~22 INVs §3.17 registry update | 3 |
| ST-014 ADR-0042 ratificação confirmation | 2 |
| ST-015 SLA addendum customer | 3 |
| ST-016 Release notes + safety doc | 4 |
| ST-017 Production rollout plan | 3 |
| ST-018 CI ship-gate workflow | 3 |
| ST-019 Final review iter (Architect + AppSec + Crypto SME + Security Lead) | 6 |
| ST-020 Ship decision documentation | 1 |
| ST-021 DSR erasure interaction test (S-11 forward stub) | 3 |

**Total Optimistic**: ~78h. PERT (O=70h, M=80h, P=120h): **~85h**.

## 18. Dependencies

- Hard: WI-S06-001..006 SEALED; Crypto SME availability; Architect availability; 30d staging continuous; Wrangler + Grafana + PagerDuty configured.
- Soft: S-09 aggregation pipeline (reclaim metric); S-16 customer dashboard; S-11 DSR signal stub.

## 19. Effort PERT: 85h. ## 20. Time-boxing: 100h hard limit.

## 21. Observability

DASH-GC 10 panels. Trace span `s06.ship_gate`, `prr.meeting`, `gc.rb_dry_run.*`.

## 22. Cost Analysis

**Sub-task cost** (one-time):
- DASH-GC provisioning: ~$120/yr Grafana hosting.
- 30d staging: ~$100 cost.
- PRR meeting: 26h person-time × 13 reviewers.

**Operational cost** (steady state):
- Sweeper cron 5 regions: ~$180/yr.
- DASH-GC: $120/yr.
- Property test 100k nightly: $365/yr.
- Total: ~$1k/yr maintenance.

**TCO**:
- Setup: ~85h × $100/hr = $8.5k one-time.
- Maintenance: $1k/yr.

## 23. API Contract

Public artifacts: SLA addendum + release notes + safety doc + conformance reports + DASH-GC URL. Internal: PRR meeting notes; sign-off log; ADR ratificações; production rollout plan.

## 24. Post-mortem Hooks

- Ship gate red (sub-gate fail) → blocker post-mortem.
- 30d sustained TLA+ slip → SEV-1.
- 30d chaos violation → SEV-0; CRITICAL.
- RB dry-run gap → runbook iteration.
- Property test 100k violation → CRITICAL.
- ADR-0042 ratificação rollback → 5-Why.
- Cost regression > 10% → bench drill-down.
- ~22 INVs §3.17 not promovidas → CI gate red.

## 25. Rollback / Recovery

Production rollout rollback: 100% → 50% → 10% → 0% via Wrangler version revert; degrade-mode `gc-pause` global stop available.

## 26. Security & Privacy (cumulative S-06; compact)

**STRIDE**: TenantCtx-only enforcement; INV-GC-004 strict `<`; audit fail-closed; degrade-mode admin-only; cron tenant-scoped. **LINDDUN**: tenant pseudonymous; reclaim metric per-tier (no PII); DSR erasure bypass authorized; LGPD Art. 16 retention compliance via grace period.

## 27. Knowledge Transfer

- Tech talk (3h): "S-06 GC GA — Architecture Overview + Operational Readiness + TLA+ Formal Verification".
- Doc `docs/customer/gc-feature-overview.md`.
- Workshop com all reviewers + on-call team.
- Onboarding test (10 questions): INV-GC-001/004 rationale, mark_started_at_ms anchor, sweep audit fail-closed, grace boundary strict <, refcount reconcile thresholds, TLA+ CI gate, RB-FM-300/404/305 procedures.

## 28. Risk Register (12-row 6-col)

| ID | Risco | Prob | Det | Imp | Exp | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | TLA+ scope incompleto | M | M | HIGH | M | LOW | Adversarial review quarterly + production trace |
| R-002 | PRR rubber-stamp | M | M | HIGH | M | LOW | Per-role checklist; CI gate; Lote 10.4bis lesson |
| R-003 | Property test 100k flake | L | H | LOW | L | LOW | Deterministic seeds; retry 3× |
| R-004 | RB dry-run gap | M | L | HIGH | L | LOW | Iterate + re-run |
| R-005 | DASH-GC alert noisy | M | M | MEDIUM | M | LOW | Calibration |
| R-006 | 30d sustained slip | L | M | MEDIUM | L | LOW | P3 minor não reseta; 4-tier (Lote 10.4bis) |
| R-007 | INV-GC-001/004 violation em 30d → SEV-0 | L | M | CRITICAL | L | LOW | TLA+ + property test 100k + chaos sob load 3 layers |
| R-008 | Crypto SME unavailable on ship | M | L | MEDIUM | L | LOW | 2-week advance; lesson Lote 10.4bis 40-80h booking |
| R-009 | Cost regression > 10% | M | L | MEDIUM | L | LOW | §14.10 cost gate |
| R-010 | ~22 INVs §3.17 não promovidas | M | M | HIGH | M | LOW | validate_inv_promotion.py CI gate; ST-013 explicit |
| R-011 | ADR-0042 ratificação rollback | L | L | LOW | L | LOW | ADR change requires ADR; transparency |
| R-012 | Refcount drift sustained > 0.1% | M | L | HIGH | L | LOW | Reconcile auto-fix + SEV-2 alert; sustained = ship blocker |

## 29. Review Checkpoints

D+0 WI-S06-001..006 SEALED; D+1 30d sustained TLA+ verde + chaos validation; D+2 DASH-GC live; D+3 RB-FM-300 dry-run; D+4 RB-FM-404; D+5 RB-FM-305; D+6 PRR meeting; D+7 customer comm; D+8 ship decision; D+9 production 10%; D+11 50%; D+13 100%; D+20 post-ship review.

## 30. Sign-off (HIGH_RISK 13)

| # | Role | Status |
|---|---|---|
| 1-2 | Owner / Final Approver (Gustavo) | _pending_ |
| 3 | SRE Lead | _staffing-blocked; ADR-0034 waiver via Architect compensation_ |
| 4 | Security Lead | _TBD; **mandatory** — INV-GC-004 + audit fail-closed_ |
| 5-6 | Engineer × 2 | _TBD; **mandatory**_ |
| 7 | QA | _TBD; **mandatory** — chaos suite + 100k race property test green_ |
| 8 | Product (Gustavo) | _pending_ |
| 9 | Compliance | _TBD; **mandatory** — LGPD Art. 16 retention_ |
| 10 | Privacy | _TBD; **mandatory** — DSR erasure interaction_ |
| 11 | Architect | _TBD; **mandatory** — ADR-0042 ratificação + cumulative INV §3.17_ |
| 12 | AppSec | _TBD; **mandatory emphatic** — TLA+ obligation alignment_ |
| 13 | Crypto SME | _**MANDATORY** (non-waivable; TLA+ formal verification + property test 100k race + INV-GC-004 strict `<` semantics)_ |

**Sign-off discipline** (Lote 10.4bis):
- 12 mandatory + Crypto SME mandatory non-waivable = 13 total.
- Per-role checklist evidence; rubber-stamp prohibido; CI gate validates.
- Sign-off via GitHub PR review + git commit signing.

## 31. Change Log

1.0.0 / 2026-04-25 / Gustavo: Criação WI-S06-007 (Lote 10.6; SOTA pós-Lote 10.5bis lessons applied: 4-tier incident classification; Crypto SME mandatory non-waivable; validate_inv_promotion CI gate; production rollout gradual; sprint contract drift defense; canonical_bytes layouts; ADR canonical path adrs/).

## 32. Anti-patterns evitados

- ❌ Ship sem 30d sustained TLA+ verde; ❌ Ship sem 13 sign-offs; ❌ Ship sem 3 RB dry-runs; ❌ Ship com INV-GC-001/004 violation; ❌ Ship com refcount drift > 0.1% sustained; ❌ Ship com cost regression > 10%; ❌ Ship com ADR não ratificada; ❌ Ship com ~22 INVs não promovidas; ❌ Ship com sign-off rubber-stamp; ❌ Production rollout direct 100%; ❌ Skip 30d chaos sustained; ❌ Skip post-ship review; ❌ Crypto SME advisory para cripto WIs (lesson Lote 10.4bis fix).

---

**Fim WI-S06-007.** S-06 spec FULL SOTA completo (7 WIs HIGH_RISK; 1 ADR forward 0042; ~22 INVs §3.17 forward).

**Próximo**: dispatch 2 agent reviews (parts 1+2) Lote 10.6 review cycle; apply Lote 10.6bis P0 fixes; commit.
