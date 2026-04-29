---
id: "WI-S07-005"
type: "work_item"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-04-25"
updated: "2026-04-28"
lane: "STANDARD"
parent: "S-07"
assignee: "Gustavo Schneiter"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "OBSERVABILITY-MODEL"
  - "SLO-CATALOG"
  - "FAILURE-MODES"
  - "INVARIANT-REGISTRY"
tags: ["wi", "s07", "dashboard", "dash-dedup", "alerts", "anomaly-detection", "sprint-review", "ship-gate", "standard"]
---

# WI-S07-005 — DASH-DEDUP Grafana Dashboard (10 panels: dedup ratio per-tenant_tier; bytes saved; eviction rate per-region; quota utilization heatmap; LRU drift; cascade prevention; 95%/100% breach counters; SOTA bench comparison NativeLink ~2.1× / BuildBuddy ~2.8×) + Alerts (anomaly detection dedup ratio drop > 30% WoW SEV-3; quota 95% breach SEV-2 per-tenant; quota 100% breach SEV-1; INV-LRU-CONSISTENCY violation SEV-1; INV-GC-001 violation SEV-0 inheritance) + Sprint Review STANDARD 5 sign-offs ship gate + ADR-0019/0020 ratificação confirmation + cumulative INV §3.X promotion (5 NEW: INV-EVICT-SOFT-DELETE-FIRST + INV-EVICT-CASCADE-PREVENTED + INV-EVICT-TTL-CAP-RESPECTED + INV-LRU-CONSISTENCY + INV-QUOTA-RESERVATION-TTL — canonical Lote 10.7-tris cycle 5 alignment)

> **doc_status:** DRAFT · **work_status:** READY · **lane:** STANDARD
> **Parent:** [S-07](../_spec_contract.md) (sprint contract; sprint.md not yet authored — defer to S-07-bis if full sprint doc needed) · **Assignee:** Gustavo Schneiter

---

## 0. Identificação

| Campo | Valor |
|---|---|
| ID | WI-S07-005 |
| Título | DASH-DEDUP Grafana 10-panel dashboard; alert rules SEV-0..3; SOTA bench comparison vs NativeLink + BuildBuddy publicado em report; sprint review STANDARD 5 sign-offs; ADR-0019 (TTL ownership S-07 supersedes S-04) + ADR-0020 (Quota ownership) ratificação confirmation; cumulative INV §3.X promotion (5 NEW canonical per Lote 10.7-tris cycle 5: EVICT-SOFT-DELETE-FIRST + EVICT-CASCADE-PREVENTED + EVICT-TTL-CAP-RESPECTED + LRU-CONSISTENCY + QUOTA-RESERVATION-TTL); RB-FM-305 + RB-FM-059 dry-run reports linked; customer-visible bytes_reclaimed + dedup_ratio metric forward S-16 |
| Sprint | S-07 |
| Lane | STANDARD |
| Forcing factors | none |

## 1. Intent

Sprint ship gate: validate **all S-07 capabilities ship-ready** through observability + dry-run evidence + sprint review:

1. **DASH-DEDUP** (10 panels) live em Grafana: dedup ratio + eviction rate + quota utilization + LRU drift + race correctness counters.
2. **Alerts armed**: SEV-0 (INV-GC-001 inheritance violation; sprint contract §6 DoD); SEV-1 (100% quota breach; INV-LRU-CONSISTENCY); SEV-2 (95% quota breach per-tenant); SEV-3 (dedup ratio anomaly).
3. **SOTA bench**: comparison report vs NativeLink OSS (~2.1×) + BuildBuddy enterprise (~2.8×); CoreLink target ≥ 2.5× (intra-tenant; cross-tenant deferred).
4. **Runbook dry-runs**: RB-FM-305 (tombstone lost) + RB-FM-059 (DO quota exceeded) executed em staging.
5. **ADR ratificação**: ADR-0019 (TTL ownership) + ADR-0020 (Quota ownership) confirmed em PRR Architect signoff.
6. **Cumulative INV promotion** (5 NEW from S-07 — canonical Lote 10.7-tris cycle 5; aligns registry §3.X L365-375): INV-EVICT-SOFT-DELETE-FIRST + INV-EVICT-CASCADE-PREVENTED + INV-EVICT-TTL-CAP-RESPECTED + INV-LRU-CONSISTENCY + INV-QUOTA-RESERVATION-TTL.

