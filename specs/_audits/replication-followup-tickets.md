---
id: "AUDIT-REPLICATION-FOLLOWUP-TICKETS-2026-05-15"
type: "audit"
doc_status: "REVIEW"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-05-15"
updated: "2026-05-15"
sprint: "R-prep follow-on backlog (Sprint-N targeting)"
parent_wi: "R-PREP-REPLICATION-AUDIT"
owner: "Gustavo Schneiter"
tags: ["audit", "replication", "backlog", "sprint-ready", "rpo", "dr-16", "wi-candidate"]
---

# Cross-Region Replication — Follow-up backlog (Sprint-ready WI candidates)

> **doc_status:** REVIEW · **scope:** convert the six replication gaps
> (GAP-R1..R6) identified in `2026-05-15-replication-audit.md` into
> Sprint-ready WI candidates, each with acceptance criteria, test
> plan, and success metric. Every WI here must be closed **before**
> the DR-16 active-failover *production-mode* dry-run is authorized
> (staging dry-run can proceed with P1/P2 outstanding, but P0 are
> hard blockers).
>
> **Anchor:** `specs/_audits/2026-05-15-replication-audit.md` §3
> (per-domain audit) + §5 (inconsistency windows) + §7 (verification).

---

## Ticket schema

Each ticket below uses the canonical form:

- **ID:** `R-PREP-REPL-{Pn}-NNN` — sprint-prefix follows the
  convention of `perf-optimization-followup-tickets.md`.
- **Priority:** P0 = hard blocker for GA (active-failover production
  dry-run cannot proceed); P1 = before GA but after staging dry-run
  proves the surface; P2 = post-GA polish.
- **Domain:** R2 / D1 / KV / DO / cross-domain.
- **Hypothesis:** what we believe is broken.
- **Acceptance criteria:** observable, testable outcome.
- **Test plan:** how reviewers verify.
- **Success metric:** concrete numeric / boolean gate.

---

## P0 — hard blockers (before DR-16 prod dry-run)

### R-PREP-REPL-P0-001 — R2-hot replication lag continuous SLI

- **ID:** R-PREP-REPL-P0-001
- **Priority:** P0
- **Domain:** R2 (hot blobs via replica-worker)
- **Maps to gap:** GAP-R1 (R2-hot RPO target ≤ 60 s **unmeasured** in production)
- **Hypothesis:** the `REPLICATION_LAG_P99_SLO_SECS = 60` constant in
  `crates/corelink-replica-worker/src/region.rs` L180 is asserted as
  a target but no Prometheus SLI emits the actual measurement; we
  cannot prove the SLO in production without it.
