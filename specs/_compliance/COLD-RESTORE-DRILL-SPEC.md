---
id: "COLD-RESTORE-DRILL-SPEC-2026-05-15"
type: "compliance_drill_spec"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.1.0"
created: "2026-05-15"
updated: "2026-05-27"
sprint: "R-6"
parent_wi: "GAP-15"
owner: "SRE Lead"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["compliance", "dr", "cold-restore", "gap-15", "soc2-cc7-5", "soc2-cc9-1", "soc2-a1-2", "soc2-a1-3", "iso-27031", "rb-cold-restore", "dr-15", "r-6"]
---

> **Post Wave 35 Phase 2 update 2026-05-27:** `corelink-dr-drill` and `corelink-drata-sync` were absorbed into `corelink-ops` via inline `mod <name>;` per SEAL specs/_audits/sealed/2026-05-26-w35-p2-ops-absorption.md. Canonical consumer paths are now `corelink_ops::dr::drill::*` and `corelink_ops::drata::*`. The legacy `crates/corelink-dr-drill/` and `crates/corelink-drata-sync/` directories no longer exist; the wrangler worker / CLI binary names may persist post-absorption.

# Cold Restore Drill Spec — Zero-Infrastructure Region Recovery

> **doc_status:** DRAFT · **scope:** simulate complete loss of one Cloudflare region (R2 + D1 + KV + Durable Objects all destroyed, not merely degraded) and validate end-to-end restore from cross-region encrypted backups to fresh infrastructure. Closes **GAP-15** (A1.2 + A1.3 evidence gap) flagged in `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md` §3.4 row 4.
>
> **Anchors:** `scripts/cold-restore-drill.sh` (orchestrator), `scripts/verify-cold-restore.py` (post-drill gate), `specs/_runbooks/RB-COLD-RESTORE-FROM-ZERO.md` (9-step runbook), `scripts/restore-from-snapshot.sh` (low-level restore primitive), `scripts/backup-daily.sh` (backup primitive).
>
> **Companion:** `specs/_compliance/BCP-DR-DRILL-CADENCE.md` DR-13 (warm backup verification, monthly) vs **DR-15 (this cold restore, quarterly)** — DR-13 verifies artifacts are decryptable + integrity-checked against an ephemeral instance; DR-15 simulates that the **entire source region is destroyed** and we must rebuild from N-1.
>
> **SOC 2 control coverage:** A1.2 (environmental protections + backups + DR — quarterly restore test), A1.3 (recovery testing operating-effectiveness for Type II), CC7.5 (recovery from disruptions), CC9.1 (risk identification + mitigation testing).
>
> **ISO 27031 alignment:** §8.4 (ICT readiness tests — full DR scenario), §9.1 (review + improvement of recovery procedures).
>
> **NIST SP 800-34 alignment:** §3.4.3 full-interruption testing (highest tier of contingency testing).

## 1. Scope + non-goals

### 1.1 In scope (what this drill simulates)

- **Total loss** of one Cloudflare region (e.g. `us-east-1` enterprise zone). Definition of "total loss":
  - R2 bucket `corelink-cold-us-east-1` destroyed (all objects unrecoverable from origin).
  - All D1 databases bound to that region (`corelink_core`, `corelink_audit`, `corelink_billing`) lost — including replicas in that region.
  - All KV namespaces bound to that region (`CORELINK_KV`, `CORELINK_FEATURE_FLAGS`) wiped.
  - All Durable Object actors hosted in that region (per `crates/corelink-region` colo allocation) annihilated; state in `tenant_cache_actor`, `config_do`, `byok_vault_do` lost.
- **Recovery target:** spin up new region (logical equivalent — same residency tag, possibly different physical CF colo) from zero infrastructure using only cross-region encrypted backups + BYOK CMKs that still live in the surviving region's KMS.

### 1.2 Out of scope (handled by adjacent drills)

