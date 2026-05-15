---
id: "RB-SLO-DEDUP-DEGRADATION"
type: "runbook"
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
tags: ["runbook", "slo", "dedup", "efficiency", "cost", "r6-2"]
---

# RB-SLO-DEDUP-DEGRADATION — Dedup Ratio SLO Degradation

> **SLO covered:** SLO-DEDUP-RATIO (§4.8.1). Targets vary by tier: free ≥ 2.0×, solo ≥ 2.5×, team ≥ 2.5×, business ≥ 2.8×, enterprise ≥ 3.0× sustained 7d.
> **Alert windows:** 1h fast-burn (10× violation rate, SEV-2) + 6h slow-burn (3× rate, SEV-3) per Google SRE Workbook Ch.5.
> **Anomaly layer:** SEV-3 if `corelink_cache_hit_ratio` drops > 30% WoW.

## Symptom

- PagerDuty SEV-2 page `corelink-slo-dedup-ratio-fast-burn` fires.
- Customer complaint: "my storage bill jumped 2× this month for the same workload".
- `DASH-CAS` dedup ratio panel below target line for sustained 1h+.
- Marketing/sales reports: competitor benchmark claim (3× SOTA) no longer holds.

## Detection

```promql
# Current dedup ratio (7d window) per tenant_id
rate(corelink_dedup_bytes_saved_total{type="chunk"}[7d])
  / rate(corelink_dedup_bytes_uploaded_total[7d])

# Anomaly: drop > 30% WoW
(rate(corelink_dedup_bytes_saved_total[7d] offset 1w)
  / rate(corelink_dedup_bytes_uploaded_total[7d] offset 1w))
- (rate(corelink_dedup_bytes_saved_total[7d])
  / rate(corelink_dedup_bytes_uploaded_total[7d]))
  > 0.3
```

Anomaly correlation:

```promql
# Cache hit ratio (proxy)
corelink_cache_hit_ratio  < (corelink_cache_hit_ratio offset 1w) * 0.7
```

## Immediate mitigation (≤ 5 min)

Dedup degradation is **not customer-facing in real-time** — no immediate user impact, but billing impact accumulates. Mitigation is investigation-led, not action-led, in the first 5 min.

1. **Confirm signal**: is degradation across all tenants (systemic) or single tenant (workload change)?
   ```promql
   topk(10, (1 - rate(corelink_dedup_bytes_saved_total{type="chunk"}[7d])
                  / rate(corelink_dedup_bytes_uploaded_total[7d])))
       by (tenant_id)
   ```
2. **Check chunker version**: any deploy in last 7d that changed chunking algo? `corelink_chunker_version` panel.
3. **If systemic** (all tenants affected simultaneously):
   - Suspect chunker regression (FM-200) → consider rollback of last chunker change.
   - Suspect AC TTL regression (FM-AC-TTL-DRIFT) → see `RB-FM-AC-TTL-DRIFT`.
4. **If single tenant** (one outlier):
   - Workload genuinely changed (e.g., adopted random encryption or compression upstream).
   - Reach out to customer CSM; no immediate action.

## Root-cause investigation (15–30 min)

1. **Chunker behavior**:
   - Distribution of chunk sizes: `histogram_quantile(0.5, ...)` should be ~16–64 KiB.
   - Average chunks per blob: should be ≥ 4 (for blobs > 64 KiB).
   - Cross-blob dedup ratio: `corelink_dedup_intra_tenant_ratio` vs baseline.
2. **AC churn check**: if AC entries are being invalidated too aggressively (FM-AC-TTL-STORM), CAS gets fewer hits → ratio degrades. See `RB-FM-AC-TTL-STORM`.
3. **GC behavior**: if GC is sweeping too eagerly (FM-305), chunks get deleted before re-reference. Check `corelink_gc_premature_sweep_total`.
4. **Workload analysis** (per affected tenant):
   - Are uploads landing in same path prefixes / digests?
   - Has client integration changed compression / encryption layer?
   - Compare top-10 digests this week vs last week.
