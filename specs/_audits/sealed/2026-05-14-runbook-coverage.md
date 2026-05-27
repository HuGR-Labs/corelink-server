---
id: "AUDIT-2026-05-14-RUNBOOK-COVERAGE"
type: "audit"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["audit", "runbook", "coverage", "slo", "alert", "fm", "r6-2"]
---

# Runbook Coverage Audit — 2026-05-14 (R6-2)

> **Scope:** every SLO in `slo_catalog.md §4`, every alert class in `observability_model.md §9`, every P0/P1 failure mode in `failure_modes.md §3` must map to ≥ 1 runbook in `specs/05_quality/runbooks/`.
>
> **Status legend:** `yes` = canonical RB exists and covers symptom/detection/mitigation/escalation; `partial` = a related RB exists but is missing one of the 7 mandatory sections (Symptom, Detection, Immediate mitigation, Root-cause investigation, Rollback/recovery, Escalation path, Post-incident); `no` = no RB exists.
>
> **Canonical RB location:** `specs/05_quality/runbooks/RB-*.md`. Auxiliary locations (`specs/_runbooks/`, `specs/05_runbooks/`) are reachable via cross-link but the `validate_references.py` registry only resolves `RB-*` IDs defined in `specs/05_quality/runbooks/`.

---

## Table A — SLO → Runbook coverage (21 SLOs)

| SLO ID                          | §slo_catalog | Runbook                                                              | Before | After  | Gap closed by                |
|---------------------------------|--------------|----------------------------------------------------------------------|--------|--------|------------------------------|
| SLO-AVAIL-CP                    | §4.1         | `RB-SLO-AVAIL-CP`                                                    | yes    | yes    | —                            |
| SLO-AVAIL-CAS-GET               | §4.2         | `RB-SLO-AVAIL-DATA-PLANE` (new) + FM-050/057/250                     | no     | yes    | new RB-SLO-AVAIL-DATA-PLANE  |
| SLO-AVAIL-CAS-PUT               | §4.3         | `RB-SLO-AVAIL-DATA-PLANE` (new) + FM-050/052                         | no     | yes    | new RB-SLO-AVAIL-DATA-PLANE  |
| SLO-AVAIL-AC                    | §4.4         | `RB-SLO-AVAIL-DATA-PLANE` (new) + RB-FM-AC-*                         | no     | yes    | new RB-SLO-AVAIL-DATA-PLANE  |
| SLO-AVAIL-EXEC                  | §4.5         | `RB-SLO-AVAIL-DATA-PLANE` (new) — Fase 2 noted                       | no     | yes    | new RB-SLO-AVAIL-DATA-PLANE  |
| SLO-LAT-CAS-GET                 | §4.6         | `RB-SLO-LATENCY-INVESTIGATION` (new)                                 | no     | yes    | new RB-SLO-LATENCY-INVESTIGATION |
| SLO-LAT-CAS-PUT                 | §4.7         | `RB-SLO-LATENCY-INVESTIGATION` (new)                                 | no     | yes    | new RB-SLO-LATENCY-INVESTIGATION |
| SLO-LAT-AC-HIT                  | §4.8         | `RB-SLO-LATENCY-INVESTIGATION` (new)                                 | no     | yes    | new RB-SLO-LATENCY-INVESTIGATION |
| SLO-DEDUP-RATIO                 | §4.8.1       | `RB-SLO-DEDUP-DEGRADATION` (new)                                     | no     | yes    | new RB-SLO-DEDUP-DEGRADATION |
| SLO-CORRECT-CAS                 | §4.9         | `RB-SLO-CORRECT-VIOLATION` (new) + RB-FM-051/062/254                 | partial| yes    | new RB-SLO-CORRECT-VIOLATION |
| SLO-CORRECT-ISO                 | §4.10        | `RB-SLO-CORRECT-VIOLATION` (new) + RB-FM-253/303                     | partial| yes    | new RB-SLO-CORRECT-VIOLATION |
| SLO-FRESH-BILLING               | §4.11        | `RB-BILLING-001` + `RB-FM-302`                                       | yes    | yes    | —                            |
| SLO-FRESH-DSR-ERASURE           | §4.12        | `RB-DSR-ERASURE-INCOMPLETE`                                          | yes    | yes    | —                            |
| SLO-DEPLOY-SAFE                 | §4.13        | `RB-ROLLOUT-STUCK`                                                   | yes    | yes    | —                            |
| SLO-ADMIN-CONFIG-PROPAGATION    | §4.14        | `RB-ADMIN-CONFIG-STALE` (new — referenced in §4.14 but missing)      | no     | yes    | new RB-ADMIN-CONFIG-STALE    |
| SLO-ADMIN-DUAL-APPROVAL-LATENCY | §4.15        | `RB-ADMIN-DUAL-APPROVAL-BREACH` (new)                                | no     | yes    | new RB-ADMIN-DUAL-APPROVAL-BREACH |
| SLO-ADMIN-ROTATION-OVERLAP      | §4.16        | `RB-ADMIN-ROTATION-GAP` (new) + RB-KEY-COMPROMISE                    | partial| yes    | new RB-ADMIN-ROTATION-GAP    |
| SLO-ADMIN-ROLLBACK-RECOVERY     | §4.17        | `RB-ROLLOUT-STUCK`                                                   | partial| yes    | RB-ROLLOUT-STUCK expanded    |
| SLO-RTO-REGION-FAILOVER         | §4.18        | `RB-DR-DRILL` + new `RB-REGION-FAILOVER`                             | partial| yes    | new RB-REGION-FAILOVER       |
| SLO-RPO-REGION                  | §4.19        | `RB-DR-DRILL` + new `RB-REGION-FAILOVER`                             | partial| yes    | new RB-REGION-FAILOVER       |
| SLO-ONCALL-MTTA-SEV1            | §4.20        | `RB-ONCALL-POLICY` (specs/_runbooks/)                                | yes    | yes    | —                            |
| SLO-ONCALL-MTTR-SEV1            | §4.21        | `RB-ONCALL-POLICY` (specs/_runbooks/)                                | yes    | yes    | —                            |

