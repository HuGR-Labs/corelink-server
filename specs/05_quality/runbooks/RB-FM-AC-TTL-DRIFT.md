---
id: "RB-FM-AC-TTL-DRIFT"
type: "runbook"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.1.0"
created: "2026-05-01"
updated: "2026-05-01"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["runbook", "p2", "ac-ttl", "cron", "data-lifecycle"]
---

# RB-FM-AC-TTL-DRIFT — AC TTL Cron Worker Stale / Misconfigured

> **FM:** FM-AC-TTL-DRIFT (S=3, P2) | **CTRLs:** INV-AC-TTL-MONOTONIC + INV-AC-EVICT-CONSISTENCY | **SLA:** mitigate < 1h (eviction lag > 2× cron interval)

## Detection

- Metric `corelink.ac.ttl.cron_ticks_total{region}` rate < expected (target ≥ 1/h × 5 regions = 120/day; alert SEV-2 if < 100/day sustained).
- Metric `corelink.ac.ttl.batch_duration_ms{region}` p99 above 30s (alarm budget exhausted; partial sweep).
- Metric `corelink.r2.ac_bucket.size_bytes{region}` growing despite no new writes (rows expired but not evicted).
- D1 query: `SELECT region, COUNT(*) FROM ac_meta WHERE expires_at < now() - 7200000 GROUP BY region` returns rows older than 2h post-expiry.
- Customer GET path returns expired (410) rows that should already be 404 (cron failed to clean D1+R2; clients see stale-row signal).

## Communication

- **SEV-2.** Page SRE + Architect (DBA specialization).
- Status page: nominal (TTL drift is internal data-lifecycle; not customer-facing latency).
- No customer comms required unless drift exceeds 24h or storage cost spike triggers billing alert.

## Immediate mitigation (≤ 1h)

1. **Verify alarm registration**: query Cloudflare DO `state.storage.alarm()` per region. If `None`, re-arm via DO alarm registration script (`scripts/rb_fm_ac_ttl_drift_rearm.sh` — TODO post WI-S04-006).
2. **Verify env config**: confirm `CORELINK_AC_TTL_CRON_INTERVAL_S` is set + within bounds (≥ 60s, ≤ 86400s); reject misconfig.
3. **Manual cron tick**: invoke `worker-evict-ac-ttl-<region>` DO `tick(now_ms, request_id)` ad-hoc via wrangler CLI to drain the backlog.
4. **Check D1 health**: `wrangler d1 execute --command "SELECT 1"` confirms reachability.
5. **Check R2 health**: `wrangler r2 object list ac-<region> --limit 1` confirms reachability.

## Full mitigation (≤ 4h)

1. **Identify root cause**:
   - Cron not re-armed: alarm system regression; investigate DO crash mid-`alarm()`.
   - D1 batch failures: D1 outage (contact Cloudflare support if widespread).
   - R2 outage: per-region R2 failure (check Cloudflare status page).
   - Misconfigured cron interval: revert env config to default 3600s.
2. **Drain backlog**: run cron tick repeatedly until `corelink.ac.ttl.batch_size` < `MAX_BATCH_SIZE` (250) sustained for 3 ticks.
3. **Re-validate alarm**: confirm `state.storage.alarm()` returns the next-fire timestamp; if absent post-drain, the alarm system itself is broken (escalate to Cloudflare).

## Forensics

1. Audit log: `corelink.ac.evict.ttl_expired` events per region; expected rate = expired-row rate × cron tick frequency.
2. Tick history: `corelink.ac.ttl.cron_ticks_total{region}` time series over last 7 days; identify when ticks went silent.
3. DO logs: stack trace from any `alarm()` panic / abort.

## Post-mortem hooks

- Cron stale > 4h sustained → **SEV-1** post-mortem (data-lifecycle SLA violation).
- Cron interval misconfig (< 60s aggressive, > 86400s conservative) → **SEV-2** review env config rollout.
- Per-region cron skew > 30 min sustained → review alarm stagger (sam=00, iad=12, lhr=24, nrt=36, syd=48 minutes past hour per WI-S04-005 §6.1.4).

## Dry-run procedure (deferred to staging)

The full host-side dry-run lands alongside the Cloudflare D1/R2 binding shim in WI-S04-006 (per charter trait-abstraction-defer pattern). The pure-logic cron worker (`InMemoryTtlWorker`) tested via 6 property tests at 10 000 iter pins the canonical algorithmic flow — this dry-run runbook documents the production procedure for staging post-WI-S04-006.

## References

- WI-S04-005 §6.1.4 (cron alarm + re-arm semantics).
- ADR-0019 — TTL ownership boundary S-04 ↔ S-07.
- `crates/corelink-worker/src/reapi/ac/ttl/worker.rs` — `TtlWorker::tick` reference impl.
- `RB-FM-AC-TTL-STORM` — sibling runbook for sustained workload outpacing the cron interval.
- `RB-FM-303` — escalation path if drift correlates with tenant-isolation alerts.