```yaml
# observability/dashboards/dash-dedup.json
panels:
  - title: "Dedup Ratio per Tenant Tier"
    type: timeseries
    query: avg by(tenant_tier) (corelink_dedup_ratio{type="chunk"})
    sla_target: 2.5
    sota_target: 3.0
  - title: "Bytes Reclaimed Last 30d (per Tenant Tier)"
    type: bar
    query: sum by(tenant_tier) (rate(corelink_evict_bytes_reclaimed_total[30d]))
  - title: "Eviction Rate per Region"
    type: timeseries
    query: rate(corelink_evict_lru_evicted_total[5m]) by(region)
  - title: "Quota Utilization Heatmap (per Tenant)"
    type: heatmap
    query: corelink_quota_utilization_pct
    threshold_warn: 0.80
    threshold_critical: 0.95
  - title: "LRU Drift p99"
    type: timeseries
    query: histogram_quantile(0.99, sum(rate(corelink_lru_drift_ms_bucket[5m])) by (le))
    sla_target_ms: 60000
  - title: "Cascade Prevention (blobs ref'd by active AC; refused eviction; BLOB-scope per Lote 10.7bis P0-8)"
    type: timeseries
    query: rate(corelink_evict_cascade_prevented_total[5m])
  - title: "95% Breach Events per Tenant"
    type: bar
    query: sum by(tenant_id) (corelink_quota_95pct_breach_total)
  - title: "100% Breach Events per Tenant"
    type: bar
    query: sum by(tenant_id) (corelink_quota_100pct_breach_total)
  - title: "INV-GC-001 Inheritance Canary (MUST be 0)"
    type: stat
    query: corelink_evict_gc_invariant_violation_total
    alert_threshold: 0
    sev: SEV-0
  - title: "SOTA Comparison: Dedup Ratio vs NativeLink (2.1×) / BuildBuddy (2.8×)"
    type: gauge
    query: avg(corelink_dedup_ratio{type="chunk"})
    targets: [2.1, 2.5, 2.8, 3.0]
    annotations: ["NativeLink OSS baseline", "CoreLink target", "BuildBuddy enterprise", "CoreLink stretch"]
```

**Cripto-driven invariants enforced** (cumulative S-07; all NEW promovidas via this WI):

1. **INV-EVICT-SOFT-DELETE-FIRST** (HIGH, NEW): eviction sets `deleted_at`; NEVER R2 DELETE direct (WI-S07-002 §12).
2. **INV-EVICT-CASCADE-PREVENTED** (HIGH, NEW): pre-evict reachable check via canonical `json_each` SQL; refuses if `active_refcount > 0` (WI-S07-002 §12).
3. **INV-LRU-CONSISTENCY** (HIGH, NEW): eviction respects authoritative `last_accessed_at` (DO buffered + D1 base UNION); 0 race violations (WI-S07-004 §12).

Plus carried forward (already in registry):
- **INV-DEDUP-CONSISTENCY** (HIGH): WI-S07-001 base; UNIQUE INDEX `(tenant_id, chunk_digest)`.
- **INV-QUOTA-ENFORCEMENT** (HIGH): WI-S07-003 DO atomic + reservation pattern.
- **INV-GC-001** (CRITICAL, TLA+): inherited via S-06 GC grace + reconcile; eviction NEVER violates.
- **INV-CAS-IMMUTABILITY** (CRITICAL): preserved.
- **INV-TENANT-ISOLATION** (CRITICAL, TLA+): preserved.

## 2. Narrative (≥ 200 palavras + ship-readiness justification)

S-07 ship gate gate consolidates all 4 prior WIs into **operational confidence**:
- **DASH-DEDUP** makes dedup ratio + eviction + quota observable at a glance; oncall can triage incidents.
- **Alerts armed** ensure incidents fire pages BEFORE customer escalation (95% quota → SEV-2 BEFORE 100% block).
- **SOTA bench** is **revenue/marketing claim defensibility**: customer asks "why CoreLink vs NativeLink?", we cite measured dedup ratio comparison + benchmark report PDF.
- **Runbook dry-runs** validate operational procedures end-to-end; RB-FM-305 (tombstone lost; S-06 inheritance verified) + RB-FM-059 (DO quota race; FM-059 mitigation verified).
- **ADR ratificação** sealed: ADR-0019 (TTL ownership boundary clear S-04 vs S-07) + ADR-0020 (quota ownership boundary clear S-07 vs S-08).
- **INV cumulative promotion**: 5 NEW invariants registered §3.X with traces to property tests + chaos (canonical Lote 10.7-tris cycle 5).