- Single-primitive recovery (covered by `BCP-DR-DRILL-CADENCE.md` DR-001..004).
- Cross-region warm failover where the primary is degraded but reachable (covered by DR-005).
- BYOK CMK rotation / compromise (covered by DR-007, DR-008).
- Backup artifact decryptability (covered by DR-013 monthly — this drill assumes DR-013 has run in last 30d).
- Customer-comms tabletop with real customers (drill is **staging-only**; no real customer involvement).

### 1.3 Threat-model assumption (which catastrophe forces a cold restore)

Cold restore is invoked when:

1. Cloudflare regional control plane returns hard-error (`region_destroyed: true`) for ≥ 4h with no ETA, **AND**
2. The residency-graph router (`crates/corelink-failover-router`) has exhausted all sibling-bucket routes in the same residency zone, **AND**
3. Cross-region warm failover (DR-005 path) is impossible because the failed region's primary state was source-of-truth for the affected residency partition.

This is **not** a partial outage. It is "the data center burned down" or equivalent.

## 2. Prerequisites (must be true before drill is even scheduled)

### 2.1 Standing prerequisites (validated by `--dry-run` mode of `cold-restore-drill.sh`)

- [ ] Cross-region replication active: `scripts/backup-daily.sh` has emitted a successful manifest into `corelink-backups-{env}` bucket in the **surviving** region within the last 24h.
- [ ] N-1 region backup retention: most recent 7 manifests present + decryptable (verified by latest DR-013 run within 30d).
- [ ] BYOK CMK accessibility: surviving region's KMS endpoint (AWS / GCP / Azure / Vault) reachable; envelope-unwrap returns < 5s P99 over last 24h.
- [ ] R2 backup-bucket Object Lock Governance retention ≥ 7d for daily artifacts (per `failure_modes.md` FM-061 mitigation).
- [ ] Audit-chain Merkle root recorded daily into surviving-region D1 (`audit_chain_checkpoints` table) — at least one entry within last 24h.
- [ ] Synthetic drill tenant `cold-restore-drill-tenant` provisioned in pre-drill window with deterministic fixture data (3 CAS blobs, 5 audit events, 1 BYOK envelope).
- [ ] `corelink-drata-sync` CLI binary built + smoke-tested (drill log will be uploaded post-completion).

### 2.2 Per-drill prerequisites (24h before scheduled drill)

- [ ] Customer-comms posted: status-page banner "Staging-only DR test on YYYY-MM-DD HH:MM UTC; no production impact expected" — issued 24h ahead.
- [ ] Admin-plane write-freeze applied to staging: feature flag `staging_admin_writes_frozen=true` set; admin-API writes will return `503 maintenance` for the drill window.
- [ ] Baseline-metrics snapshot captured: Grafana dashboards exported (PDF + JSON) at T-24h covering RTO/RPO panels, audit-chain throughput, BYOK envelope-unwrap latency, SLO burn-rate panels.
- [ ] IC + scribe + L2/L3 backup engineers identified + acknowledged on the PagerDuty drill rotation.
- [ ] War-room channel pre-created: `#drill-cold-restore-YYYY-MM-DD` Slack channel with bookmarks to runbook + dashboards + this spec.
- [ ] Synthetic drill tenant's fixture state hashed + stored: `verify-cold-restore.py` fixture file at `specs/_compliance/drill-evidence/fixtures/cold-restore-tenant-pre-drill.json` updated within 24h.

## 3. RTO / RPO targets

These are the **hard ceilings** the drill validates. Failure to meet any one criterion drives a YELLOW or RED drill outcome and an action item against the closure plan.