- **Acceptance criteria:**
  1. Replica-worker emits `corelink_replication_lag_seconds{domain="r2_hot", region}` histogram per blob upon `replication.completed` audit emit, with `lag = replication_completed_ts − blob.created_ts`.
  2. Batch-level counter `corelink_replication_batch_total{outcome ∈ ("ok","partial","failed")}` distinguishing partial-failure batches (the silent-skip hazard called out in audit §5.2).
  3. New SLO `SLO-REPLICATION-LAG-R2` (target p99 ≤ 60 s) added to `slo_catalog.md` (done in this audit's commit) and a Prometheus alerting rule with 1h fast-burn + 6h slow-burn windows per Google SRE Workbook Ch. 5.
- **Test plan:**
  - Unit: assert histogram bucket boundaries match SLO threshold (60 s).
  - Integration: drive `InMemoryReplicationWorker` with seeded blobs of synthetic age; assert lag histogram bucket counts match.
  - Staging: run `scripts/verify-replication-lag.py --domain r2` against staging Prometheus for 7 d; verify p99 ≤ 60 s and partial-batch counter ≤ 1% of batches.
- **Success metric:** p99 lag ≤ 60 s sustained 7 d in staging; zero "partial" batches with no Prometheus emit.

### R-PREP-REPL-P0-002 — D1 read-replica lag continuous SLI

- **ID:** R-PREP-REPL-P0-002
- **Priority:** P0
- **Domain:** D1 (cross-region read replica)
- **Maps to gap:** GAP-R3 (D1 read-replica lag unmeasured)
- **Hypothesis:** DR-16 §1.3 trigger requires "replication lag ≤ 5 min RPO budget at declaration" — but the only way to know D1 lag today is to query CF dashboard manually; without a Prometheus SLI, the runbook cannot make a programmatic go/no-go.
- **Acceptance criteria:**
  1. A per-region D1 read-replica lag gauge `corelink_d1_replica_lag_seconds{primary_region, replica_region}` emitted every 30 s by a lightweight probe worker (queries primary `MAX(created_at)` and replica `MAX(created_at)` on a known low-volume table; difference = lag).
  2. New SLO `SLO-REPLICATION-LAG-D1` (target p99 ≤ 60 s, RPO ceiling 6 h) wired into `slo_catalog.md` + alerts (done in this commit).
  3. DR-16 RB-ACTIVE-FAILOVER §1 (Detect step) reads this metric instead of CF dashboard.
- **Test plan:**
  - Unit: D1 probe correctness on synthetic clock skew.
  - Integration: inject lag via test fixture; assert SEV-2 alert at 5 min sustained.
  - Staging: 7 d of measured lag p99 ≤ 60 s; one synthetic drill where probe-lag > 5 min triggers SEV-2 within 1 min.
- **Success metric:** p99 D1 lag ≤ 60 s sustained 7 d; alert fires within 1 min when injected lag > threshold.

### R-PREP-REPL-P0-003 — Cross-region replication-lag verifier in daily cron

- **ID:** R-PREP-REPL-P0-003
- **Priority:** P0
- **Domain:** Cross-domain (orchestration)
- **Maps to gap:** GAP-R1, R3, R4, R5 collectively (no daily aggregate check)
- **Hypothesis:** without a daily script that polls all four domains, individual SLOs may silently regress between incident reviews.
- **Acceptance criteria:**
  1. `scripts/verify-replication-lag.py` (added in this audit's commit) wired into the daily cron (alongside `scripts/backup-daily.sh`).
  2. Exit non-zero if any domain exceeds RPO budget; emit Prometheus `corelink_replication_verifier_status{result}` counter.
  3. SEV ladder: 1 fail = SEV-3; 2 consecutive = SEV-2; 3 consecutive = SEV-1 (mirrors `SLO-BACKUP-VERIFICATION` alert ladder).
- **Test plan:**
  - Unit: `pytest` on the script with mocked Prometheus query responses.
  - Integration: GitHub Actions nightly run against staging; assert exit 0 in green path; assert exit 1 + SEV-3 on injected stale fixture.
- **Success metric:** 30 consecutive daily runs pass in staging before DR-16 prod-mode dry-run.

---

## P1 — before GA, after staging dry-run proves the surface

### R-PREP-REPL-P1-001 — KV cross-region propagation-lag SLI

- **ID:** R-PREP-REPL-P1-001
- **Priority:** P1
- **Domain:** KV
- **Maps to gap:** GAP-R4 (KV propagation ≤ 60 s "typical" claim untested)
- **Hypothesis:** the "≤ 60 s typical" claim from Cloudflare KV docs is a documentation assertion; we should empirically verify it at our scale before production traffic relies on it.
- **Acceptance criteria:**
  1. Synthetic KV-probe worker writes `kv_probe:<region>:<ts_ms>` from each region every 30 s and reads from all four regions; emits `corelink_kv_propagation_lag_seconds{write_region, read_region}` histogram.
  2. New SLO `SLO-REPLICATION-LAG-KV` (target p99 ≤ 60 s typical / ≤ 300 s pessimistic) added to `slo_catalog.md` (done in this commit).
- **Test plan:**
  - Synthetic-probe deployed to staging; 7 d of data; p99 must be ≤ 60 s in 95% of region-pair samples.
- **Success metric:** p99 KV propagation lag ≤ 60 s in 95% of inter-region samples sustained 7 d.

### R-PREP-REPL-P1-002 — `audit_outbox` failback drain test

- **ID:** R-PREP-REPL-P1-002
- **Priority:** P1
- **Domain:** D1 / audit chain
- **Maps to gap:** Audit §5.1 hidden hazard — old-primary outbox draining during failback
- **Hypothesis:** RB-ACTIVE-FAILOVER step 7 (Reverse / failback) documents the drain requirement, but no automated test asserts that re-engaging writes is **blocked** while the old primary's `audit_outbox` has un-drained rows.
- **Acceptance criteria:**
  1. New E2E test under `tests/e2e-failover-router/` that seeds rows in old-primary `audit_outbox` (drain skipped), executes failback flow, and asserts that the orchestrator refuses to re-engage writes until `audit_outbox WHERE emitted_at IS NULL` count = 0.
  2. Refusal emits `corelink_failback_blocked_total{reason="audit_outbox_dirty"}` for observability.
- **Test plan:** new E2E scenario added to `active-failover-drill.sh --staging` matrix.
- **Success metric:** E2E scenario passes; manual injection of dirty outbox triggers refusal within 1 cycle.

### R-PREP-REPL-P1-003 — DO-to-D1 sync-age SLI for TenantQuota + RateLimiter

- **ID:** R-PREP-REPL-P1-003
- **Priority:** P1
- **Domain:** DO state
- **Maps to gap:** GAP-R5 (DO cross-region rebuild latency unmeasured)
- **Hypothesis:** at failover, DO state is rehydrated from D1 — staleness is bounded by the DO's last-D1-sync. We have `tenant_storage_state.last_synced_at` (data_model.md §4.2) but no Prometheus gauge for sync-age.
- **Acceptance criteria:**
  1. `corelink_do_sync_age_seconds{do_class, tenant_tier_label}` gauge emitted from each DO on its periodic tick.
  2. New SLO `SLO-REPLICATION-LAG-DO` (target p99 sync-age ≤ 300 s for `TenantQuota`; ≤ 60 s for `ConfigSingleton`; `RateLimiter` excluded — intentional reset) added to `slo_catalog.md` (done in this commit).
- **Test plan:** unit + integration as for P0-001; staging 7-d burn-in.
- **Success metric:** p99 sync-age within target sustained 7 d.

### R-PREP-REPL-P1-004 — R2 platform CRR (cold/AC/audit) lag indirect SLI

- **ID:** R-PREP-REPL-P1-004
- **Priority:** P1
- **Domain:** R2 (cold + AC + audit buckets — platform CRR)
- **Maps to gap:** GAP-R2 (R2 platform CRR lag not directly measured)
- **Hypothesis:** Cloudflare R2 cross-region replication is a managed feature without a direct lag metric; we can build an *indirect* SLI by writing a synthetic object every 5 min and checking the replica bucket.
- **Acceptance criteria:**
  1. Synthetic R2-probe object `r2_probe/<region>/<ts_ms>.bin` written every 5 min to each region; sibling-region probe-worker confirms presence; `corelink_r2_crr_lag_seconds{primary_region, replica_region}` histogram emitted.
  2. Daily aggregate p99 ≤ 24 h surfaced as part of `SLO-REPLICATION-LAG-R2` extension (or new sub-SLO `SLO-REPLICATION-LAG-R2-CRR`; see ADR pending).
- **Test plan:** 7-d staging burn-in.
- **Success metric:** p99 lag ≤ 24 h with zero "object missing after 24 h" incidents.

---

## P2 — post-GA polish

### R-PREP-REPL-P2-001 — Neon read-replica lag CoreLink-side SLI

- **ID:** R-PREP-REPL-P2-001
- **Priority:** P2
- **Domain:** Neon
- **Maps to gap:** GAP-R6 (Neon replica lag is platform-measured, no CoreLink SLI)
- **Hypothesis:** Neon's SLA is sufficient for GA, but post-GA we may want our own lag signal to catch silent Neon-platform regressions.
- **Acceptance criteria:**
  1. Probe-worker writes `neon_probe(ts)` to primary every 60 s; reads from each EU replica; emits `corelink_neon_replica_lag_seconds{replica}` gauge.
  2. Soft SLO `SLO-REPLICATION-LAG-NEON` (target p99 ≤ 5 s) — informational, not alerting.
- **Test plan:** 30-d post-GA observation; no alerting initially.
- **Success metric:** baseline established; reviewed quarterly.

### R-PREP-REPL-P2-002 — Replica-worker hot-blob coverage SLI

- **ID:** R-PREP-REPL-P2-002
- **Priority:** P2
- **Domain:** R2 (hot blobs)
- **Maps to gap:** Audit §3.1 edge case (c) — "sudden hot blob is cold for the full aggregation window"
- **Hypothesis:** the aggregation window (`AGGREGATION_WINDOW_DAYS`) creates a blind spot for newly-hot blobs. We may want to instrument "newly-hot but not-yet-replicated" as an explicit SLI to inform whether the aggregation window is the right cadence.
- **Acceptance criteria:**
  1. `corelink_hot_blob_replication_coverage_ratio` gauge = `(blobs_replicated_within_24h_of_becoming_hot) / (blobs_classified_hot_in_window)`.
  2. Target ≥ 90% (i.e. at most 10% of hot blobs replicate later than 24 h after qualification).
- **Test plan:** post-GA observation 90 d; if < 90%, propose ADR shortening aggregation window.
- **Success metric:** coverage ratio ≥ 90% sustained.

### R-PREP-REPL-P2-003 — MultipartSession in-flight inventory at failover

- **ID:** R-PREP-REPL-P2-003
- **Priority:** P2
- **Domain:** DO state (MultipartSession)
- **Maps to gap:** Audit §3.5 edge case (b) — in-flight uploads ABORTED on failover, client must restart
- **Hypothesis:** clients with large multi-GiB uploads in flight at failover lose work; we should at minimum surface a per-failover count + audit emit so on-call can proactively notify enterprise tenants.
- **Acceptance criteria:**
  1. RB-ACTIVE-FAILOVER step 3 (Drain) lists multipart in-flight count by tenant and emits `failover.multipart_aborted.v1` CloudEvent per aborted session.
  2. Enterprise on-call comms template added for "your in-flight upload was aborted; please restart" notification.
- **Test plan:** staging dry-run with 3 synthetic in-flight uploads; verify all three emit `multipart_aborted` and on-call playbook fires.
- **Success metric:** 100% of in-flight multipart sessions accounted for in audit at failover.

---

## Summary

| Priority | Count | Description |
|---|---|---|
| P0 | 3 | Hard blockers for DR-16 prod-mode dry-run (SLI + verifier coverage of R2-hot + D1 + daily aggregate). |
| P1 | 4 | Required before GA; covers KV + audit_outbox + DO + R2-CRR. |
| P2 | 3 | Post-GA polish (Neon SLI + replica-worker coverage + multipart-failover inventory). |
| **Total** | **10** | |

All 10 tickets reference back to the canonical audit (`2026-05-15-replication-audit.md`) and the four new SLOs (`SLO-REPLICATION-LAG-{R2,D1,KV,DO}`) added to `slo_catalog.md` in the same commit.

---

## Cross-links

- Source audit: `specs/_audits/2026-05-15-replication-audit.md`
- Updated SLOs: `specs/03_architecture/slo_catalog.md §4.23..4.26`
- Verifier: `scripts/verify-replication-lag.py`
- Consuming drill specs: `specs/_compliance/ACTIVE-FAILOVER-DRILL-SPEC.md` (DR-16) + `specs/_compliance/COLD-RESTORE-DRILL-SPEC.md` (DR-15)
- ROADMAP entry: `ROADMAP-TO-GA.md §6` (Wave R-6 BCP/DR cadence)