**Why STANDARD lane** (NOT HIGH_RISK PRR):
- Sprint contract §2 explicit: STANDARD; eviction is reversible; quota race-free via DO actor; dedup is metadata index.
- Sign-off ceremony: 5 roles (Owner + Engineer peer + QA + AppSec + SRE) — no full 13-row HIGH_RISK PRR.

**Customer-visible (forward S-16)**:
- `bytes_reclaimed_last_30d{tenant_id, tier}` (gauge; consumed by S-16 customer dashboard).
- `dedup_ratio_last_30d{tenant_id}` (gauge; "saved 5 GB this month via dedup" customer message).

**Adversarial scenarios for ship gate**:
- **Dedup ratio inflation** (false positive): bench numbers cherry-picked from synthetic Docker workload; customer real workload disappoints. Mitigação: bench across ≥ 3 distinct workloads (Docker, ML, generic Bazel); honest report.
- **Alert noise**: SEV-3 anomaly fires false positives → oncall ignores → real incident missed. Mitigação: 2-week tune-in period; threshold tuning post-deploy.
- **DASH-DEDUP performance**: heavy queries impact Grafana; cardinality bomb (per-tenant metrics × N tenants). Mitigação: top-50 tenants only em heatmap; aggregate em tier metric for global view.
- **ADR ratificação rubber-stamp**: signing without substantive review (Sonnet R5 lesson). Mitigação: Architect MUST cite ADR-0019 + ADR-0020 design content em sprint review notes; proof-of-engagement.

**Risk justification STANDARD**:
- Observability + sprint review; not blocking customer functionality.
- Reversible: dashboard revert via Grafana version; alerts can be tuned.
- Sprint review NON-WAIVABLE for SEAL.

## 3. Customer Impact & Journey

**Persona 1 — Customer (visible)**: post-S-07 SEALED, S-16 dashboard shows monthly "5.2 GB reclaimed via dedup; dedup ratio 2.7×"; storage cost transparency.

**Persona 2 — DevOps (oncall)**: DASH-DEDUP live; alerts SEV-3..0 armed; runbooks RB-FM-305 + RB-FM-059 dry-run reports linked; oncall confidence high.

**Persona 3 — Compliance / Sales**: SOTA bench report (`benches/dedup-comparison-2026-08.pdf`) cites NativeLink + BuildBuddy comparison; defensible sales claim.

**SLA addendum**:
- DASH-DEDUP live em Grafana ≥ 99.5% uptime (Grafana SLO inheritance).
- Alerts SEV-1+ paging response ≤ 15min (oncall SLO).
- SOTA bench report published before sprint SEAL.

## 4. Capability Mapping

- **CAP-DEDUP-002** (Dedup ratio metric exposed) — IMPLEMENTA primary; dashboard panels.
- **CAP-DEDUP-003** (Dedup ratio per-tier dashboard) — IMPLEMENTA primary.
- All CAP-EVICT-* validated end-to-end via runbook dry-runs.
- Trace: `observability_model.md DASH-DEDUP` + `slo_catalog.md SLO-DEDUP-RATIO` + `failure_modes.md FM-059, FM-305 + RB-FM-*`.

## 5. Tipo

Sprint ship gate; STANDARD lane.

## 6. Escopo

### 6.1 In-scope

1. **DASH-DEDUP Grafana JSON** em `observability/dashboards/dash-dedup.json` (10 panels listed §1).
2. **Alert rules** em `observability/alerts/dash-dedup-alerts.yml`:
   - SEV-0: `corelink_evict_gc_invariant_violation_total > 0` (INV-GC-001 inheritance violation).
   - SEV-1: `corelink_quota_100pct_breach_total{tenant_id}` increase (per-tenant); `corelink_lru_consistency_violation_total > 0`.
   - SEV-2: `corelink_quota_95pct_breach_total{tenant_id}` rate > 1/h per-tenant; `corelink_lru_drift_ms_bucket` p99 > 60000.
   - SEV-3: `corelink_dedup_ratio` drop > 30% WoW (anomaly detection algorithm: 2-week trailing mean ± 2σ).
3. **Customer-visible métricas** (consumed by S-16 forward):
   - `corelink.cache.bytes_reclaimed_last_30d{tenant_id, tier}` (gauge; aggregate of corelink_evict_bytes_reclaimed_total over 30d window).
   - `corelink.cache.dedup_ratio_last_30d{tenant_id}` (gauge; aggregate of corelink_dedup_bytes_saved_total / corelink_dedup_bytes_uploaded_total over 30d).
