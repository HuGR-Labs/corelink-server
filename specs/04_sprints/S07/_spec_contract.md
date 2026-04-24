---
id: "SPEC-CONTRACT-S07"
type: "spec_contract"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-24"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["spec-contract", "s07", "dedup", "eviction", "standard"]
---

# Spec Contract — S-07: Dedup + Eviction Policy

## 0. Metadata

| Sprint ID | S-07 | Lane | STANDARD |
|---|---|---|---|
| Duração | 2.5 semanas | WIs | 5 |

## 1. Objetivo

Implementar dedup (tenant-local default) e eviction policy per-tier (LRU + TTL + quota-based). Reduz storage cost + melhora hit ratio. Cross-tenant dedup fica em backlog (requer BYOE + ADR).

## 2. Lane + forcing factors

- **Lane:** STANDARD. Não toca invariantes CRITICAL diretamente.

## 3. Inherits_from

```yaml
inherits_from:
  - "REMOTE-CACHE-PRODUCT-PROFILE"
  - "DATA-MODEL"
  - "STORAGE-SEMANTICS-MATRIX"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "SLO-CATALOG"
```

## 4. CAPs entregues

- **CAP-DEDUP-001**: Intra-tenant chunk dedup (via manifest_chunks table).
- **CAP-DEDUP-002**: Dedup ratio métrica `corelink_dedup_ratio{tenant_id}`.
- **CAP-EVICT-001**: LRU eviction per-tier (limits conforme `remote_cache_product_profile.md §8.2`).
- **CAP-EVICT-002**: TTL-based AC entry expiry (default 30d-365d tier-dependent).
- **CAP-EVICT-003**: Quota-based write rejection (95% quota → warn; 100% → reject).

## 5. Requirements específicos

- **R-S07-1**: Dedup index `manifest_chunks` indexado por `(tenant_id, chunk_digest)`.
- **R-S07-2**: Eviction worker daily + 95% quota trigger.
- **R-S07-3**: LRU tracking via `blob_meta.last_accessed_at` (update on hit).
- **R-S07-4**: REAPI `FindMissingBlobs` otimizado pra detectar chunks existentes (reduz upload).
- **R-S07-5**: Dashboard `DASH-DEDUP` + alerta anomaly detection.

## 6. DoD

- [ ] 5 WIs SEALED.
- [ ] Dedup ratio mensurável em 5 tenants de teste; target ≥ 2× para Docker-like workload.
- [ ] Eviction não deleta reachable (herda INV-GC-001 enforcement).
- [ ] Quota soft-limit (95%) dispara alerta; hard-limit (100%) rejeita write com erro tipado.

## 7. Completeness (delta)

- [ ] **10.s07.1** Quota enforcement per-tier sustained 24h staging.
- [ ] **10.s07.2** Dedup hit ratio > baseline em workload real (Docker pulls).

## 8. Invariants

- INV-TENANT-ISOLATION: dedup intra-tenant apenas; chunk index scoped.
- INV-QUOTA-ENFORCEMENT (HIGH): tenant não ultrapassa quota.

## 9. Quality Standards

- Zero overhead dedup path no hot path (check O(1) via D1 index).
- Eviction batch jitter ±10% pra evitar thundering herd.

## 10. Anti-scope

- ❌ Cross-tenant dedup (backlog S-XX pós-GA).
- ❌ Content-defined chunking FastCDC (S-05 já estabeleceu fixed 2 MiB).

## 11. Dependencies

- Blocker: S-05 (chunking) + S-06 (GC).

## 12. WIs antecipados

| ID | Título |
|---|---|
| WI-S07-001 | Dedup index manifest_chunks query optimization |
| WI-S07-002 | Eviction worker (LRU + TTL + quota) |
| WI-S07-003 | Quota enforcement middleware |
| WI-S07-004 | FindMissingBlobs optimization |
| WI-S07-005 | DASH-DEDUP + alertas |

## 13. Duração

2.5 semanas; buffer 3 dias.

## 14. Critérios de promoção

- Dedup ratio measurable.
- Quota enforcement working end-to-end.

## 15. Riscos

| Risco | Prob | Impacto |
|---|---|---|
| Quota race condition (FM-059) | M | MEDIUM |
| Eviction deleta chunk ainda referenciado | L | HIGH (mitigated by INV-GC-001) |
| Dedup ratio baixo por workload não-friendly | M | LOW (métrica, não deployment) |

---