5. **Compare to benchmarks**:
   - BuildBuddy baseline ~2.8×; NativeLink ~2.1×; our target SOTA 3.0×.
   - If we dropped below 2.1× systemically → competitive disaster.

## Rollback / recovery

| Cause                                      | Action                                                                         |
|--------------------------------------------|--------------------------------------------------------------------------------|
| Chunker version regression                 | `wrangler rollback --env prod --service worker-cp` (chunker is in same worker)|
| AC TTL drift / storm                       | Follow `RB-FM-AC-TTL-DRIFT` / `RB-FM-AC-TTL-STORM`                            |
| GC premature sweep (FM-305)                | Increase grace period (`gc_grace_secs` env); follow `RB-FM-305`                |
| Workload change (legitimate)               | No action; document in CSM notes; revisit SLO target if pattern persists       |
| Chunk-size distribution skewed (regression)| Re-tune chunker parameters; rollout via PAT-PROGRESSIVE-ROLLOUT-001            |

Verify recovery:

```promql
# 7d ratio returns to ≥ target for the affected tier
rate(corelink_dedup_bytes_saved_total{type="chunk"}[7d])
  / rate(corelink_dedup_bytes_uploaded_total[7d]) >= 2.5  # team baseline
```

## Escalation path

| Time elapsed | Who                                | Criteria                                    |
|--------------|------------------------------------|---------------------------------------------|
| 0            | Primary SRE (Slack `#sre-oncall`)  | SEV-3 fires (24h fast-burn 10× rate)        |
| 0            | Primary SRE (PagerDuty)            | SEV-2 fires (1h fast-burn 10× rate)         |
| 4h (no mitigation) | Secondary SRE + WI-S07 owner | Investigation stalled                        |
| 24h          | Product Owner + Finance            | Sustained breach impacting billing forecast |
| 7d           | VP Engineering                     | Sustained > 1 week = revenue/cost mismatch  |

**Comms template (internal Slack `#sre-oncall`):**

```
SEV-{2|3} — Dedup ratio degradation on tier {tier}
Observed: {ratio}× over 7d (target {target}×, gap {%}).
Top-3 affected tenants: {tenant_ids redacted}.
Investigation owner: @{handle}. Next update: +1h.
```

No customer-facing comms unless: (a) sustained > 30 days, OR (b) customer raises directly (CSM-owned).

## Post-incident

Capture:

- Ratio time-series before/during/after.
- Top-10 affected tenants + per-tenant baseline diff.
- Chunker version delta (if regression).
- Customer billing impact estimate (Finance).
- Was anomaly detection (30% WoW drop) the first signal or did burn-rate catch first?
- Update SLO targets if persistent shift across the fleet (requires ADR per §6.1 slo_catalog).

## Related

- **SLO:** SLO-DEDUP-RATIO (§4.8.1).
- **Alerts:** `corelink-slo-dedup-ratio-fast-burn` (SEV-2 1h), `-slow-burn` (SEV-3 6h), `-anomaly` (SEV-3 30% WoW).
- **FMs:** FM-200 (deploy regression), FM-305 (tombstone lost), FM-AC-TTL-DRIFT, FM-AC-TTL-STORM.
- **Patterns:** PAT-PROGRESSIVE-ROLLOUT-001, PAT-GC-HEALTHCHECK-001.
- **ADRs:** ADR-0019 (TTL ownership S04/S07), ADR-0022 (chunk size vs part size), ADR-0039 (chunker public API stability).
- **Dashboards:** `DASH-CAS` (dedup panel), `DASH-GLOBAL-PRODUCT`, `DASH-COST`.
- **Sister runbooks:** `RB-FM-305`, `RB-FM-AC-TTL-DRIFT`, `RB-FM-AC-TTL-STORM`.
- **WI:** WI-S07-001 (lookup), WI-S07-005 (dashboard).
- **Drill cadence:** quarterly (synthetic workload with known dedup baseline).

---

**Fim RB-SLO-DEDUP-DEGRADATION.**
