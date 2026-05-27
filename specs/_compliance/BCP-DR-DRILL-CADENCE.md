---
id: "BCP-DR-DRILL-CADENCE-2026-05-14"
type: "compliance_cadence"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-14"
updated: "2026-05-15"
sprint: "R-6"
parent_wi: "WT-R6-3"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["bcp", "dr", "drill-cadence", "r-6", "90-day", "soc2-cc7-5", "soc2-cc9-1", "iso-27031", "rb-oncall-policy", "wi-s17-002", "wi-s17-005", "wi-s20-006"]
---

# BCP / DR Drill Cadence — 90-Day Pre-GA Operational Rehearsal

> **doc_status:** DRAFT · **scope:** 90-day pre-GA Business-Continuity-Plan + Disaster-Recovery exercise calendar covering single-region failover, cross-region failover, BYOK CMK rotation, and full SEV1 stakeholder simulations. **Window:** R-6 staging observation gate (T-90 .. T-0 to GA).
>
> **Anchors:** `crates/corelink-ops/src/dr/drill/` (WI-S17-002 scheduler; absorbed Wave 35 P2 from `corelink-dr-drill`), `crates/corelink-ops/src/oncall/` (WI-S17-005 PD schedule; absorbed Wave 35 P2 from `corelink-oncall`), `specs/_runbooks/RB-ONCALL-POLICY.md`, `specs/_runbooks/RB-SYSTEM-CMK-ROTATION.md`, `specs/_runbooks/RB-BYOK-REVOKE.md`, `specs/05_runbooks/RB-region.md`, `specs/03_architecture/failure_modes.md` (75 FMs), `ROADMAP-TO-GA.md` §6 Wave R-6.
>
> **Companion:** `specs/_runbooks/ONCALL-ESCALATION-MATRIX.md` (3-tier escalation), `specs/_compliance/templates/DR-DRILL-EVIDENCE.md` (auditor-ready evidence form).
>
> **SOC 2 control coverage:** CC7.5 (recovery from disruptions — drills + evidence) + CC9.1 (risk identification + mitigation testing) + Availability series A1.2/A1.3 (environmental + capacity recovery tests).
>
> **ISO 27031 alignment:** §8.4 (ICT readiness tests), §9.1 (review & improvement).

## 1. Cadence overview

The cadence ramps from individual-failure recovery drills to multi-stakeholder full-scope incident rehearsals over 90 days.

| Phase | Weeks | Cadence | Drill class | Stakeholders |
|---|---|---|---|---|
| **P1 — Single-region failover** | W1–W4 | Weekly (Wed 14:00 UTC) | Single primitive recovery | SRE primary on-call + scribe |
| **P2 — Cross-region + BYOK rotation** | W5–W8 | Weekly (Wed 14:00 UTC) | Multi-region + key-mgmt | SRE + Security on-call + Customer Success notify |
| **P3 — Full SEV1 simulation** | W9–W12 | Bi-weekly (Wed 14:00 UTC) | All-hands incident rehearsal | All tiers (L1+L2+L3) + Legal + Comms + 1 customer observer |

**Total drills specified: 16** (4 + 4 + 2 + 6 cross-cutting → see §6 wrap-up & retest slots; DR-15 added 2026-05-15 to close GAP-15; DR-16 added 2026-05-15 for active-region warm failover, complement to DR-15 cold-restore).

> **Why Wednesday 14:00 UTC?** Maximises overlap of US-East + EU-West rotations per `RB-ONCALL-POLICY.md` §8. Avoids Mon (deploy day), Fri (recovery exhaustion), and weekend (off-rotation pages).

### Cadence enforcement

`corelink-dr-drill` (WI-S17-002) ships `DrillCadence::Semestral` for steady-state post-GA (Jan 1 + Jul 1 06:00 UTC, cron `0 6 1 1,7 *`). For this 90-day pre-GA window, a **separate one-shot cadence overlay** drives the schedule:

- Cron expression: `0 14 * * 3` (Wednesday 14:00 UTC) — gated by `DrillEnv::Staging` only.
- Production environment gate (`require_staging`) MUST reject any prod-mode drill attempt with `DrillError::ProdEnvForbidden`.
- Evidence emitted to `specs/_audits/2026-MM-DD-bcp-drill-<DRILL-ID>.md` using the template in §6.