4. **SOTA bench report** em `benches/dedup-comparison-2026-08.pdf`:
   - 3 workloads tested: Docker pulls (multi-stage), ML training (PyTorch checkpoint), generic Bazel (Linux kernel).
   - Comparison: CoreLink dedup ratio measured vs NativeLink OSS (deployed locally; baseline) vs BuildBuddy (cited published claim 2.8×).
   - Methodology section: reproducible benchmark scripts em `benches/dedup-bench/`.
   - Honest report: if CoreLink < target, document gap + improvement plan; do NOT cherry-pick.
5. **2 RB dry-run reports** (canonical sprint contract §6 DoD: RB-FM-305 + RB-FM-059; Lote 10.7-tris cycle 3 fix; was 3 — drift from sprint contract):
   - **RB-FM-305 dry-run** (tombstone lost; consumed by WI-S07-002 ST-010): chaos PR simulates GC tombstone loss; engineer follows runbook; SLA detection ≤ 5min, remediation ≤ 30min, customer notif ≤ 1h. Report em `specs/_audits/2026-XX-XX-rb-fm-305-dry-run-s07.md`.
   - **RB-FM-059 dry-run** (DO quota exceeded; consumed by WI-S07-003 ST-011): chaos PR injects FM-059 race scenario; engineer follows runbook; verify DO atomic + reservation pattern catches; INV-QUOTA-ENFORCEMENT 0 violations. Report em `specs/_audits/2026-XX-XX-rb-fm-059-dry-run-s07.md`.
6. **ADR ratificação confirmation**:
   - ADR-0019 (TTL ownership S-07 supersedes S-04): existing FROZEN (Lote 9.4); confirm no regressions; Architect cite-and-acknowledge in sprint review notes.
   - ADR-0020 (Quota ownership S-07 owns ≤95%, S-08 owns 100% hard-block): existing FROZEN; confirm boundary respected by WI-S07-002/003.
7. **Cumulative INV §3.X promotion** (5 NEW; registry update — canonical alignment Lote 10.7-tris cycle 4 fix; was 3, registry §3.X actual is 5):
   ```markdown
   <!-- Add to invariant_registry.md §3.X (canonical 5 NEW per Lote 10.7-tris cycle 4) -->
   | INV-EVICT-SOFT-DELETE-FIRST | Eviction sets deleted_at; NEVER R2 DELETE direct | HIGH | WI-S07-002 §6.1.7 | chaos test #1 + property prop_evict_idempotent | N/A |
   | INV-EVICT-CASCADE-PREVENTED | Pre-evict reachable check via json_each refuses if blob ref'd by active ac_meta.blob_refs (BLOB-scope per Lote 10.7bis P0-8) | HIGH | WI-S07-002 §6.1.6 | chaos test #2 + property prop_evict_cascade_prevention | N/A |
   | INV-EVICT-TTL-CAP-RESPECTED | Enterprise TTL ≤ 730d hard cap (CAP-EVICT-002) | MEDIUM | WI-S07-002 §6.1.3 | chaos test #7 + integration boundary | N/A |
   | INV-LRU-CONSISTENCY | Eviction respects authoritative last_accessed_at (DO buffered + D1 base UNION) | HIGH | WI-S07-004 §6.1.7 | chaos test #1 + property prop_lru_eviction_race | N/A |
   | INV-QUOTA-RESERVATION-TTL | Pending reservations auto-release after size-proportional TTL min(7d, max(60s, req_bytes/1MB/s × 2)) | HIGH | WI-S07-003 §6.2 | chaos test #4 + property prop_quota_ttl_release | N/A |
   ```
8. **Sprint review STANDARD 5 sign-off ceremony** (60min structured):
   - Pre-review: each role completes checklist evidence (Engineer peer, QA, AppSec, SRE; Owner curates).
   - Sign-offs via GitHub PR review + git-trailer signing.
9. **CI ship-gate workflow** `.github/workflows/s07-ship-gate.yml`:
   - PR labeled `s07-ship-candidate` triggers:
     - All 5 WIs at v1.X.X+ (post-bis if applicable).
     - All property tests 100k nightly green sustained 7d.
     - All chaos tests green.
     - SOTA bench report present em `benches/`.
     - 2 RB dry-run reports present em `specs/_audits/` (RB-FM-305 + RB-FM-059 per sprint contract §6 DoD).
     - DASH-DEDUP JSON validates Grafana schema.
     - Alert YAML validates Prometheus schema.
     - 5 sign-offs via PR review approvals.