**Before:** 6 yes + 5 partial + 10 no = 6/21 fully covered.
**After:** 21/21 fully covered.

---

## Table B — Alert → Runbook coverage

`observability_model.md §9.3` defines per-SLO multi-burn-rate alerts (4 windows × 21 SLOs = 84 alert instances), plus the discrete alert rules referenced across spec corpus.

### B.1 — Burn-rate alerts (per SLO)

Each SLO has 4 alert windows (`-fast`, `-slow`, `-medium`, `-ticket`). Coverage is **inherited from Table A** (same RB serves all four windows for a given SLO). The two specific alert IDs `RB-SLO-AVAIL-CAS-GET-FAST-BURN` and `RB-SLO-AVAIL-CAS-GET-SLOW-BURN` referenced in `WI-S09-006` are now resolved by `RB-SLO-AVAIL-DATA-PLANE` (anchor sections `§Fast-burn (≤5 min)` and `§Slow-burn (≤30 min)`).

### B.2 — Non-SLO alerts (discrete rules in `observability/alerts/*.yaml`)

| Alert / Rule                                  | Source                              | Runbook                                | Before | After |
|-----------------------------------------------|-------------------------------------|----------------------------------------|--------|-------|
| `corelink_resource_saturation_ratio > 0.8`    | obs §4.3                            | `RB-SLO-AVAIL-DATA-PLANE §Saturation`  | no     | yes   |
| `corelink_cache_hit_ratio` drop > 30% WoW     | obs §4.4                            | `RB-SLO-DEDUP-DEGRADATION` (proxy)     | no     | yes   |
| `corelink_runbook_dry_run_total{overdue}`     | RB-RUNBOOK-DRILL-INDEX §5           | `RB-FM-202`                            | yes    | yes   |
| Cardinality budget breach (`>2× expected`)    | obs §11.2                           | `RB-OBS-CARDINALITY-001`               | yes    | yes   |
| Synthetic page canary fail                    | obs §12 EVT-031                     | `RB-SYNTHETIC-PAGE-DRILL`              | yes    | yes   |
| `corelink_gc_unexpected_delete_total > 0`     | RB-FM-300 detection                 | `RB-FM-300`                            | yes    | yes   |
| `corelink_dsr_resolution_hours > 720`         | SLO-FRESH-DSR-ERASURE               | `RB-DSR-ERASURE-INCOMPLETE`            | yes    | yes   |
| `dev.hugr.corelink.residency.violation.v1`    | obs §7.2 + FM-451                   | `RB-DATA-RESIDENCY-LEAK`               | yes    | yes   |
| `corelink_admin_config_propagation_ms p99 > 5m` | SLO-ADMIN-CONFIG-PROPAGATION      | `RB-ADMIN-CONFIG-STALE` (new)          | no     | yes   |
| `corelink_admin_rollback_recovery_ms p99 > 60s` | SLO-ADMIN-ROLLBACK-RECOVERY        | `RB-ROLLOUT-STUCK`                     | yes    | yes   |
| `oncall_pages` MTTA breach 3 cycles           | SLO-ONCALL-MTTA-SEV1                | `RB-ONCALL-POLICY` + `RB-FM-202`       | yes    | yes   |
| `corelink_billing_event_age_seconds`          | SLO-FRESH-BILLING                   | `RB-FM-302` + `RB-BILLING-001`         | yes    | yes   |
| `corelink_isolation_assertion_total{fail}`    | SLO-CORRECT-ISO                     | `RB-FM-253` + `RB-FM-303` + new `RB-SLO-CORRECT-VIOLATION` | partial | yes |
| `corelink_cas_client_verify_total{mismatch}`  | SLO-CORRECT-CAS                     | `RB-FM-051` + `RB-FM-062` + `RB-FM-254` + new `RB-SLO-CORRECT-VIOLATION` | partial | yes |
| Region failover triggered                     | SLO-RTO/RPO-REGION                  | `RB-REGION-FAILOVER` (new) + `RB-DR-DRILL` | partial | yes |