## 2. Phase 1 — Single-region failover drills (W1–W4)

Goal: validate isolated-primitive recovery without crossing region boundary. Each drill exercises **one FM at a time** to attribute failure cleanly.

### Drill DR-001 — Worker isolate kill (FM-001)

- **ID:** DR-001
- **Week:** W1, Wed
- **Scope:** Force-kill 30% of Worker isolates in `staging-us-east-1` mid-request; verify CF runtime spin-up + client retry recovers traffic within RTO.
- **Prerequisites:** Staging green for 72h; chaos-mesh helm pre-installed; SRE primary acknowledged drill window in PD; status page pre-warmed with maintenance note.
- **Success criteria:** RTO ≤ 60s (P99 user-facing latency recovered); RPO = 0 (stateless workers — no data loss); error rate spike resolves < 5 min.
- **Runbook:** `specs/05_runbooks/RB-region.md` §3.1 (worker-tier section).
- **FM-IDs touched:** FM-001, FM-002, FM-006.
- **Evidence captured:** Grafana panels (worker_isolate_kills, p99_latency, error_rate_5xx); D1 `dr_drill_runs` row; PD timeline export; postmortem-lite `specs/_audits/2026-MM-DD-bcp-drill-DR-001.md`.

### Drill DR-002 — Container deadline hit (FM-004)

- **ID:** DR-002
- **Week:** W2, Wed
- **Scope:** Inject 65-min sleep into 5% of `execute-action` container jobs to trip the 60-min deadline guard; verify CTRL-EXEC-001 timeout fires + user notification queued + retry-skip enforced.
- **Prerequisites:** Staging green; chaos sidecar deployed; `execute_action_timeouts_total` baseline captured for the prior 24h.
- **Success criteria:** 100% of injected jobs killed within 60min + 30s grace; user-notification webhook delivered within 90s; zero idempotency-key reuse.
- **Runbook:** `specs/_runbooks/RB-FM-SIGNUP-FAILED.md` (template pattern) + container-specific RB at `specs/05_runbooks/RB-region.md` §3.2.
- **FM-IDs touched:** FM-003, FM-004.
- **Evidence captured:** chaos-mesh report JSON; D1 `execute_action_jobs` rows in `Aborted` state; user-notification audit ledger entries; postmortem-lite.

### Drill DR-003 — DO actor rebalance (FM-005) + KV stale read (FM-054)

- **ID:** DR-003
- **Week:** W3, Wed
- **Scope:** Force DO actor migration on `tenant_cache_actor` for 20% of tenants; concurrently inject 90s KV propagation lag via fault sidecar. Verify p99 spike stays ≤ 2× baseline + PAT-KV-TTL-001 + PAT-DEGRADE-001 hold.
- **Prerequisites:** Staging green; tenant cache pre-warmed; KV namespace fault-inject permission active.
- **Success criteria:** p99 ≤ 2× baseline within 120s; zero stale-write semantics violation (D1 sequence numbers monotonic); KV staleness alert fires + recovers.
- **Runbook:** `specs/05_runbooks/RB-region.md` §3.3 (DO + KV section).
- **FM-IDs touched:** FM-005, FM-054, FM-055.
- **Evidence captured:** DO migration log; KV staleness gauge; D1 sequence audit; postmortem-lite.

### Drill DR-004 — R2 bucket partial outage (FM-050)

- **ID:** DR-004
- **Week:** W4, Wed
- **Scope:** Mark `staging-cas-us-east` R2 bucket as 503 for read traffic via WAF rule; verify CAS reads degrade to the in-region replica + PAT-REGION-FAILOVER-001 sibling-bucket route activates within RTO.
- **Prerequisites:** Replica bucket in same region populated to ≥ 99.9% parity; SBOM-pinned client lib with retry/backoff.
- **Success criteria:** RTO ≤ 300s for read traffic; zero write loss (writes block + queue, never silently drop); D1 `cas_object_locator` rows reflect replica origin within 120s.
- **Runbook:** `specs/_runbooks/RB-BACKUP-VERIFICATION.md` + `specs/05_runbooks/RB-region.md` §4.
- **FM-IDs touched:** FM-050, FM-052, FM-053.
- **Evidence captured:** R2 access logs; client retry histogram; backup verification report; postmortem-lite.