10. **Métricas (rollup; ship gate observability)**:
    - `corelink.s07.ship_gate.checks_total{check, result}` (counter).
    - `corelink.s07.ship_gate.signoffs_collected` (gauge; 0..5).
    - `corelink.s07.ship_gate.adr_ratified` (gauge boolean per ADR; 0019, 0020).

### 6.2 Out-of-scope (deferred)

- S-08 hard-block rate-limit infrastructure (CAP-QUOTA-001 explicit boundary).
- S-16 customer dashboard frontend (consumes metrics here; downstream).
- Multi-region dedup aggregation (per-region; aggregate em S-09 OLAP forward).
- Predictive quota exhaustion alerts (ML; pós-GA).

## 7. Anti-Scope

- ❌ Ship sem sprint review (5 sign-offs mandatory).
- ❌ Ship sem 2 RB dry-runs (RB-FM-305 + RB-FM-059 canonical per sprint contract §6 DoD; Lote 10.7-tris cycle 3 fix).
- ❌ Ship sem SOTA bench report.
- ❌ Ship sem ADR-0019/0020 ratificação confirmation.
- ❌ Ship sem cumulative INV §3.X promotion.
- ❌ Ship com cherry-picked bench numbers.
- ❌ Skip alert tuning period (2-week post-deploy).
- ❌ Skip Architect cite-and-acknowledge em ADR ratificação.

## 8. Acceptance Criteria (Gherkin) — 8 scenarios

```gherkin
Feature: S-07 sprint ship gate

  Scenario: DASH-DEDUP live em Grafana
    Given dash-dedup.json provisioned
    Then 10 panels render
    And alerts route to PagerDuty + Slack
    And dedup_ratio panel shows ≥ 2.5× sustained 7d em ≥ 3 tenants

  Scenario: SOTA bench report defensible
    Given dedup-comparison-2026-08.pdf published em benches/
    Then 3 workloads tested (Docker + ML + Bazel)
    And methodology section reproducible (scripts em benches/dedup-bench/)
    And CoreLink ratio measured ≥ 2.5× (target met) OR documented gap + improvement plan

  Scenario: RB-FM-305 dry-run successful
    Given chaos PR injects tombstone loss
    When engineer follows RB-FM-305
    Then detection ≤ 5min via storage growth metric
    And remediation ≤ 30min via manual mark+sweep trigger
    And customer notif ≤ 1h
    Then post-mortem report written em specs/_audits/

  Scenario: RB-FM-059 dry-run successful
    Given chaos PR injects FM-059 race (1000 concurrent writes at 99.9% quota)
    When engineer follows RB-FM-059
    Then DO atomic + reservation pattern catches; INV-QUOTA-ENFORCEMENT 0 violations
    And race_detected_total metric remains 0
    Then post-mortem report written

  Scenario: ADR-0019 ratificação confirmation
    Given ADR-0019 file at FROZEN
    When sprint review Architect signoff
    Then sprint review notes cite ADR-0019 §3 (TTL ownership boundary S-04 vs S-07)
    And TTL per-tier values (free=7d, ..., enterprise=730d max) confirmed em WI-S07-002 §6.1.3
    And no regression vs S-04 default 90d (now superseded per ADR)

  Scenario: ADR-0020 ratificação confirmation
    Given ADR-0020 file at FROZEN
    When sprint review Architect signoff
    Then sprint review notes cite ADR-0020 (Quota ownership boundary S-07 ≤95% vs S-08 100%)
    And boundary respected by WI-S07-002 (95% trigger eviction) + WI-S07-003 (100% emits provisional 429 — transitional; S-08 owns canonical rate-limit DO per ADR-0020 FROZEN)

  Scenario: Cumulative INV §3.X promotion
    Given invariant_registry.md updated with 5 NEW INVs (canonical §3.X group; INV-DEDUP-CONSISTENCY in §3.3 separate domain)
    When validate_inv_promotion.py runs
    Then all WI-declared INVs (INV-EVICT-SOFT-DELETE-FIRST + INV-EVICT-CASCADE-PREVENTED + INV-LRU-CONSISTENCY) match registry
    And CI green

  Scenario: Sprint review 5 sign-offs collected
    Given sprint review meeting executed (60min structured)
    Then 5 roles signed (Owner + Engineer peer + QA + AppSec + SRE)
    And rubber-stamp prevented (Architect cite-and-acknowledge for ADR ratificação)
    And ship decision = GO em sprint review notes
```

