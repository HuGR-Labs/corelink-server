---
id: "SPEC-CONTRACT-S14"
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
tags: ["spec-contract", "s14", "region", "failover", "byok", "high-risk"]
---

# Spec Contract — S-14: Region Expansion + Cross-Region Failover + BYOK (Enterprise)

## 0. Metadata

| Sprint ID | S-14 | Lane | HIGH_RISK |
|---|---|---|---|
| Duração | 4 semanas | WIs | 8 |
| Forcing factors | FF-HR-002 (cross-region tenant isolation), FF-HR-003 (residency PII), FF-HR-005 (BYOK), FF-HR-008 (vendor lock-in) |

## 1. Objetivo

Expandir para as 4 regiões target (WNAM, ENAM, WEUR, SAM; APAC/AFR deferred) + implementar region failover pra reads (PAT-REGION-FAILOVER-001) + habilitar BYOK enterprise (AWS KMS / GCP KMS / Azure Key Vault / Vault). Critical pra aumentar audiência enterprise e FedRAMP-ready.

## 2. Lane + forcing factors

- **Lane:** HIGH_RISK
- **FF-HR-002**: cross-region keys/queries podem vazar tenant data.
- **FF-HR-003**: residency EU crítica para GDPR.
- **FF-HR-005**: BYOK introduz novo vetor de controle crypto (CTRL-KEY-010..012).
- **FF-HR-008**: multi-cloud BYOK introduz vendor lock-in risk.

## 3. Inherits_from

```yaml
inherits_from:
  - "KEY-MANAGEMENT"
  - "SECURITY-MODEL"
  - "PRIVACY-MODEL"
  - "STORAGE-SEMANTICS-MATRIX"
  - "COMPLIANCE-MATRIX"
  - "RESILIENCE-PATTERNS"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
  - "SLO-CATALOG"
```

## 4. CAPs entregues

- **CAP-REGION-001**: 4 regiões operantes (WNAM, ENAM, WEUR, SAM).
- **CAP-REGION-002**: Tenant pinning por região + cross-region restrict.
- **CAP-REGION-003**: Read failover via PAT-REGION-FAILOVER-001 (top 1% hot blobs).
- **CAP-BYOK-001**: BYOK AWS KMS integration.
- **CAP-BYOK-002**: BYOK GCP KMS integration.
- **CAP-BYOK-003**: BYOK Azure Key Vault integration.
- **CAP-BYOK-004**: BYOK HashiCorp Vault integration.
- **CAP-BYOK-005**: Customer kill switch (revoke access → cache inacessível em ≤ 5 min).

## 5. Requirements específicos

- **R-S14-1**: Infrastructure: R2 buckets + D1 instances em 4 regiões.
- **R-S14-2**: Tenant `primary_region` enforcement no write path.
- **R-S14-3**: Hot blob replica detector (top 1% via métrica) + replication worker.
- **R-S14-4**: BYOK adapter trait + 4 implementations (AWS/GCP/Azure/Vault).
- **R-S14-5**: CMK revocation detection → auto degrade mode read-only pra tenant.
- **R-S14-6**: Erasure attestation signed with Ed25519 (CTRL-KEY-015).

## 6. DoD

- [ ] 8 WIs SEALED.
- [ ] 4 regions live + chaos test region outage.
- [ ] Tenant EU: blob lands em WEUR, nunca ENAM (residency test).
- [ ] BYOK AWS KMS E2E: customer CMK → wrap DEK → CAS write → unwrap no read.
- [ ] Kill switch test: revoke CMK → cache 503 em < 5 min.
- [ ] SLO sustained during region failover chaos.

## 7. Completeness (delta)

- [ ] **10.s14.1** Replication lag < 60s p99 para hot blobs.
- [ ] **10.s14.2** Residency contract: DPA amendment + Schrems II TIA template.

## 8. Invariants

- INV-DATA-RESIDENCY (HIGH): tenant region pinned + enforced.
- INV-TENANT-ISOLATION: mantém cross-region.
- INV-KEY-NO-SKIP: BYOK revoke → writes fail corretamente.

## 9. Quality Standards

- Region failover transparent ao cliente (SLO preserved).
- BYOK latency overhead < 30ms p99.
- Multi-cloud BYOK matrix tested em staging.

## 10. Anti-scope

- ❌ APAC/AFR regions (pós-GA demand-driven).
- ❌ Active-active multi-region writes (primary-only em S-14).
- ❌ BYOE (Bring Your Own Encryption) — Fase 2.

## 11. Dependencies

- Blocker: S-01..S-10 (core product).
- Blocker: S-11 (residency tooling).
- Blocker: S-13 (admin plane pra config flags per-region).

## 12. WIs antecipados

| ID | Título |
|---|---|
| WI-S14-001 | R2+D1 provisioning 4 regions + migration |
| WI-S14-002 | Tenant region pinning enforcement |
| WI-S14-003 | Hot blob replica worker (PAT-REGION-FAILOVER-001) |
| WI-S14-004 | BYOK AWS KMS adapter |
| WI-S14-005 | BYOK GCP/Azure/Vault adapters |
| WI-S14-006 | CMK revocation detection + kill switch |
| WI-S14-007 | Erasure attestation signing |
| WI-S14-008 | RB-BYOK-REVOKE dry-run + DPA amendment |

## 13. Duração

4 semanas; buffer 10 dias (multi-cloud integrations unpredictable).

## 14. Critérios de promoção

- DoD + pentest BYOK flow + DPA signed com 1 enterprise customer de teste.

## 15. Riscos

| Risco | Prob | Impacto |
|---|---|---|
| BYOK API drift entre clouds | H | MEDIUM |
| Residency leak via KV global (FM-054) | M | HIGH |
| Region replication diverge (FM-105) | M | MEDIUM |
| Customer CMK-off causa false SEV-1 | M | LOW (expected behavior) |

---