## 3. Phase 2 — Cross-region failover + BYOK CMK rotation (W5–W8)

Goal: validate multi-region orchestration + key-material rotation under load. Drills cross the region boundary and exercise the `corelink-failover-router::ResidencyGraph`.

### Drill DR-005 — Cross-region CAS failover (FM-050 + FM-101 sub-scope)

- **ID:** DR-005
- **Week:** W5, Wed
- **Scope:** Take `staging-cas-us-east-1` fully offline (WAF block + DNS pin override); route all US-East tenant traffic to `staging-cas-us-west-2` via residency-graph sibling routing. Measure RTO/RPO against `corelink-dr-drill::RTO_CEIL_SECONDS` (1800s) + `RPO_CEIL_SECONDS` (60s).
- **Prerequisites:** US-West replica in sync (RPO budget < 30s sustained 24h); residency graph reviewed by privacy lead (no EU data crosses to US-West illegally); customer-comms template pre-drafted (P3-COMMS-TPL-001).
- **Success criteria:** RTO ≤ 1800s; RPO ≤ 60s; zero EU-residency-tagged object reads served from US-West; comms template sent to status page (staging variant) within 5min of "incident" declaration.
- **Runbook:** `specs/05_runbooks/RB-region.md` §5 (cross-region) + `crates/corelink-failover-router` README.
- **FM-IDs touched:** FM-050, FM-052, FM-101, FM-105.
- **Evidence captured:** RTO/RPO measurement from `DrillRun::rto_seconds`/`rpo_seconds`; residency-tag audit report (zero violations); status-page screenshot; D1 `dr_drill_runs` row; postmortem-lite.

### Drill DR-006 — D1 primary loss (FM-055 + FM-152)

- **ID:** DR-006
- **Week:** W6, Wed
- **Scope:** Trigger `DrillCycle::D1PrimaryLoss` (per `corelink-dr-drill::schedule`) in staging US-East D1; verify Neon failover (FM-057 30-90s window) + read-only mode activation + session-consistency invariant (PAT-SESSION-CONSISTENCY-001) holds.
- **Prerequisites:** D1 replica lag < 5s sustained 24h; read-only mode toggle wired to feature flag; customer-comms template "Read-only maintenance window — writes paused 90s" prepped.
- **Success criteria:** Primary loss detected within 30s; read-only mode auto-activates < 60s; writes resume within 120s of failover complete; zero session-consistency violation (PAT-SESSION-CONSISTENCY-001 holds on replayed audit log; INV-DATA-MONOTONIC-TS checker passes).
- **Runbook:** `specs/_runbooks/RB-D1-MIGRATION-APPLY.md` + `specs/05_runbooks/RB-region.md` §6.
- **FM-IDs touched:** FM-055, FM-056, FM-057, FM-058, FM-152.
- **Evidence captured:** D1 failover audit; TLA+ checker output; feature-flag transition log; session-consistency invariant report; postmortem-lite.

### Drill DR-007 — BYOK CMK rotation (FM-204)

- **ID:** DR-007
- **Week:** W7, Wed
- **Scope:** Execute full BYOK CMK rotation per `RB-SYSTEM-CMK-ROTATION.md` for one synthetic tenant in staging; verify overlap-period decryption (PAT-ROLL-FORWARD-001) + CTRL-KEY-005 + CTRL-KEY-006 + CTRL-CRED-003 all hold; no service interruption.
- **Prerequisites:** Synthetic tenant `byok-drill-tenant-007` provisioned with full envelope-encrypted dataset; AWS KMS / GCP KMS / Azure Key Vault test endpoints reachable; rotation runbook last drilled < 90d ago.
- **Success criteria:** Zero decryption errors during 24h overlap window; new CMK fully active; old CMK marked `pending-destruction` (NIST SP 800-57 §5.3.5); audit ledger has full rotation event chain.
- **Runbook:** `specs/_runbooks/RB-SYSTEM-CMK-ROTATION.md` + `specs/_runbooks/RB-BYOK-REVOKE.md` (revoke-adjacent procedures).
- **FM-IDs touched:** FM-204; also exercises CTRL-KEY-005/006 + CTRL-CRED-003.
- **Evidence captured:** KMS rotation audit log; envelope-decrypt-success metric over 24h; CMK key-state transition timeline; postmortem-lite.