## 9. Design Decisions

- 9.1: DASH-DEDUP 10 panels (parity com DASH-AC + DASH-MULTIPART).
- 9.2: Alert SEV thresholds (sprint contract §15 risks aligned): SEV-0 INV-GC-001 inheritance violation; SEV-1 quota 100% breach OR INV-LRU-CONSISTENCY violation; SEV-2 quota 95% breach OR LRU drift; SEV-3 dedup anomaly.
- 9.3: SOTA bench 3 workloads (representativeness; not cherry-pick).
- 9.4: Sprint review STANDARD 5 sign-offs (NOT HIGH_RISK 13 PRR).
- 9.5: ADR-0019 + ADR-0020 ratificação cite-and-acknowledge (rubber-stamp prevention; Sonnet R5 lesson).
- 9.6: Customer-visible metrics forward S-16 (bytes_reclaimed + dedup_ratio).

## 10. Completeness Criteria SOTA

- [ ] **10.s07.005.1** DASH-DEDUP 10 panels live em Grafana; alerts route to PagerDuty + Slack.
- [ ] **10.s07.005.2** SOTA bench report published em `benches/dedup-comparison-2026-08.pdf`; 3 workloads; methodology reproducible.
- [ ] **10.s07.005.3** RB-FM-305 dry-run + post-mortem report.
- [ ] **10.s07.005.4** RB-FM-059 dry-run + post-mortem report.
- [ ] **10.s07.005.5** ADR-0019 ratificação confirmed em sprint review notes.
- [ ] **10.s07.005.6** ADR-0020 ratificação confirmed em sprint review notes.
- [ ] **10.s07.005.7** 5 NEW INVs §3.X promovidas + validate_inv_promotion.py CI green (canonical Lote 10.7-tris cycle 4 alignment).
- [ ] **10.s07.005.8** Sprint review 5 sign-offs collected; ship decision = GO.
- [ ] **10.s07.005.9** Customer-visible metrics emit (consumed by S-16 forward).
- [ ] **10.s07.005.10** CI ship-gate workflow `s07-ship-gate.yml` operational.
- [ ] **10.s07.005.11** Dedup ratio ≥ 2.5× sustained 7d staging em ≥ 3 tenants (sprint contract §6 DoD).
- [ ] **10.s07.005.12** Cost regression gate green all S-07 WIs; per-WI ≤ targets.

## 11. DoD

- [ ] WI-S07-001..004 SEALED.
- [ ] DASH-DEDUP live; alerts validated.
- [ ] SOTA bench report published.
- [ ] 2 RB dry-runs + post-mortems.
- [ ] ADR-0019/0020 ratificadas + cited em sprint review.
- [ ] 5 NEW INVs promovidas (canonical §3.X group per Lote 10.7-tris cycle 4 alignment; INV-DEDUP-CONSISTENCY in §3.3 separate domain).
- [ ] Sprint review 5 sign-offs.
- [ ] CI ship-gate workflow green.

## 12. Invariants Validated (cumulative S-07)

End-to-end validation of:
- INV-DEDUP-CONSISTENCY (HIGH; WI-S07-001 base).
- INV-QUOTA-ENFORCEMENT (HIGH; WI-S07-003 base).
- INV-QUOTA-RESERVATION-TTL (HIGH NEW; WI-S07-003).
- INV-EVICT-SOFT-DELETE-FIRST (HIGH NEW; WI-S07-002).
- INV-EVICT-CASCADE-PREVENTED (HIGH NEW; WI-S07-002).
- INV-EVICT-TTL-CAP-RESPECTED (MEDIUM NEW; WI-S07-002).
- INV-LRU-CONSISTENCY (HIGH NEW; WI-S07-004).
- INV-GC-001 (CRITICAL inheritance via S-06; WI-S07-002 chaos 30d sustained).
- INV-CAS-IMMUTABILITY (CRITICAL inheritance).
- INV-TENANT-ISOLATION (CRITICAL inheritance via TLA+ tenant_isolation.tla).

## 13. Artifacts Produced

