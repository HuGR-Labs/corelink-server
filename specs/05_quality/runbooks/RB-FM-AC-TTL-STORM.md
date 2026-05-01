---
id: "RB-FM-AC-TTL-STORM"
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
tags: ["runbook", "p2", "ac-ttl", "cron", "scaling"]
---

# RB-FM-AC-TTL-STORM — AC TTL Cron Workload Outpaces Interval

> **FM:** FM-AC-TTL-STORM (S=3, P2) | **CTRLs:** INV-AC-EVICT-CONSISTENCY + bounded batch | **SLA:** mitigate < 4h (sustained workload signal)

## Detection

- Metric `corelink.ac.ttl.batch_size{region}` continuously hits the canonical cap (250) for ≥ 3 ticks in a row.
- `TtlWorkerTickOutcome::hit_row_ceiling == true` reported by every tick (pure-logic worker; production wraps in DO `alarm()`).
- Metric `corelink.ac.ttl.batch_duration_ms{region}` approaches the 30s alarm budget.
- D1 query: `SELECT COUNT(*) FROM ac_meta WHERE expires_at < now() AND region = ?` returns ≥ 10× expected backlog (sustained workload).

## Communication

- **SEV-2.** Page SRE + Architect.
- Internal-only; not customer-facing.

## Immediate mitigation (≤ 1h)

1. **Reduce alarm interval**: temporarily drop `CORELINK_AC_TTL_CRON_INTERVAL_S` to 1800s (30 min) for the affected region while the backlog drains.
2. **Increase row ceiling**: per-tick `row_ceiling` (`InMemoryTtlWorker` arg / production DO env) can rise from 1000 → 5000 to absorb a batch backlog faster — accepts longer per-tick wall-clock.
3. **Scale-out**: if a single region's cron is the bottleneck, sharding to multiple DO instances per region (currently 1; see WI-S04-005 anti-scope §7) requires ADR + WI-S04-006 follow-up.

## Full mitigation (≤ 4h)

1. **Identify backlog source**:
   - Mass TTL change runtime (env override): `CORELINK_AC_TTL_DEFAULT_MS` reduced; pre-existing rows now expired en masse → drain naturally over a few cron intervals.
   - Per-tier TTL migration (S-13 admin plane): expected; documented in ADR-0019 §Migration plan.
   - Customer mass deletion / quota cleanup: confirm no customer-driven anomaly via audit chain `corelink.ac.evict.*` event volume.
2. **Drain plan**: run cron tick at reduced interval until `batch_size < cap` sustained for 5 ticks.
3. **Restore canonical interval**: revert `CORELINK_AC_TTL_CRON_INTERVAL_S` to 3600s.
4. **Tune ceilings** if sustained workload signals undersized cron — open follow-up Lote.

## Forensics

1. Audit log: aggregate `corelink.ac.evict.ttl_expired` rate per region per hour.
2. Storm signature: backlog growth slope vs cron tick eviction rate; if eviction < growth, scale-out is overdue.
3. Customer-driven: per-tenant audit chain `corelink.ac.update.ok` rate (high write rate + short TTL = self-induced storm).

## Post-mortem hooks

- Storm signal triggered > 24h sustained → **SEV-1** review (cron undersized for steady-state load).
- Per-tenant storm signal correlated with single tenant → quota escalation review (S-08 forward).
- Cron interval reduced below 60s minimum → CHARTER violation (R2 rate-limit risk).

## Dry-run procedure (deferred)

The full host-side dry-run with synthetic 10 000-row backlog lands alongside the Cloudflare DO binding in WI-S04-006. The pure-logic worker (`InMemoryTtlWorker`) property test `prop_evict_idempotent_across_repeated_ticks` (10 000 iter) plus `tick_signals_storm_at_row_ceiling` (unit) cover the algorithmic boundary.

## References

- WI-S04-005 §6.1.5 + §6.1.10 (bounded batch + storm metric).
- WI-S04-005 §15 chaos experiment 12 (10M-row bulk eviction storm).
- `RB-FM-AC-TTL-DRIFT.md` — sibling runbook for the inverse failure (cron silent / not firing).
- `crates/corelink-worker/src/reapi/ac/ttl/worker.rs` — `TtlWorkerTickOutcome::hit_row_ceiling` signal.