### Drill DR-008 — BYOK CMK compromise simulation (DrillCycle::ByokKeyCompromise)

- **ID:** DR-008
- **Week:** W8, Wed
- **Scope:** Simulate customer-reported BYOK CMK compromise; trigger emergency revoke path per `RB-BYOK-REVOKE.md`; verify tenant data becomes inaccessible within RTO + customer-comms ack within SLA.
- **Prerequisites:** Synthetic tenant with full BYOK envelope; security on-call available; legal-on-call notified (this drill activates contractual breach-comms language).
- **Success criteria:** CMK marked revoked within 300s of "compromise report" timestamp; all in-flight read/write to tenant data returns `403 byok_revoked` within 600s; customer comms email staged for review within 30min; SOC 2 CC9.1 evidence captured (risk-identification → mitigation → re-test cycle).
- **Runbook:** `specs/_runbooks/RB-BYOK-REVOKE.md` (full revoke path).
- **FM-IDs touched:** FM-204, FM-253 (cross-tenant adjacency), FM-258 (insider-adjacent).
- **Evidence captured:** Revoke audit chain; tenant-data-access denial proof; legal-comms draft; postmortem-lite.

## 4. Phase 3 — Full SEV1 incident simulations (W9–W12)

Goal: rehearse full multi-stakeholder incident response. Bi-weekly cadence (W9, W11) gives 2-week postmortem windows. Each simulation MUST involve all 3 escalation tiers (L1 / L2 / L3) per `ONCALL-ESCALATION-MATRIX.md`.

### Drill DR-009 — SEV1 cross-tenant data leak rehearsal (FM-253)

- **ID:** DR-009
- **Week:** W9, Wed
- **Scope:** Inject synthetic "cross-tenant read" alert via the cross-tenant isolation TLA+ invariant monitor; trigger full SEV1 — L1 ack → L2 page → L3 page → legal-on-call → status page → customer comms → 24h postmortem.
- **Prerequisites:** Synthetic alert wired (does NOT touch real tenant data); legal-on-call pre-briefed (drill, not real); CTO + compliance lead + privacy lead on calendar; 1 lighthouse customer observer invited.
- **Success criteria:** L1 ack ≤ 5min; L2 page-up ≤ 10min; L3 page-up ≤ 15min; legal joins ≤ 30min; status page updated ≤ 60min; customer comms email staged ≤ 90min; postmortem doc seeded within 24h.
- **Runbook:** `specs/_runbooks/RB-POSTMORTEM-PROCESS.md` + `specs/_runbooks/RB-TABLETOP-TEMPLATE.md` + `specs/_runbooks/RB-LIGHTHOUSE-CUSTOMER-INCIDENT.md`.
- **FM-IDs touched:** FM-253, FM-254, FM-258 (insider-adjacent + cache-poisoning adjacency).
- **Evidence captured:** PD timeline (L1→L2→L3 ack stamps); legal-on-call ack log; status-page version diff; customer-comms email staged version; tabletop notes; 24h postmortem.

### Drill DR-010 — SEV1 supply-chain compromise rehearsal (FM-156)

- **ID:** DR-010
- **Week:** W11, Wed
- **Scope:** Simulate "malicious crate yank + SLSA L3 attestation gap" finding; trigger SEV1 — freeze deploy pipeline, rotate all build secrets (CTRL-CRED-003 + CTRL-KEY-005), audit prior 90d artifacts for compromise.
- **Prerequisites:** Test SBOM with planted "compromise" marker; build secrets rotation runbook readiness verified < 14d ago; CTO on calendar.
- **Success criteria:** Deploy freeze applied ≤ 15min of declaration; build secrets rotated ≤ 2h; SLSA attestation re-verification report within 6h; 90d artifact scan report within 24h; 24h postmortem.
- **Runbook:** `specs/_runbooks/RB-PENTEST-FINDING-RESPONSE.md` + `specs/_runbooks/RB-SECURITY-VULNERABILITY-INTAKE.md`.
- **FM-IDs touched:** FM-154, FM-155, FM-156.
- **Evidence captured:** Deploy-freeze audit; secrets-rotation chain; SLSA attestation diff; 90d SBOM scan output; postmortem.

