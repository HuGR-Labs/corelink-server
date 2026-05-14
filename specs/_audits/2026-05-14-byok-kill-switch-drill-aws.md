---
type: chaos-drill-report
wi: WI-S14-006
date: 2026-05-14
provider: aws
tenant: staging-byok-drill-aws
sla_seconds: 360
result: PASS
---

# BYOK Kill Switch Chaos Drill — aws — 2026-05-14

## Summary

| Metric | Value | SLA | Result |
|---|---|---|---|
| Detection latency | 2s | ≤ 60s | PASS |
| Total kill switch duration | 2s | ≤ 360s | PASS |
| DEK cache empty post-evict | true | true | PASS |
| Tenant degraded read-only | degraded_read_only = degraded_read_only | true | PASS |
| Recovery after re-enable | active = active | true | PASS |

## Provider

**aws** (week 20 of 4-week rotation: aws gcp azure vault)

## INV-BYOK-CRYPTO-SOVEREIGNTY

Kill switch SLA: **PASS**

## Drift Findings

- None (automated drill; staging environment).

## Drill Date

2026-05-14 UTC — cron weekly Sunday 03:00 UTC.