| Metric | Target | Source-of-truth |
|---|---|---|
| **Detection-to-IC-establishment** | ≤ 15 min | PD ack stamp; runbook step 1 |
| **RTO (read-path)** | ≤ 4 hours from drill start to first synthetic-tenant read returning correct fixture data | runbook steps 1–8; verified by `verify-cold-restore.py` |
| **RTO (full write-path)** | ≤ 8 hours from drill start to first successful write that produces a valid audit-chain row + BYOK envelope unwrap | runbook step 8 smoke tests |
| **RPO (data loss window)** | ≤ 15 minutes — measured as `now() − last_replication_checkpoint_ts` at moment of "outage" declaration | `audit_chain_checkpoints.checkpoint_ts` + R2 cross-region replication SLA |
| **Audit-chain continuity** | Pre-drill Merkle root = post-restore Merkle root for the snapshot's checkpoint epoch | `verify-cold-restore.py` audit-chain check |
| **BYOK envelope integrity** | Synthetic tenant's pre-drill envelope unwraps post-restore returning the exact same DEK SHA-256 | `verify-cold-restore.py` BYOK check |

> **Why 4h read / 8h write?** Reads can come up the moment R2 + D1 are restored (steps 4–5). Writes additionally require DO state hydration + KV restore + BYOK envelope re-binding (steps 6–7). The 4h:8h ratio mirrors NIST SP 800-34's tier-3 contingency planning (most data systems target ≤ 24h for full recovery; we set an aggressive 8h ceiling because Cloudflare's restore primitives are operationally cheap).

> **Why 15min RPO?** This matches the `corelink-dr-drill::RPO_CEIL_SECONDS` ceiling (60s for warm failover) relaxed by an order of magnitude for cold-restore — cross-region replication lag + manifest emission cadence (hourly) sets the floor at ~15min. Tenants who require sub-15min RPO must adopt synchronous cross-region writes (a future product feature, post-GA).

## 4. Drill mode taxonomy

The drill orchestrator (`scripts/cold-restore-drill.sh`) supports three modes; only `--dry-run` and `--staging` are intended for the R-6 cadence. `--prod` exists as a documented but **firewall-protected** mode for the rare scenario where prod is itself destroyed and the operator needs the same orchestration path against real prod backups.

| Mode | What it does | Required env | Safe to run? |
|---|---|---|---|
| `--dry-run` | Validates inventory + permissions + backup freshness + manifest decrypt; touches no state; emits log only. | none | Always safe; runnable in CI. |
| `--staging` | Runs the full 9-step runbook against an ephemeral staging-equivalent region (`staging-cold-restore-YYYY-MM-DD`); tears down on exit. | `CORELINK_ENV=staging`; `BACKUP_GPG_RECIPIENT`; `BYOK_TEST_TENANT=cold-restore-drill-tenant` | Safe; isolated namespace. |
| `--prod` | Restores into **new prod region** from prod backups. ONLY for real incidents. | `CORELINK_ENV=production`; `CONFIRM_PROD=I_UNDERSTAND`; SRE-Lead PD ack within 5min. | NOT for drill; real-incident-only. |

The cadence schedules `--dry-run` weekly (CI cron) + `--staging` quarterly (DR-15). `--prod` is never scheduled.

## 5. Pre-drill checklist (T-24h gate)

The IC must walk this checklist 24h before scheduled drill. **Any unchecked item delays the drill** per `BCP-DR-DRILL-CADENCE.md` §9 cancellation rules.

### 5.1 Customer-comms (T-24h)

- [ ] Status-page banner posted: "Staging-only DR test — no customer impact expected" — schedule for the drill 90-min window.
- [ ] Lighthouse-customer account managers notified via email; opt-in to observe as auditor allowed (logged as `drill_observer`).
- [ ] No real customer SLAs are engaged because the drill targets a staging namespace.

### 5.2 Operational freezes (T-24h)

- [ ] Admin-plane write freeze activated for staging (feature flag).
- [ ] Deploy freeze: no merges to `main` between T-2h and T+drill_end + 4h post-drill.
- [ ] PagerDuty drill rotation overrides primary rotation for the drill 90-min + 4h post-drill window.

### 5.3 Baseline-metrics snapshot (T-24h)