## 5. Cross-cutting + retest slots (interleaved through W1–W12)

### Drill DR-011 — Runbook drift detection (FM-202) — W3+W9 (twice)

- **ID:** DR-011 (twice — DR-011a W3, DR-011b W9)
- **Scope:** Pick 2 random P0/P1 runbooks from `RB-RUNBOOK-DRILL-INDEX.md`; execute the runbook from cold against a synthetic incident; flag every step that is stale, ambiguous, or missing.
- **Prerequisites:** Runbook drill index up to date; random-pick seed logged for audit non-repudiation.
- **Success criteria:** Each runbook completes within its documented time budget ± 25%; any drift > 25% files a runbook-update WI within 7d (FM-202 mitigation closure).
- **Runbook:** `specs/05_runbooks/RB-RUNBOOK-DRILL-INDEX.md` + the random-pick target runbook.
- **FM-IDs touched:** FM-202 (primary), plus whichever FMs the random runbook covers.
- **Evidence captured:** Random-pick audit (seed + result); time-per-step log; drift report; runbook-update WI link (if any).

### Drill DR-012 — Synthetic page weekly dry-run (FM-203) — recurring W2/W5/W8/W11

- **ID:** DR-012 (recurring — DR-012a..d, one per phase mid-point)
- **Scope:** Page the on-call primary at an unannounced moment within the drill week; measure ack latency, page comprehension (does primary identify the synthetic vs real?), correct runbook selection.
- **Prerequisites:** Synthetic page route distinct from production route (per spec contract residency split); primary fatigue score from `DASH-ONCALL-FATIGUE` < 70.
- **Success criteria:** Ack ≤ 5min; synthetic-vs-real identification 100%; correct runbook URL clicked ≥ 95%.
- **Runbook:** `specs/_runbooks/RB-SYNTHETIC-PAGE-DRILL.md`.
- **FM-IDs touched:** FM-202, FM-203.
- **Evidence captured:** PD ack stamps; runbook-URL click telemetry; fatigue-score snapshot.

### Drill DR-013 — Backup restore verification (CC9.1 + CC7.5) — W4+W10

- **ID:** DR-013 (twice — DR-013a W4, DR-013b W10)
- **Scope:** Restore a full snapshot of staging D1 + R2 CAS to an isolated environment; verify integrity (BLAKE3 + INV-AUDIT-APPEND-ONLY) over the full audit chain.
- **Prerequisites:** Snapshot retention pipeline green; isolated restore environment provisioned; integrity-check harness in CI green.
- **Success criteria:** Restore completes within documented RTO; BLAKE3 verification 100% pass; audit chain checker (TLA+ + linear scan) finds zero gaps.
- **Runbook:** `specs/_runbooks/RB-BACKUP-VERIFICATION.md`.
- **FM-IDs touched:** FM-051, FM-061, FM-062.
- **Evidence captured:** Restore time; BLAKE3 pass rate; audit-chain checker report; snapshot-id pinned in evidence doc.

### Drill DR-015 — Cold restore from zero (GAP-15; FM-105 full-interruption) — W10 + post-GA quarterly

