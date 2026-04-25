---
id: "ADR-0042"
type: "adr"
doc_status: "FROZEN"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-25"
updated: "2026-04-25"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["adr", "gc", "worker", "scheduler", "degrade-mode", "s06"]
---

# ADR-0042 — GC Worker Scheduler Design + Degrade-Mode `gc-pause` Contract

## Status

FROZEN (S-06 WI-S06-001 ratificada em Lote 10.6 ship gate WI-S06-007).

## Context

S-06 GC worker é **single point of failure** para INV-GC-001 (reachable never deleted). Worker scheduler design impacta:

1. **Cron schedule** (per-region; jitter ±10min para evitar thundering herd 5 regions × 02:00 UTC).
2. **Sticky DO per region** (consistente WI-S04-005 sweeper pattern).
3. **Idempotent re-run via checkpoint** (gc_run table; PAT-RETRY-IDEMPOTENT-001).
4. **Single running per (tenant, region)** (partial UNIQUE WHERE status='running'; lesson Lote 10.5bis partial UNIQUE).
5. **Phase transitions monotonic** (idle→mark→sweep→physical_delete→reconcile→completed).
6. **Degrade-mode `gc-pause` global emergency stop** (PAT-DEGRADE-001 alignment).
7. **Manual trigger admin API** (forward S-13; staging stub OK).

## Decision

**Per-region sticky DO + cron 02:00 UTC + jitter ±10min + degrade-mode gc-pause via DO config-singleton + manual admin trigger forward S-13.**

**Architectural choices**:

| Aspect | Decision | Rationale |
|---|---|---|
| Worker topology | 5 regions × 1 sticky DO each | Pattern reuse WI-S04-005 sweeper; per-region isolation; aligned com R2 bucket region attribution |
| Cron schedule | 02:00 UTC daily + jitter ±10min | Off-peak hour; jitter spreads thundering herd; configurable env `CORELINK_GC_CRON_JITTER_MINUTES` |
| Concurrency control | Partial UNIQUE INDEX `WHERE status='running'` | Lote 10.5bis partial UNIQUE lesson; race-free; allows multiple completed/aborted records as audit trail |
| Idempotent resume | Checkpoint per batch boundary em gc_run.last_checkpoint_at_ms | PAT-RETRY-IDEMPOTENT-001; worker crashes resume from last checkpoint |
| Phase tracking | gc_run.phase enum monotonic | CHECK constraint inline; reverse transitions rejected |
| Degrade-mode | DO config-singleton + probe per batch boundary | Bounded propagation ≤100ms; PAT-DEGRADE-001 |
| Manual trigger | Admin API stub forward S-13 | Staging stub returns 501 in non-staging envs; per-tenant rate limit forward |
| Stale detection | Index `WHERE status='running' AND last_checkpoint > 1h` | Crashed workers detected within 1h; cron daily promotes to status='crashed' |
| gc_run cleanup | Daily cron truncate > 30d old | Bounded table size; configurable env |

**Rejected alternatives**:

- **Single global cron** (não per-region): single failure point; coordination overhead; rejected per WI-S04-005 pattern.
- **No jitter** (5 regions fire simultaneously): D1 throttle thundering herd; rejected.
- **Hash-based concurrency control** (não partial UNIQUE): D1 has no native row-locks for hashing; partial UNIQUE é cleanest SQLite-supported pattern.
- **Worker as monolithic** (não phase-tracked): impossible to checkpoint mid-phase; rejected for idempotent-resume requirement.
- **Synchronous degrade-mode** (não probe per batch): degrade-mode propagation latency unbounded; rejected.

## Consequences

**Positive**:
- Per-region scheduler scales horizontally (more regions = more workers; no coordination).
- Idempotent re-run handles crashes gracefully.
- Partial UNIQUE prevents race; race-free per-tenant per-region single-flight.
- Degrade-mode bounded propagation ≤ 100ms (probe per batch boundary).
- Pattern reuse from WI-S04-005 sweeper reduces cognitive load for ops team.

**Negative**:
- Per-region sticky DO means single tenant's GC runs sequentially per region (não parallelizable per-tenant); for fat tenants > 5M blobs phase budget exceeded → SEV-2 alert.
- Manual admin trigger forward S-13 (staging stub may surprise prod ops; 501 fallback documented).

**Neutral**:
- gc_run table size bounded to 30d retention (~15M rows max em high-volume tenant; D1 single-shard fits).

## References

- WI-S06-001 §1 worker skeleton design.
- WI-S06-002..005 consume scheduler.
- ADR-0019 (TTL ownership; sweeper pattern reuse).
- ADR-0034 (PRR staffing waiver path).
- ADR-0036 (schema migration governance) — gc_run schema reuse pattern.
- `failure_modes.md FM-300/305/404` — runbook RB-FM-* dry-runs em WI-S06-007.
- `resilience_patterns.md PAT-DEGRADE-001 + PAT-RETRY-IDEMPOTENT-001`.

## Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (via Claude Opus 4.7) | Criação ADR-0042 (Lote 10.6 worker scheduler design). Per-region sticky DO; cron 02:00 + jitter; partial UNIQUE concurrency control; idempotent resume; degrade-mode gc-pause probe per batch. |