- [ ] Grafana export: `dashboards-rto-rpo`, `dashboards-audit-chain`, `dashboards-byok-envelope`, `dashboards-slo-burn`.
- [ ] D1 audit-chain checkpoint hash captured: `SELECT merkle_root FROM audit_chain_checkpoints ORDER BY checkpoint_ts DESC LIMIT 1` — stored in pre-drill evidence doc.
- [ ] Synthetic-tenant fixture file at `specs/_compliance/drill-evidence/fixtures/cold-restore-tenant-pre-drill.json` refreshed (3 CAS blob SHA-256s, 5 audit-event IDs, 1 BYOK envelope WrappedDEK).
- [ ] Backup-bucket freshness: `wrangler r2 object get corelink-backups-{env}/latest/manifest.json` returns valid JSON.

### 5.4 War-room readiness (T-2h)

- [ ] `#drill-cold-restore-YYYY-MM-DD` Slack channel created + bookmarked.
- [ ] Runbook `RB-COLD-RESTORE-FROM-ZERO.md` printed/PDF'd as offline fallback.
- [ ] Conference bridge URL posted in channel.
- [ ] All participants confirmed in PD; no SEV1/SEV2 active.

## 6. Drill execution (overview — full procedure in `RB-COLD-RESTORE-FROM-ZERO.md`)

The drill executes the 9-step runbook timeline. Each step has a target time budget. The orchestrator script (`cold-restore-drill.sh`) emits structured log lines that map to runbook steps; the verification gate (`verify-cold-restore.py`) reads those log lines + queries restored state to compute pass/fail.

| Step | Title | Budget | Cumulative |
|---|---|---|---|
| 1 | Establish IC + war room | 15 min | T+0:15 |
| 2 | Inventory damage | 15 min | T+0:30 |
| 3 | Provision new region | 60 min | T+1:30 |
| 4 | Restore R2 from cross-region backup | 90 min | T+3:00 |
| 5 | Restore D1 from snapshot | 60 min | T+4:00 |
| 6 | Restore KV from export (parallel with step 5) | 60 min | T+4:00 |
| 7 | DO state recovery | 120 min | T+6:00 |
| 8 | Smoke tests | 60 min | T+7:00 |
| 9 | Customer comms + go-live decision | 60 min | T+8:00 |

**Read-path open ≤ T+4:00 (4h target); full write-path open ≤ T+8:00 (8h target).** Steps 5 and 6 run in parallel because D1 and KV restore use independent CF APIs.

## 7. Success criteria (auditor-readable)

The drill is **PASS** iff all 7 criteria below evaluate to true:

1. **RTO read-path met:** synthetic tenant's first GET against restored region returns correct fixture data at T ≤ 4:00 wall-clock.
2. **RTO write-path met:** synthetic tenant's first successful write (which generates audit event + envelope-wraps DEK) completes at T ≤ 8:00 wall-clock.
3. **RPO met:** `last_replication_checkpoint_ts` at moment of declared "outage" ≤ 15 min stale.
4. **Audit-chain continuity:** post-restore Merkle root for pre-drill epoch equals pre-drill checkpoint Merkle root (bit-for-bit).
5. **BYOK envelope integrity:** synthetic tenant's pre-drill envelope unwraps post-restore returning identical DEK SHA-256.
6. **SLO histograms recovered:** Prometheus / Grafana panels for `gw_request_latency_p99`, `audit_chain_append_latency_p99`, `byok_envelope_unwrap_latency_p99` show non-zero data within 15min of step 8 completion.
7. **No real customer impact:** zero production-tenant 5xx in drill window; zero production-audit-chain rows missing.

## 8. Failure thresholds + escalation

| Drill outcome | Criteria | Action |
|---|---|---|
| **PASS** | All 7 criteria green | Seal evidence doc; update GAP-15 status to GREEN; file the evidence in Drata `incident_response` stream. |
| **PARTIAL (YELLOW)** | 1–2 criteria amber (within 25% of target) | File AI within 7d; reschedule cold-restore drill within 30d; do **not** close GAP-15. |
| **FAIL (RED)** | ≥ 3 criteria amber OR any 1 criterion red OR write-path RTO > 16h (2× target) | SEV-2 declared (drill, not real); root-cause within 14d; cold-restore drill blocks Type II audit pass; GAP-15 stays open. |
| **ABORT** | Real SEV1/SEV2 hits during drill window | Halt drill immediately; resume real incident response; reschedule cold-restore within 14d. |