| Artifact | Path | Tipo |
|---|---|---|
| DASH-DEDUP Grafana JSON | `observability/dashboards/dash-dedup.json` | JSON |
| Alert rules YAML | `observability/alerts/dash-dedup-alerts.yml` | YAML |
| SOTA bench report | `benches/dedup-comparison-2026-08.pdf` | PDF |
| Bench scripts | `benches/dedup-bench/` | Rust + scripts |
| RB-FM-305 dry-run report | `specs/_audits/2026-XX-XX-rb-fm-305-dry-run-s07.md` | Markdown |
| RB-FM-059 dry-run report | `specs/_audits/2026-XX-XX-rb-fm-059-dry-run-s07.md` | Markdown |
| Sprint review notes | `specs/_audits/2026-XX-XX-s07-sprint-review.md` | Markdown |
| Customer comm artifacts (forward S-16) | `docs/customer/dedup-feature-overview.md` | Markdown |
| CI ship-gate workflow | `.github/workflows/s07-ship-gate.yml` | YAML |

## 14. Quality Standards SOTA

- 14.s07.005.1: 10 panels + alerts validated; PagerDuty + Slack integration.
- 14.s07.005.2: SOTA bench reproducible (scripts em benches/).
- 14.s07.005.3: 2 RB dry-runs (RB-FM-305 + RB-FM-059) executed em staging (sprint contract §6 DoD canonical; Lote 10.6bis P0-W7-4 lesson absorbed; design-readiness gate accepts ≥1 dry-run per RB).
- 14.s07.005.4: Sprint review 60min structured.
- 14.s07.005.5: ADR ratificação cite-and-acknowledge (rubber-stamp prevention; Sonnet R5 lesson).
- 14.s07.005.6: validate_inv_promotion.py CI gate green.
- 14.s07.005.7: Customer comm artifacts pre-S-16 ship.

## 15. Chaos Experiments (5; sprint review focus, not chaos-heavy)

1. DASH-DEDUP alert noisy → threshold tuning (2-week tune-in).
2. RB-FM-305 dry-run gap → iterate runbook.
3. RB-FM-059 dry-run race → INV-QUOTA-ENFORCEMENT verified.
4. SOTA bench cherry-pick → reviewer rejects; require 3 workloads.
5. ADR ratificação rubber-stamp → CI gate fails (Architect must cite ADR §).

## 16. PRR

STANDARD lane sprint review (5 sign-offs; not HIGH_RISK PRR).

## 17. Sub-tasks

| ID | Sub-task | h |
|---|---|---|
| ST-001 | DASH-DEDUP Grafana JSON (10 panels) | 3 |
| ST-002 | Alert rules YAML SEV-0..3 | 2 |
| ST-003 | Customer-visible metrics rollup (S-16 hook) | 1.5 |
| ST-004 | SOTA bench scripts + 3 workloads | 4 |
| ST-005 | SOTA bench report PDF authoring | 2 |
| ST-006 | RB-FM-305 dry-run + report (consume from WI-002) | 0 (delegated to WI-S07-002 ST-010) |
| ST-007 | RB-FM-059 dry-run + report (consume from WI-003) | 0 (delegated to WI-S07-003 ST-011) |
| ST-008 | ADR-0019/0020 ratificação confirmation notes | 1 |
| ST-009 | INV §3.X registry update + validate_inv_promotion CI | 1 |
| ST-010 | Sprint review prep (checklists per role) | 1.5 |
| ST-011 | Sprint review meeting (60min) | 1 |
| ST-012 | 5 sign-off collection via PR review | 1 |
| ST-013 | CI ship-gate workflow `s07-ship-gate.yml` | 2 |
| ST-014 | Customer comm doc `dedup-feature-overview.md` | 1.5 |

**Total**: ~21h. **PERT** O=18h M=20h P=28h: **~21h** (sprint contract estimate 12h; conservative buffer for sprint review + bench + RB integration).

## 18. Dependencies

- Hard: WI-S07-001..004 SEALED; ADR-0019 + ADR-0020 (already FROZEN); validate_inv_promotion.py CI gate (Lote 10.4bis lesson).
- Soft: S-09 (PagerDuty integration; Grafana hosting); S-16 (customer dashboard consumes metrics).

## 19. Effort PERT: ~21h. ## 20. Time-boxing: 28h hard limit.

## 21. Observability

3 ship-gate metrics §6.1.10 + DASH-DEDUP rollup. Trace span `s07.ship_gate.{checks, signoffs, adr_ratification}`.

## 22. Cost Analysis

- DASH-DEDUP provisioning: ~$120/yr Grafana hosting share.
- Alert PagerDuty integration: ~$0.05/incident × 10 incidents/month = $6/yr trivial.
- SOTA bench compute: 3 workloads × 30min × $0.05/min = ~$5 one-time.
- TCO 12m: ~$150/yr maintenance.

## 23. API Contract