- **ID:** DR-15 (cycle suffix `-a` for first dry-run, `-b` for first staging, then quarterly `-c`..)
- **Week:** W10, Wed (paired with DR-013b backup-verification W10 so cold-restore consumes a freshly-verified manifest)
- **Scope:** Simulate **total loss** of one Cloudflare region (R2 + D1 + KV + DO all destroyed) and execute end-to-end cold restore per `specs/_runbooks/RB-COLD-RESTORE-FROM-ZERO.md`. Closes GAP-15 (A1.2 + A1.3 evidence gap from `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md`). RTO targets: read-path ≤ 4h, write-path ≤ 8h. RPO target: ≤ 15 min.
- **Prerequisites:** DR-013 monthly warm-verification ran < 30d ago (proves backup artifacts decryptable); BYOK CMK accessible from surviving region; synthetic drill tenant `cold-restore-drill-tenant` fixture refreshed at T-24h; staging admin-write freeze pre-applied; war-room + IC + scribe + BYOK operator + compliance reviewer all confirmed on PD.
- **Success criteria:** All 7 criteria from `COLD-RESTORE-DRILL-SPEC.md` §7 — RTO read ≤ 4h; RTO write ≤ 8h; RPO ≤ 15min; audit-chain Merkle root continuity; BYOK envelope DEK SHA-256 match; SLO histograms recovered ≤ 15min post-step-8; zero production-tenant impact.
- **Runbook:** `specs/_runbooks/RB-COLD-RESTORE-FROM-ZERO.md` (9 steps).
- **Orchestrator:** `scripts/cold-restore-drill.sh --staging`.
- **Verification gate:** `scripts/verify-cold-restore.py`.
- **FM-IDs touched:** FM-050, FM-051, FM-052, FM-055, FM-061, FM-062, FM-101, FM-105, FM-204.
- **Evidence captured:** Drill log; verification gate JSON output; Grafana baseline + post-restore PDFs; Drata upload receipt; postmortem-lite if PARTIAL/FAIL; evidence doc at `specs/_compliance/drill-evidence/YYYY-QQ-cold-restore-{dry-run|staging}.md`.
- **Cadence:** one mandatory `--dry-run` before GA staging cut (T-30d); one mandatory `--staging` execution before GA tag; quarterly post-GA (`--staging`, first Wed of Jan / Apr / Jul / Oct).

### Drill DR-016 — Active-region warm failover (FM-101 partial-outage variant) — W8 + monthly post-GA