**Before:** 9 yes + 3 partial + 3 no = 9/15 fully covered.
**After:** 15/15 fully covered.

---

## Table C — P0/P1 Failure Mode → Runbook coverage (23 FMs)

| FM ID  | Class | Runbook                                                              | Before | After  | Notes                              |
|--------|-------|----------------------------------------------------------------------|--------|--------|------------------------------------|
| FM-007 | P1    | `RB-FM-007`                                                          | yes    | yes    | —                                  |
| FM-051 | P1    | `RB-FM-051`                                                          | yes    | yes    | —                                  |
| FM-054 | P1    | `RB-FM-054`                                                          | yes    | yes    | —                                  |
| FM-062 | P1    | `RB-FM-062`                                                          | yes    | yes    | —                                  |
| FM-100 | P1    | `RB-FM-100`                                                          | yes    | yes    | —                                  |
| FM-101 | P1    | `RB-FM-101`                                                          | yes    | yes    | —                                  |
| FM-156 | P1    | `RB-FM-156`                                                          | yes    | yes    | —                                  |
| FM-202 | P1    | `RB-FM-202` (FM-202 stub) — separate RB exists via meta-drill        | partial| yes    | Reference `RB-RUNBOOK-DRILL-INDEX` |
| FM-205 | P1    | `RB-FM-205`                                                          | yes    | yes    | —                                  |
| FM-206 | P1    | `RB-FM-206`                                                          | yes    | yes    | —                                  |
| FM-253 | P1    | `RB-FM-253`                                                          | yes    | yes    | —                                  |
| FM-254 | P1    | `RB-FM-254`                                                          | yes    | yes    | —                                  |
| FM-258 | P1    | `RB-FM-258`                                                          | yes    | yes    | —                                  |
| FM-300 | P1    | `RB-FM-300`                                                          | yes    | yes    | —                                  |
| FM-302 | P1    | `RB-FM-302`                                                          | yes    | yes    | —                                  |
| FM-303 | P1    | `RB-FM-303`                                                          | yes    | yes    | —                                  |
| FM-400 | P1    | `RB-FM-400`                                                          | yes    | yes    | —                                  |
| FM-403 | P1    | `RB-FM-403`                                                          | yes    | yes    | —                                  |
| FM-404 | P1    | `RB-FM-404`                                                          | yes    | yes    | —                                  |
| FM-450 | P1    | `RB-DSR-ERASURE-INCOMPLETE`                                          | partial| yes    | RB exists, FM cross-ref added      |
| FM-451 | **P0**| `RB-DATA-RESIDENCY-LEAK`                                             | yes    | yes    | —                                  |
| FM-452 | P1    | `RB-CONSENT-TAMPERING`                                               | yes    | yes    | —                                  |
| FM-453 | P1    | `RB-SUB-PROCESSOR-BROADCAST-MISS`                                    | yes    | yes    | —                                  |

