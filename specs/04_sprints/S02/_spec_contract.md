---
id: "SPEC-CONTRACT-S02"
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
tags: ["spec-contract", "s02", "cas", "read-path", "high-risk"]
---

# Spec Contract — S-02: CAS Read Path + Client Verify

## 0. Metadata

| Campo | Valor |
|---|---|
| Sprint ID | S-02 |
| Nome | CAS Read Path + Client Verify |
| Lane | HIGH_RISK |
| Lane forcing factors | FF-HR-002 (tenant isolation), FF-HR-005 (CTRL-CAS-002) |
| Duração estimada | 3 semanas |
| WIs antecipados | 6 |

## 1. Objetivo

Implementar path de leitura CAS com verify client-side obrigatório (CTRL-CAS-002), completando o loop write→read. Inclui REAPI `ByteStream::Read` + `GetBlob` + negative caching (404 result-caching) + integrity verify automático em clientes CoreLink.

## 2. Lane + forcing factors

- **Lane:** HIGH_RISK
- **FF-HR-002**: reads cross-tenant seriam catastróficos; mesma superfície do S-01.
- **FF-HR-005**: implementa CTRL-CAS-002 + CTRL-ISO-002 (AuthZ check on storage call).

## 3. Inherits_from

```yaml
inherits_from:
  - "SECURITY-MODEL"
  - "REMOTE-CACHE-PRODUCT-PROFILE"
  - "AUTH-MODEL"
  - "KEY-MANAGEMENT"
  - "INVARIANT-REGISTRY"
  - "DATA-MODEL"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "SLO-CATALOG"
```

## 4. CAPs entregues

- **CAP-CAS-004**: GET blob by digest (REAPI + HTTP).
- **CAP-CAS-005**: Client-side verify integrity (SDK defaults on).
- **CAP-CAS-006**: Streaming read (ByteStream chunked).
- **CAP-CAS-007**: Negative caching (404 com TTL curto).

## 5. Requirements específicos

- **R-S02-1**: REAPI `ByteStream::Read` Worker handler com streaming.
- **R-S02-2**: `GetBlob` unary (pequenos blobs ≤ 4 MB inline).
- **R-S02-3**: `FindMissingBlobs` para batch discovery.
- **R-S02-4**: Crate `corelink-client-verify` (lib SDK side) que verifica hash automaticamente pós-download.
- **R-S02-5**: Constant-time 404 vs 403 (CTRL-ISO-004) — prevent enumeration side-channel.
- **R-S02-6**: Negative cache KV `ac_neg:<digest>` com TTL 300s (PAT-KV-TTL-001).

## 6. Definition of Done

- [ ] 6 WIs SEALED.
- [ ] Property test: nenhum read retorna blob fora de namespace do tenant.
- [ ] TLA+ `tenant_isolation.tla` + `cas_integrity.tla` verdes em CI.
- [ ] Load test read 50k QPS × 10 min em staging.
- [ ] Client verify habilitado por default em SDK; opt-out documentado.
- [ ] Side-channel test: medir latência 404 vs 403 → p99 diff < 5ms.
- [ ] Runbook `RB-FM-253` dry-run.
- [ ] SBOM + signed release.

## 7. Completeness Criteria (delta local)

- [ ] **10.s02.1** E2E: write em S-01 → read em S-02 retorna body idêntico byte-a-byte.
- [ ] **10.s02.2** SLO-AVAIL-CAS-GET: 99.9% em staging sustained 72h.
- [ ] **10.s02.3** SLO-LAT-CAS-GET: 99% < 300ms (team target).

## 8. Invariants

- INV-TENANT-ISOLATION (CRITICAL, TLA+): read nunca retorna blob de outro tenant.
- INV-CAS-INTEGRITY (CRITICAL): client verify detecta bit rot/poisoning.
- INV-CAS-IMMUTABILITY: reads após GC soft-delete respeitam tombstone.

## 9. Quality Standards (delta local)

- **14.s02.1** Streaming memory bounded: Worker não carrega blob completo em memória; chunk-based.
- **14.s02.2** Cold read p99 ≤ 500ms; warm ≤ 100ms (cache hit).

## 10. Anti-scope

- ❌ Write path (S-01).
- ❌ AC reads (S-04 — AC tem read path próprio).
- ❌ Multi-region failover read (S-14).

## 11. Dependencies

- **Blocker:** S-01 SEALED.
- **Blocker:** S-00 roadmap.

## 12. WIs antecipados

| ID | Título |
|---|---|
| WI-S02-001 | REAPI ByteStream::Read handler |
| WI-S02-002 | GetBlob unary + FindMissingBlobs |
| WI-S02-003 | Crate client-verify (SDK lib) |
| WI-S02-004 | Constant-time 404/403 middleware |
| WI-S02-005 | Negative cache KV adapter |
| WI-S02-006 | Property test E2E write→read |

## 13. Duração

3 semanas; buffer 5 dias.

## 14. Critérios de promoção

- DoD complete.
- PRR approved.
- Read SLOs sustaining staging 72h.

## 15. Riscos

| Risco | Prob | Impacto |
|---|---|---|
| Side-channel timing vaza existência de blob | M | HIGH (THR-I-002) |
| Streaming memory leak em long reads | M | MEDIUM (FM-403) |
| SDK client verify default-off por bug | L | CRITICAL (CTRL-CAS-002 bypassed) |

---

**Fim spec contract S-02.**