## 9. Post-drill evidence (sealed within 24h)

Evidence doc lives at `specs/_compliance/drill-evidence/YYYY-QQ-cold-restore-{dry-run|staging|prod}.md` using a section structure analogous to `templates/DR-DRILL-EVIDENCE.md` plus three cold-restore-specific sections:

- **§A** RTO/RPO measurements vs targets (table from §3 above).
- **§B** Per-step timing vs budget (table from §6 above with actual durations).
- **§C** Verification gate output (full `verify-cold-restore.py` output dumped verbatim).

The evidence doc MUST be uploaded to Drata via `corelink-drata-sync` CLI as an `incident_response` evidence payload (per `crates/corelink-drata-sync/src/stream.rs::EvidenceStream::IncidentResponse`).

## 10. Cadence + ownership

| Attribute | Value |
|---|---|
| **Cadence (pre-GA)** | One mandatory `--dry-run` before GA staging cut (T-30d); one mandatory `--staging` full execution before GA tag. |
| **Cadence (post-GA)** | Quarterly `--staging` execution (Jan + Apr + Jul + Oct, first Wednesday 14:00 UTC). |
| **Owner** | SRE Lead. |
| **IC role** | Senior SRE on rotation; backup IC = Eng Manager. |
| **Reviewers** | Compliance Officer (drill_status), Security Lead (BYOK + audit-chain checks), Privacy Lead (residency-tag preservation). |
| **Sign-off (post-drill)** | IC + Scribe + SRE Lead + Compliance Officer; doc_status frozen within 24h. |
| **SLA** | Complete drill cycle ≤ 8h wall; evidence doc sealed ≤ 24h post-drill. |
| **Retention** | 7 years (per `specs/_compliance/SOC2-ROADMAP.md` evidence-retention). |

## 11. Cross-references

- `specs/_runbooks/RB-COLD-RESTORE-FROM-ZERO.md` — 9-step procedural runbook.
- `scripts/cold-restore-drill.sh` — drill orchestrator (3 modes).
- `scripts/verify-cold-restore.py` — post-drill verification gate.
- `scripts/restore-from-snapshot.sh` — low-level restore primitive (used by step 4–6 of runbook).
- `scripts/backup-daily.sh` — backup primitive that produces the artifacts this drill consumes.
- `specs/_compliance/BCP-DR-DRILL-CADENCE.md` — DR-15 cadence row (this drill).
- `specs/_compliance/SOC2-EVIDENCE-ROLLUP-2026-05-15.md` — GAP-15 row; A1.2 + A1.3 status flips to GREEN on first PASS.
- `specs/_compliance/SOC2-GAP-ANALYSIS.md` §A1.2 — GAP-15 closure binding.
- `specs/_runbooks/RB-BACKUP-VERIFICATION.md` — DR-13 monthly warm-verification (prerequisite for DR-15).
- `specs/03_architecture/resilience_patterns.md` — PAT-REGION-FAILOVER-001, PAT-ROLL-FORWARD-001.
- `specs/03_architecture/data_model.md` — per-binding persistence matrix.
- `crates/corelink-region` — colo + residency-graph routing.
- `crates/corelink-failover-router` — sibling-bucket route selection.
- `crates/corelink-cf-bindings` — R2 / D1 / KV / DO wrappers.
- `crates/corelink-dr-drill` — drill scheduler crate (RTO/RPO ceilings; this drill registers as `DrillCycle::ColdRestoreFromZero`).
- `crates/corelink-drata-sync` — evidence upload CLI.

---

**End COLD-RESTORE-DRILL-SPEC.**