**FM-202 partial reason:** the existing `RB-FM-202-runbook-stale.md` documents the *concept* of runbook staleness but lacks explicit cross-link to `RB-RUNBOOK-DRILL-INDEX` (the operational implementation in S-17). Resolved by adding a § *Detection & Index* pointer.

**Before:** 19 yes + 2 partial + 0 no = 19/23 fully covered.
**After:** 23/23 fully covered.

---

## Summary

|                                  | Before | After  |
|----------------------------------|--------|--------|
| SLOs with full runbook coverage  | 6/21   | 21/21  |
| Alert rules with full coverage   | 9/15   | 15/15  |
| P0/P1 FMs with full coverage     | 19/23  | 23/23  |
| **Total full-coverage rows**     | **34/59** | **59/59** |

### Runbooks created (8)

1. `RB-SLO-AVAIL-DATA-PLANE` — covers SLO-AVAIL-CAS-GET/PUT, SLO-AVAIL-AC, SLO-AVAIL-EXEC + resolves `RB-SLO-AVAIL-CAS-GET-FAST-BURN` and `-SLOW-BURN` aliases.
2. `RB-SLO-LATENCY-INVESTIGATION` — covers SLO-LAT-CAS-GET/PUT, SLO-LAT-AC-HIT.
3. `RB-SLO-CORRECT-VIOLATION` — covers SLO-CORRECT-CAS, SLO-CORRECT-ISO (zero-budget SEV-1).
4. `RB-SLO-DEDUP-DEGRADATION` — covers SLO-DEDUP-RATIO + cache hit anomaly.
5. `RB-ADMIN-CONFIG-STALE` — covers SLO-ADMIN-CONFIG-PROPAGATION (referenced from `slo_catalog.md §4.14`).
6. `RB-ADMIN-DUAL-APPROVAL-BREACH` — covers SLO-ADMIN-DUAL-APPROVAL-LATENCY.
7. `RB-ADMIN-ROTATION-GAP` — covers SLO-ADMIN-ROTATION-OVERLAP (key continuity, signing outage prevention).
8. `RB-REGION-FAILOVER` — covers SLO-RTO-REGION-FAILOVER + SLO-RPO-REGION operational steps (complements existing `RB-DR-DRILL` which is drill-oriented).

### Runbooks expanded (2)

1. `RB-ROLLOUT-STUCK` — added explicit § *SLO-ADMIN-ROLLBACK-RECOVERY breach handling* (≤60s p99) + § *Escalation path*.
2. `RB-FM-202-runbook-stale.md` — added § *Detection & Index* pointer to `RB-RUNBOOK-DRILL-INDEX`.

### INDEX update

`specs/05_quality/runbooks/INDEX.md` created (none existed) — catalogs all 56 runbooks under canonical path with summaries + cross-refs to SLOs/Alerts/FMs.

---

## Methodology

1. Enumerate SLO IDs from `slo_catalog.md §4.*` headers (21 IDs).
2. Enumerate P0/P1 FM IDs from `failure_modes.md §3.*` rows where `Classe ∈ {P0, P0 (...), P1, P1 (...)}` (23 IDs; 1 P0 + 22 P1).
3. Enumerate alert classes: per-SLO multi-burn (covered via SLO row) + discrete rules grep'd from `observability_model.md §9` and `§12` (15 distinct discrete rules).
4. For each row, search canonical RB directory + cross-source link.
5. Mark `partial` only if a related RB exists AND is missing ≥ 1 of the 7 mandatory sections (Symptom, Detection, Immediate mitigation, Root-cause investigation, Rollback/recovery, Escalation path, Post-incident).
6. Create or expand RB to close every `no` / `partial`.

---

**Fim AUDIT-2026-05-14-RUNBOOK-COVERAGE.**
