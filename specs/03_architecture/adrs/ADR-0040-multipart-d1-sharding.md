---
id: "ADR-0040"
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
tags: ["adr", "multipart", "d1", "sharding", "scaling", "s05"]
---

# ADR-0040 — Multipart D1 Sharding Strategy: Per-Region Sharding com Sharding Trigger 80% D1 Hard Limit

## Status

ACCEPTED (S-05 WI-S05-004 ratificada em Lote 10.5bis P0 fix; previously procedural rubber-stamp risk flagged em Agent R4 review s05-part2 P0 #3 — sharding strategy not designed at sprint shipping).

## Context

WI-S05-004 D1 schema `chunks` projects 250M rows × 150 bytes ≈ 37 GB at S-05 GA target workload (10M chunked blobs × 25 chunks/blob avg × 100 tenants). **D1 hard limit per database é 10 GB** (Cloudflare current policy as of 2026; subject to change). Sharding mandatory antes de exceder threshold; this ADR defines:

1. Sharding key (per-region OR per-tenant_tier).
2. Sharding trigger threshold.
3. Migration plan first-shard-split.
4. Cross-shard query semantics.

## Decision

**Sharding key: per-region (5 shards: sam, iad, lhr, nrt, syd).**

**Rationale**:
- **Aligned com R2 bucket region attribution** — each region's chunks live em corelink-chunk-<region>; D1 shard naturally co-located.
- **Cross-tenant load distribution** — large tenants distribute across regions per their workload; single-tenant burst em one region won't saturate other shards.
- **Operational simplicity** — 5 shards é manageable; per-tenant_tier (3-5 tiers) similar count but tier upgrades require row migration; per-region é fixed.
- **Graviton/Intel mix nas regions** — D1 latency varies per CF datacenter; per-region sharding aligns latency profile.
- **Tenant analytics queries** — most analytics são per-tenant per-region anyway (e.g., "chunks count for tenant X em region sam").

**Rejected alternatives**:
- **Per-tenant_tier (free/solo/team/business/enterprise)**: tier upgrade requires row migration cross-shard; migration window during upgrade events is operational pain; rejected.
- **Per-tenant (one shard per tenant)**: explodes shard count linearly with tenants (100 tenants = 100 shards); operational nightmare; rejected unless single-tenant data residency requirement (BYOK S-14 forward).
- **Hash-based sharding (modulo on tenant_id)**: cross-region queries become multi-shard; complexity outweighs benefit at S-05 scale; rejected.

## Sharding trigger

**Sharding mandatory before any single shard reaches 80% of D1 10 GB hard limit (= 8 GB ≈ 30M chunks rows)**.

**Storage growth alert**:
- Metric `corelink.d1.chunks.size_bytes{region}` per shard.
- Alert PD-WARNING at 50% (5 GB) — capacity planning.
- Alert PD-CRITICAL at 80% (8 GB) — sharding migration trigger; ETA 30 days to next shard split.
- HARD STOP at 95% (9.5 GB) — INSERT rejected; tenant migration mandatory.

## Migration plan (first-shard-split)

When per-region shard reaches 80% threshold, sub-shard the region:

1. **D-30**: PD-CRITICAL alert; sprint-level capacity planning meeting.
2. **D-25 to D-15**: design sub-shard strategy (typically per-tenant_tier within region: e.g., region=sam splits into sam-business, sam-enterprise, sam-free+solo+team).
3. **D-15 to D-10**: provision new D1 instances per sub-shard.
4. **D-10 to D-5**: dual-write window (handler writes to BOTH old + new shard; reads from old; reconcile job migrates historical rows). **Lote 10.5-tris P0-SR5-004 fix — reconcile job scope explicit**: includes BOTH `cas_blobs` + `chunks` (historical) AND `multipart_sessions` (LIVE table; in_progress sessions migrated; orphan-detection blind spot closed). `multipart_sessions` rows with `state='in_progress'` AND `created_at_ms < dual_write_start_ms` are migrated to new shard before D-5 read cutover; sweeper sees all in-flight sessions post-cutover.
5. **D-5**: read cutover; reads now from new sub-shards.
6. **D-3**: dual-write disabled; old shard read-only; verification window 72h.
7. **D-0**: old shard archived; chunks rows moved.

**Migration tooling**: `scripts/d1_shard_migrate.py` orchestrates dual-write + reconcile + cutover; integration tested in staging.

### §A1 — `multipart_sessions` cross-shard migration discipline (Lote 10.5-tris P0-SR5-004)

**Issue (Sonnet R5)**: `multipart_sessions` is a LIVE table (NOT historical). In-progress sessions during dual-write window may be invisible to sweeper post-cutover IF the reconcile job migrates only historical tables. Result: orphan-detection blind spot during shard-split transitions; INV-MULTIPART-ORPHAN-DETECTABLE breached during 5-day window.

**Resolution**:
1. **Migration job scope**: explicitly INCLUDES `multipart_sessions` (rows with `state='in_progress'`); NOT only `cas_blobs` + `chunks`.
2. **Migration ordering** (D-10 to D-5):
   - D-10: dual-write begins; handler writes new sessions to BOTH shards.
   - D-9: reconcile job kicks off; for each `multipart_sessions` row in old shard with `state='in_progress'`: COPY to new shard (preserving session_id, upload_id, created_at_ms, last_activity_at_ms, state).
   - D-7: reconcile complete; verify count match (old.in_progress = new.in_progress).
   - D-5: read cutover; sweeper post-cutover sees all sessions in new shard.
3. **Sweeper multi-shard awareness** (cross-ref WI-S05-006 §6.1.1 sweeper):
   - During dual-write window (D-10 to D-3), sweeper queries BOTH old and new shards; deduplicates by session_id; aborts session on either shard.
   - R2 AbortMultipartUpload returns 404 on already-aborted upload_id → handled gracefully (not error).
4. **Chaos test** (cross-ref WI-S05-006 §6.1.x): "Mid-shard-split orphan detection: session S started pre-dual-write; sweeper fires post-read-cutover; assert S detected and aborted within 7d SLA".
5. **Invariant** INV-MULTIPART-ORPHAN-DETECTABLE updated em registry §3.16: "Guarantee holds across D1 shard splits; sweeper multi-shard aware during split transitions (D-10 to D-3 dual-write window)".

## Cross-shard query semantics

- **Default**: handler queries single shard (region attribution from blob row).
- **Analytics (DASH-MULTIPART)**: aggregates across shards via S-09 OLAP pipeline (writes to BigQuery / ClickHouse forward); D1 shards are NOT joined directly em hot path.
- **DSR cascade (S-11 forward)**: tenant DELETE traverses all shards; bounded by tenant region attribution (most tenants single-region).

## Consequences

**Positive**:
- D1 storage scales to 5 × 10 GB = 50 GB at S-05 GA; 5 × 8 GB threshold = 40 GB before sub-sharding.
- Region-aligned operationally simple.
- Sharding trigger explicit (80% threshold; alert-driven).

**Negative**:
- Cross-region queries forward to S-09 OLAP pipeline (additional infrastructure).
- Multi-region tenant (rare; S-14 forward) has distributed chunks across shards.

**Neutral**:
- Aligned com WI-S04-002 ac_meta single-shard projection (only 2.7 GB at S-04 GA; sharding deferred to S-09 forward).

## References

- WI-S05-004 §22 storage projection (250M rows / 37 GB).
- WI-S05-006 §6.1.9 ratificação confirmation.
- ADR-0036 (AC schema migration governance) — sharding criteria pattern reuse.
- Cloudflare D1 documentation — hard limit per database.

## Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (via Claude Opus 4.7) | Criação ADR-0040 (Lote 10.5bis P0 fix #3: design content authored; previously rubber-stamp risk per Agent R4 review). Per-region sharding decision; 80% threshold; migration plan dual-write + reconcile. |
| 1.1.0 | 2026-04-25 | Gustavo (Lote 10.5-tris P0-SR5-004 fix) | §A1 addendum: multipart_sessions cross-shard migration discipline; reconcile job scope explicit (in_progress sessions migrated D-10 to D-5; sweeper multi-shard aware during dual-write window); INV-MULTIPART-ORPHAN-DETECTABLE updated registry §3.16. Closes Sonnet R5 cross-shard orphan blind spot defect. |
