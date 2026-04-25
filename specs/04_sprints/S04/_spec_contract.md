---
id: "SPEC-CONTRACT-S04"
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
tags: ["spec-contract", "s04", "action-cache", "reapi", "high-risk"]
---

# Spec Contract — S-04: Action Cache (AC)

## 0. Metadata

| Campo | Valor | |
|---|---|---|
| Sprint ID | S-04 | |
| Nome | Action Cache (GetActionResult + UpdateActionResult + Merkle verify) | |
| Lane | HIGH_RISK | |
| Lane forcing factors | FF-HR-002, FF-HR-005 | |
| Duração estimada | 3 semanas | |
| WIs antecipados | 6 | |

## 1. Objetivo

Implementar Action Cache (AC) — a "cereja" do remote cache Bazel/Buck2: dado hash de action determinística, retorna resultados de execução previamente computada. Requer integridade Merkle + tenant-scoped + TTL management. Sem AC, CoreLink é só blob store; com AC, é remote cache real que acelera builds.

## 2. Lane + forcing factors

- **Lane:** HIGH_RISK
- **FF-HR-002**: AC cross-tenant = serve build result de outro tenant = catastrófico (cenário FM-303).
- **FF-HR-005**: implementa CTRL-AC-001 (Merkle verify) + CTRL-AC-002 (digest signing).

## 3. Inherits_from

```yaml
inherits_from:
  - "REMOTE-CACHE-PRODUCT-PROFILE"
  - "SECURITY-MODEL"
  - "DATA-MODEL"
  - "INVARIANT-REGISTRY"
  - "KEY-MANAGEMENT"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "SLO-CATALOG"
```

## 4. CAPs entregues

- **CAP-AC-001**: REAPI GetActionResult (cache hit)
- **CAP-AC-002**: REAPI UpdateActionResult (cache write)
- **CAP-AC-003**: Merkle verification server-side + client-side
- **CAP-AC-004**: AC TTL management **infrastructure** (TTL worker + refresh-on-hit + expiry detection). **Default value supersedido por S-07 CAP-EVICT-002 per-tier table** (ADR-0019); S-04 entrega a infra de TTL, não o policy de defaults por tier. Free-tier customers em pre-S-07 staging usam 90d default; pós-S-07 SEALED, defaults migram per ADR-0019 §migration-plan.
- **CAP-AC-005**: AC entry invalidation (cliente-requested + admin override)

## 5. Requirements específicos

- **R-S04-1**: REAPI `ActionCache::GetActionResult` + `UpdateActionResult` handlers.
- **R-S04-2**: D1 schema `ac_meta` (conforme `data_model.md §4.2`) + R2 bucket `ac-<region>`.
- **R-S04-3**: Crate `corelink-ac` com ActionResult proto codec + Merkle tree builder/verifier.
- **R-S04-4**: AC digest signing via CTRL-AC-002 (HKDF tenant key info="ac-sig").
- **R-S04-5**: TTL worker (cron DO) que expira entries > `expires_at`.
- **R-S04-6**: Metrics + dashboards (DASH-AC conforme `observability_model §8`).

## 6. Definition of Done

- [ ] 6 WIs SEALED.
- [ ] E2E: Bazel build com `--remote_cache=corelink://...` → cache hit reduz tempo em > 50% em rebuilds.
- [ ] Property test: AC entry de Tenant A nunca retornada a Tenant B.
- [ ] TLA+ invariantes verdes.
- [ ] Chaos: TTL expiry sob load; stale read handling.
- [ ] SLO-AVAIL-AC e SLO-LAT-AC-HIT sustained 72h staging.
- [ ] PRR + adversarial review.

## 7. Completeness Criteria (delta local)

- [ ] **10.s04.1** REAPI v2 conformance test suite (bazelbuild/remote-apis) passa para AC ops.
- [ ] **10.s04.2** Merkle tree verify: invalid tree rejeitada no read path + write path.
- [ ] **10.s04.3** AC hit ratio > 70% em workload de test Bazel sintético.

## 8. Invariants

- INV-AC-OUTPUTS-VALID (HIGH): outputs referenciados existem em blob_meta alive.
- INV-AC-TENANT-SCOPED (CRITICAL, TLA+): deriva de INV-TENANT-ISOLATION.
- INV-CAS-IMMUTABILITY: AC entry é imutável pós-write (atualização = nova entrada).

## 9. Quality Standards (delta local)

- **14.s04.1** Merkle verify p99 ≤ 10ms pra trees ≤ 100 nodes.
- **14.s04.2** AC GetActionResult p99 ≤ 150ms (SLO-LAT-AC-HIT).
- **14.s04.3** Cache hit ratio emitido como métrica business (`corelink_cache_hit_ratio{type=ac}`).

## 10. Anti-scope

- ❌ Execute Action (Fase 2 — Remote Execution).
- ❌ AC cross-tenant dedup (requer ADR — bloqueado).
- ❌ AC Content-Delivery Network (CDN layer em S-14).

## 11. Dependencies

- **Blocker:** S-01 SEALED (CAS write path).
- **Blocker:** S-02 SEALED (read path para output blobs referenciados).
- **Blocker:** S-03 SEALED (auth tenant ctx).

## 12. WIs antecipados

| ID | Título |
|---|---|
| WI-S04-001 | REAPI ActionCache proto handlers |
| WI-S04-002 | D1 ac_meta + R2 ac bucket + schema |
| WI-S04-003 | Crate corelink-ac (Merkle codec + verify) |
| WI-S04-004 | CTRL-AC-002 digest signing (HKDF + Ed25519 opt) |
| WI-S04-005 | TTL worker (cron DO + expiry job) |
| WI-S04-006 | REAPI conformance tests + DASH-AC dashboards |

## 13. Duração

3 semanas; buffer 5 dias.

## 14. Critérios de promoção

- DoD complete.
- Bazel conformance passa.
- Cache hit ratio measurable.
- PRR.

## 15. Riscos

| Risco | Prob | Impacto |
|---|---|---|
| Merkle tree invalid detected late (após persist) | M | HIGH (stale AC entries) |
| AC TTL too aggressive → hit ratio baixo | M | MEDIUM (cliente perception) |
| AC tenant leak via bug de digest computation | L | CRITICAL (FM-303) |
| REAPI v2 spec interpretation divergente de Bazel | M | MEDIUM (rework) |

---

**Fim spec contract S-04.**
