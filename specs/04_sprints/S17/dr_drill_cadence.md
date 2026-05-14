---
id: "DR-DRILL-CADENCE-S17"
type: "governance"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.1.0"
created: "2026-05-14"
updated: "2026-05-14"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
parent: "S-17"
tags: ["governance", "wi-s17-002", "dr-drill", "cadence", "calendar", "semestral", "annual"]
---

# DR Drill Cadence Calendar — Semestral (Cycle 1) + Annual (Cycles 2/3 deferred at GA)

> **WI:** WI-S17-002 | **Crate:** `corelink-dr-drill` | **Runbook:** [`RB-DR-DRILL`](../../05_quality/runbooks/RB-DR-DRILL.md)

## Cadence overview

| Cycle | Scenario | Cadence | CF Cron | Status |
|---|---|---|---|---|
| 1 | CF region outage simulate + failover + SLO sustained | Semestral | `0 6 1 1,7 *` | **ACTIVE — mandatory ship gate (S-17)** |
| 2 | D1 primary loss + restore from backup | Annual (deferred at GA via waiver opt) | `0 6 1 1 *` | PLACEHOLDER — ADR mandatory |
| 3 | BYOK key compromise + crypto-erase + customer notification | Annual (deferred at GA via waiver opt) | `0 6 1 7 *` | PLACEHOLDER — ADR mandatory |

## Cron expression breakdown

Format: `minute hour day-of-month month day-of-week`.

- **Cycle 1 — `0 6 1 1,7 *`**: minute=0, hour=6, dom=1, month={Jan, Jul}, dow=* → Jan 1 + Jul 1 at 06:00 UTC.
- **Cycle 2 — `0 6 1 1 *`**: Jan 1 at 06:00 UTC (annual).
- **Cycle 3 — `0 6 1 7 *`**: Jul 1 at 06:00 UTC (annual; staggered offset to avoid overlap with cycle 2).

## Why semestral (não quarterly)?

Per WI §9.1:

- Semestral balances drill frequency against ops cost (SRE Lead solo-tier per ADR-0034).
- Quarterly is the ideal end-state once SRE team headcount is staffed (Tier-1 hire pending).
- Pós-GA roadmap: increase to quarterly cadence post-S-Y.

## Why cycle 1 = CF region outage first?

Per WI §9.2:

- FM-101 CF edge outage is highest-frequency real-world risk (P0).
- D1 primary loss (cycle 2) is lower frequency (Neon backup baseline).
- BYOK key compromise (cycle 3) is lowest frequency (CRITICAL but rare; reuses S-14 patterns).
- Cycle 1 first establishes production confidence baseline.

## Calendar (next 7 years; 7y archive horizon per Quality Standard 14.s17.7)

| Date (UTC 06:00) | Cycle | Cadence | Action |
|---|---|---|---|
| 2026-07-01 | 1 | Semestral | **First execution — S-17 mandatory ship gate** |
| 2027-01-01 | 1 | Semestral | Second cycle 1 execution |
| 2027-01-01 | 2 | Annual | Cycle 2 first execution (post-GA; waiver expired) |
| 2027-07-01 | 1 | Semestral | Third cycle 1 execution |
| 2027-07-01 | 3 | Annual | Cycle 3 first execution (post-GA; waiver expired) |
| 2028-01-01 | 1 | Semestral | … |
| 2028-01-01 | 2 | Annual | … |
| 2028-07-01 | 1 | Semestral | … |
| 2028-07-01 | 3 | Annual | … |
| 2029-01-01 → 2033-12-31 | repeat pattern | both cadences | rolling 7y retention archive in R2 |

## Deferred annual cycles 2/3 — waiver opt + ADR

Per WI §6.2 + §9.5:

- Cycles 2 + 3 are deferred annual at GA via waiver opt (per spec contract §19).
- ADR mandatory: `specs/_decisions/ADR-XXXX-dr-drill-cycles-2-3-deferred.md`.
- Waiver expiry: next sprint review post-GA.
- SRE Lead approves.

## Cadence-missed alert

Prometheus alert when expected cadence cycle is skipped:

```promql
# Alert if semestral cycle 1 hasn't run within 200d (cron interval = 182d + buffer).
absent_over_time(corelink_dr_drill_total{cycle="1", outcome="completed"}[200d]) == 1
```

Routes to SRE Lead. Severity SEV-2. Runbook: [`RB-DR-DRILL`](../../05_quality/runbooks/RB-DR-DRILL.md).

## References

- WI-S17-002 — DR drill scheduler + cycle 1 CF region outage.
- `crates/corelink-dr-drill` — `SEMESTRAL_CRON` constant + scheduler trait.
- `RB-DR-DRILL` — execution procedure.
- ADR-0034 — staffing solo-tier (SRE Lead pending).

---

**Fim DR-DRILL-CADENCE-S17.**