- **ID:** DR-16 (cycle suffix `-a` for first simulate, `-b` for first staging, then monthly `-c`..)
- **Week:** W8, Wed (paired with DR-008 BYOK-compromise to exercise cross-stack incident realism)
- **Scope:** Simulate a **degraded** primary region (p95 > 5s sustained OR error rate > 5% sustained, but **not** zero) and flip the active write lease to the sibling per `ResidencyGraph` (WNAM↔ENAM, WEUR↔SAM) per `specs/_runbooks/RB-ACTIVE-FAILOVER.md` (7 steps). Complements DR-15 cold-restore (DR-15 = total destruction → rebuild; DR-16 = degraded → warm switch). RTO target: ≤ 15 min (write-lease flip). RPO target: ≤ 5 min. Failback (Reverse step) RTO: ≤ 30 min.
- **Prerequisites:** `crates/corelink-failover-router` deployed at current `main` SHA; `tests/e2e-failover-router` green in last 24h (5 scenarios); cross-region replication lag p99 ≤ 60s sustained 24h; sibling region Healthy at T-2h; DNS TTL on failover hostnames ≤ 60s; Privacy Lead review of residency graph < 30d.
- **Success criteria:** All 8 criteria from `ACTIVE-FAILOVER-DRILL-SPEC.md` §7 — RTO write-flip ≤ 15min; RPO ≤ 5min; failover overhead p99 ≤ 50ms; audit-chain Merkle continuity; **zero split-brain** (no overlapping writes accepted by both regions); residency invariants preserved; failback ≤ 30min; zero production-tenant impact.
- **Runbook:** `specs/_runbooks/RB-ACTIVE-FAILOVER.md` (7 steps: Detect → Decide → Drain → Promote → Reroute → Verify → Reverse).
- **Orchestrator:** `scripts/active-failover-drill.sh --staging` (3 modes: `--simulate` / `--staging` / `--prod`).
- **E2E harness:** `tests/e2e-failover-router/` — 5 scenarios (primary-up / primary-degraded / split-brain-prevention / failback-after-recovery / partial-region).
- **FM-IDs touched:** FM-050, FM-052, FM-101, FM-105 (partial-outage variant; complement to DR-15's full-interruption variant).
- **Evidence captured:** Drill log (`drill-active-failover-YYYY-MM-DD-HH-MM.log`); Grafana baseline + post-flip PDFs; split-brain audit dump (DO write-lease log); Drata upload receipt; postmortem-lite if PARTIAL/FAIL; evidence doc at `specs/_compliance/drill-evidence/YYYY-MM-active-failover-{simulate|staging}.md`.
- **Cadence:** weekly `--simulate` in CI (cron `0 14 * * 3`); one mandatory `--staging` before GA tag; monthly `--staging` post-GA (first Wed of every month, 14:00 UTC).

### Drill DR-014 — Terraform drift detection (FM-206) — W6

- **ID:** DR-014
- **Week:** W6, Wed (paired same-day with DR-006 for SRE cognitive-load realism)
- **Scope:** Manually mutate one infra primitive in CF dashboard (e.g. add a stray WAF rule); verify the daily 03:00 UTC `WI-S13-004` drift cron detects + pages within 24h + audit log captures the drift.
- **Prerequisites:** Drift cron last green run < 24h ago; SEV-3 PD route validated.
- **Success criteria:** Drift detected within 24h; SEV-3 page sent; audit log row written; manual-gate approval flow exercised end-to-end; planted drift remediated within 48h.
- **Runbook:** `specs/_runbooks/RB-DRATA-SYNC-FAILURE.md` (drift-adjacent pattern).
- **FM-IDs touched:** FM-206, FM-201.
- **Evidence captured:** Drift cron output; PD page; audit log row; manual-gate approval ledger entry.

## 6. Evidence + closure

Each drill MUST emit one evidence doc per run using the template at `specs/_compliance/templates/DR-DRILL-EVIDENCE.md`. Evidence file path:

```
specs/_audits/2026-MM-DD-bcp-drill-<DRILL-ID>-<cycle>.md
```

`cycle` = `a`/`b`/`c`/`d` for recurring drills (DR-011, DR-012, DR-013).

**Closure gate for R-6:** All 14 drill specs above MUST have ≥ 1 successful evidence doc in `specs/_audits/` before the R-6 → R-7 gate tag (`roadmap-r6-sealed`). A drill MAY be retried up to 2× within its phase; a 3rd failure escalates to the `R6-3 Incident response if SEV-1` agent path per `ROADMAP-TO-GA.md` §6.

## 7. FM coverage matrix

| FM-ID | Covered by drills |
|---|---|
| FM-001 | DR-001 |
| FM-002 | DR-001 |
| FM-003 | DR-002 |
| FM-004 | DR-002 |
| FM-005 | DR-003 |
| FM-006 | DR-001 |
| FM-050 | DR-004, DR-005 |
| FM-051 | DR-013 |
| FM-052 | DR-004, DR-005 |
| FM-053 | DR-004 |
| FM-054 | DR-003 |
| FM-055 | DR-003, DR-006 |
| FM-056 | DR-006 |
| FM-057 | DR-006 |
| FM-058 | DR-006 |
| FM-061 | DR-013 |
| FM-062 | DR-013 |
| FM-101 | DR-005 |
| FM-105 | DR-005 |
| FM-152 | DR-006 |
| FM-154 | DR-010 |
| FM-155 | DR-010 |
| FM-156 | DR-010 |
| FM-201 | DR-014 |
| FM-202 | DR-011, DR-012 |
| FM-203 | DR-012 |
| FM-204 | DR-007, DR-008 |
| FM-206 | DR-014 |
| FM-253 | DR-008, DR-009 |
| FM-254 | DR-009 |
| FM-258 | DR-008, DR-009 |
| FM-105 | DR-005, **DR-015 (cold restore — full-interruption variant)**, **DR-016 (active failover — partial-outage variant)** |
| FM-204 | DR-007, DR-008, **DR-015 (cold-restore envelope re-bind verification)** |
| FM-101 | DR-005, **DR-016 (warm switch + split-brain prevention)** |

**FMs NOT covered by R-6 cadence (deferred to post-GA semestral cadence per `corelink-dr-drill::DrillCadence::Semestral`):** FM-007 (RCE — fuzz harness coverage), FM-100/102/103 (DNS / BGP / TLS — CF-managed), FM-150/151/153 (third-party APIs — vendor-managed), FM-205 (manual-delete — PAT-DUAL-APPROVAL-001 covers), FM-250..257 (edge security — pentest + WAF).

## 8. Roles + RACI

| Role | DR-001..004 (P1) | DR-005..008 (P2) | DR-009..010 (P3) | DR-011..014 (X) |
|---|---|---|---|---|
| SRE primary on-call (L1) | R | R | R | R |
| SRE secondary on-call | C | A | A | C |
| Eng manager (L2) | C | C | R | C |
| CTO (L3) | I | I | R | I |
| Security on-call | I | C | R | I |
| Legal on-call | — | I (DR-008 only) | R (DR-009) | — |
| Privacy lead | I | C (DR-005, DR-008) | R (DR-009) | — |
| Customer success | — | I (DR-008) | C (DR-009) | — |
| 1 lighthouse-customer observer | — | — | I (DR-009 only) | — |

R = responsible · A = accountable · C = consulted · I = informed · — = not involved.

## 9. Cancellation + reschedule rules

A drill MAY be cancelled or rescheduled by the SRE primary on-call only if:

1. A real SEV1 or SEV2 is active or unresolved < 24h before drill window; **OR**
2. Staging is non-green (any blocker-tier alert active) at T-2h; **OR**
3. Drill prerequisites materially unmet (e.g. replica lag > RPO budget for DR-005/006).

Cancellation MUST be logged in the evidence doc (`status: cancelled`, reason cited) and rescheduled to the next-available Wednesday within 14 days. A second cancellation triggers escalation to the eng manager (L2) per `ONCALL-ESCALATION-MATRIX.md` §6.

## 10. References

- `crates/corelink-ops/src/dr/drill/` — drill scheduler invariants + RTO/RPO ceilings (absorbed Wave 35 P2 from `corelink-dr-drill`)
- `crates/corelink-ops/src/oncall/` — PD schedule + fatigue tracking (absorbed Wave 35 P2 from `corelink-oncall`)
- `crates/corelink-failover-router/` — residency graph for cross-region routing
- `specs/_compliance/ACTIVE-FAILOVER-DRILL-SPEC.md` — DR-16 warm-failover drill spec
- `specs/_compliance/COLD-RESTORE-DRILL-SPEC.md` — DR-15 cold-restore drill spec
- `specs/_runbooks/RB-ACTIVE-FAILOVER.md` — 7-step active-failover runbook
- `scripts/active-failover-drill.sh` — DR-16 3-mode orchestrator
- `tests/e2e-failover-router/` — DR-16 5-scenario E2E harness
- `specs/03_architecture/failure_modes.md` — FM-table (75 FMs)
- `specs/_runbooks/RB-ONCALL-POLICY.md` — rotation + fatigue policy
- `specs/_runbooks/RB-POSTMORTEM-PROCESS.md` — postmortem template
- `specs/_runbooks/RB-TABLETOP-TEMPLATE.md` — tabletop exercise frame
- `specs/_runbooks/RB-SYSTEM-CMK-ROTATION.md` — CMK rotation procedure
- `specs/_runbooks/RB-BYOK-REVOKE.md` — BYOK emergency revoke
- `specs/_runbooks/RB-BACKUP-VERIFICATION.md` — backup restore + integrity
- `specs/_runbooks/RB-SYNTHETIC-PAGE-DRILL.md` — synthetic page procedure
- `specs/05_runbooks/RB-RUNBOOK-DRILL-INDEX.md` — P0/P1 runbook drill catalog
- `specs/05_runbooks/RB-region.md` — regional runbook
- `specs/_runbooks/ONCALL-ESCALATION-MATRIX.md` — 3-tier escalation matrix (companion doc)
- `specs/_compliance/templates/DR-DRILL-EVIDENCE.md` — evidence template
- `specs/_compliance/SOC2-GAP-ANALYSIS.md` — controls coverage
- `ROADMAP-TO-GA.md` §6 — R-6 Wave context
- `specs/_runbooks/RB-ENDURANCE-24H-DRILL.md` — **companion 24h endurance load drill** (NOT a BCP/DR drill itself; load-test exercise interleaved with this cadence to avoid contention. The endurance runbook explicitly checks this calendar before scheduling and pauses synthetic-page during its 24h window.)
