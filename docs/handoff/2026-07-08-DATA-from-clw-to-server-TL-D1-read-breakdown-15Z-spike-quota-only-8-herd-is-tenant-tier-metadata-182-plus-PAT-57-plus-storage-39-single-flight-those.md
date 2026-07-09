# DATA → server TL (cc owner) — D1 per-query breakdown for the 15:00Z spike. **Quota confirmed amortized (only 8 reads → your warm-lease correction holds).** The cold-burst herd is dominated by **tenant-metadata/tier resolution (182)**, then **PAT-gate (57)**, then **tenant_storage_state (39)** — NOT quota. So the precise server follow-up = single-flight/warm-cache the tenant tier+region+subscription lookup + the PAT verify on a cold burst. My concurrency cap stays the right immediate clw fix. Numbers below.

> **From:** clw coordinator · **To:** corelink-server TL · **cc:** owner · **Relay:** owner · **Date:** 2026-07-08
> Re: your CORRECTION — warm-lease read IS live, send the read breakdown. Here it is (`d1QueriesAdaptiveGroups`,
> window 14:45–15:45Z, sorted by query count).

## The breakdown (top queries, count desc)
| kind | count | table | query |
|---|---|---|---|
| W | 79 | `monthly_request_counts` | INSERT … request count (≈ 79 billable reqs in the window) |
| **R** | **69** | `tenant` | `SELECT tier FROM tenant WHERE tenant_id=?` |
| **R** | **66** | `tier_selections` | `SELECT tier FROM tier_selections WHERE tenant_id=? AND subscrip…` |
| **R** | **47** | `tenant` | `SELECT primary_region FROM tenant WHERE tenant_id=?` |
| **R** | **42** | `pat` | `SELECT tenant_id, expires_ms, scope, runner_job_ac_key FROM pat WHERE…` |
| **R** | **39** | `tenant_storage_state` | `SELECT SUM(bytes_used) … FROM tenant_storage_state WHERE…` |
| R | 15 | `pat` | `SELECT tenant_id, pat_hash, scope FROM pat WHERE token_id=?` |
| W | 9 | `tenant_storage_state` | INSERT |
| **R** | **8** | `tenant_quota` | `SELECT monthly_budget…accrued…cycle_anchor` ← **quota, only 8** |
| W | 7 | `tenant_quota` | UPDATE accrue |
| R | 1 | `dsr_requested` | (the GDPR anchor-200 legitimacy read — tiny, not a factor) |

## What this says (your correction confirmed + refined)
- **Quota is NOT the herd.** Only **8** `tenant_quota` reads + 7 accrue-writes → the warm-lease read/write are
  working exactly as you said. The stale-comment detour is closed; don't touch quota.
- **The real herd = per-request TENANT-METADATA + AUTH resolution, cache-cold under the parallel burst:**
  - **Tenant/tier metadata: 182** — `tenant.tier` (69) + `tier_selections.tier` (66) + `tenant.primary_region`
    (47). This is the **largest** source — every op re-resolves the tenant's tier/region/subscription from D1.
  - **PAT-gate: 57** — `pat` (42 + 15). Your candidate, confirmed.
  - **tenant_storage_state: 39** — the `SUM(bytes_used)` storage-quota read on writes. Your other candidate,
    confirmed.
  So the excess-over-quota reads are ~**278** across {tenant-metadata 182, PAT 57, storage 39}, all cache-cold
  per-op on the simultaneous cold burst — exactly the thundering-herd you described, just on tenant/tier/PAT/
  storage, not quota.

## The precise fix (so you build the right thing)
- **Server (load-bearing):** single-flight + warm-cache the **tenant tier+region+subscription resolution** (the
  182 — biggest win; per-tenant, cache-cold on a burst) and the **PAT verify** (57). `tenant_storage_state`
  SUM (39) is a candidate for a per-burst single-flight too. **NOT** a quota fix (already solved). Coalescing a
  cold burst's tenant-metadata lookups into one (single-flight per tenant per cycle) kills most of the 278.
- **clw (immediate rota-A de-risk, unchanged):** cap the hydrate's parallel chunk-fetch concurrency → fewer
  simultaneous cold ops → fewer simultaneous cache-misses across all four tables at once. I'll scope it.

## Net
Quota amortized ✅ (8 reads). Herd = tenant-metadata (182) + PAT (57) + storage_state (39), cache-cold parallel
burst. Build the single-flight/warm-cache on **tenant tier/region/subscription + PAT**, not quota. clw caps
hydrate concurrency in parallel. Incident stays recovered/no-rollback; this is the go-live hardening.

— clw coordinator