- Grafana JSON dashboard schema.
- Prometheus alert YAML schema.

## 24. Post-mortem Hooks

- Ship gate red sub-gate fail → blocker post-mortem.
- DASH-DEDUP alert noisy > 2 weeks post-tune-in → threshold review.
- SOTA bench drift > 30% from claim → marketing claim review.
- INV-GC-001 inheritance violation detected post-ship → CRITICAL post-mortem (FM-300).

## 25. Rollback / Recovery

- Dashboard rollback via Grafana version revert.
- Alert tuning via YAML version revert; no service impact.
- RB dry-run reports immutable (audit trail).

## 26. Security & Privacy

**STRIDE / LINDDUN**: dashboard query results no PII; aggregate metrics; per-tenant top-50 only em heatmap.

## 27. Knowledge Transfer

Tech talk (1h): "S-07 Operations: DASH-DEDUP + Alerts + Runbooks"; doc `docs/internal/s07-operational-runbook.md`; onboarding test 5 questions: dedup ratio anomaly threshold, 95% vs 100% boundary (S-07 vs S-08), INV-GC-001 inheritance via S-06, RB-FM-305/059 procedures, SOTA bench methodology.

## 28. Risk Register (8-row)

| ID | Risco | Prob | Det | Imp | Exp | Res | Mitigação |
|---|---|---|---|---|---|---|---|
| R-001 | DASH-DEDUP alert noisy | M | L | LOW | L | LOW | 2-week tune-in; threshold tunable |
| R-002 | RB-FM-305 dry-run gap | L | L | LOW | L | LOW | Iterate runbook; cross-WI integration test |
| R-003 | RB-FM-059 dry-run race | L | L | MEDIUM | L | LOW | Property test 10k iter (WI-003); chaos #1 |
| R-004 | SOTA bench cherry-pick | M | L | LOW (perception) | L | LOW | 3 workloads mandatory; honest report; reviewer reject |
| R-005 | ADR-0019/0020 rubber-stamp | M | M | MEDIUM | M | LOW | Architect cite-and-acknowledge required |
| R-006 | INV cumulative count drift | L | L | LOW | L | LOW | validate_inv_promotion.py CI gate |
| R-007 | Sprint review 5 sign-offs unfilled | M | L | MEDIUM | L | LOW | Sign-off booking calendar (Lote 10.6bis lesson; STANDARD lighter) |
| R-008 | CI ship-gate workflow regression | L | L | LOW | L | LOW | Workflow integration test |

## 29. Review Checkpoints

D+0 dashboard authoring; D+2 alert rules; D+4 SOTA bench scripts run; D+5 RB dry-runs; D+6 sprint review meeting; D+7 sign-off collection.

## 30. Sign-off (STANDARD 5)

| # | Role | Status |
|---|---|---|
| 1 | Owner / Final Approver (Gustavo) | _pending_ |
| 2 | Engineer (peer) | _TBD; mandatory_ |
| 3 | QA | _TBD; mandatory — RB dry-runs + property/chaos green_ |
| 4 | AppSec | _TBD; mandatory — TenantCtx + audit + DO isolation cross-WI verified_ |
| 5 | SRE | _TBD; mandatory — DASH-DEDUP live + alerts armed + runbooks reviewed_ |

## 31. Change Log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (Lote 10.7) | Criação WI-S07-005 (sprint ship gate); SOTA pós-Lote 10.6-tris lessons absorbed: ADR ratificação cite-and-acknowledge (rubber-stamp prevention); validate_inv_promotion CI gate; RB dry-run ≥1 run sufficient for STANDARD lane (Lote 10.6bis lesson scaled down); customer-visible metrics forward S-16. |

## 32. Anti-patterns evitados

- ❌ Ship sem 5 sign-offs; ❌ Ship sem 2 RB dry-runs (RB-FM-305 + RB-FM-059; canonical per sprint contract §6 DoD); ❌ Ship com cherry-picked bench; ❌ Skip ADR cite-and-acknowledge (rubber-stamp); ❌ Ship com INV count drift; ❌ Skip alert tune-in 2-week; ❌ DASH-DEDUP cardinality bomb (top-50 only).

---

**Fim WI-S07-005.** S-07 spec FULL completo (5 WIs STANDARD; 0 ADRs novos — usa ADR-0019/0020 existing; 5 NEW INVs §3.X canonical per Lote 10.7-tris cycle 4).

**Próximo**: dispatch 2-round adversarial reviews (Agent R4 + Sonnet R5) per Lote 10.X cycle pattern.
