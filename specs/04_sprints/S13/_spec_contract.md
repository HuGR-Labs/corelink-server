---
id: "SPEC-CONTRACT-S13"
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
tags: ["spec-contract", "s13", "admin-plane", "config", "standard"]
---

# Spec Contract — S-13: Admin Plane (Config + Feature Flags + Secret Rotation)

## 0. Metadata

| Sprint ID | S-13 | Lane | STANDARD |
|---|---|---|---|
| Duração | 2 semanas | WIs | 5 |

## 1. Objetivo

Implementar admin plane: DO `config-singleton` para feature flags + policies; dual-approval workflow para destructive ops (PAT-DUAL-APPROVAL-001); secret rotation automation (PAT-ROLL-FORWARD-001); terraform drift detection (PAT-DRIFT-DETECTION-001); progressive rollout com auto-rollback.

## 2. Lane + forcing factors

- **Lane:** STANDARD (operações internas mas precisam rigor).

## 3. Inherits_from

```yaml
inherits_from:
  - "RESILIENCE-PATTERNS"
  - "SECURITY-MODEL"
  - "KEY-MANAGEMENT"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
```

## 4. CAPs entregues

- **CAP-ADMIN-001**: DO config-singleton (feature flags, rate limits tunáveis, retention).
- **CAP-ADMIN-002**: Dual-approval workflow (PAT-DUAL-APPROVAL-001 enforcement).
- **CAP-ADMIN-003**: Secret rotation automation (CTRL-CRED-003 + PAT-ROLL-FORWARD-001).
- **CAP-ADMIN-004**: Terraform drift detection daily (PAT-DRIFT-DETECTION-001).
- **CAP-ADMIN-005**: Progressive rollout orchestrator (PAT-PROGRESSIVE-ROLLOUT-001).

## 5. Requirements específicos

- **R-S13-1**: DO `config-singleton` com flags tipados + CAS version-aware updates.
- **R-S13-2**: Admin endpoint `POST /v1/admin/ops` + dual-approval check.
- **R-S13-3**: Secret rotation worker (TDKs, PAT signing keys, audit chain key).
- **R-S13-4**: Terraform plan daily in CI; diff alert em drift.
- **R-S13-5**: Progressive rollout controller (1% → 10% → 50% → 100%) + error budget burn auto-rollback.

## 6. DoD

- [ ] 5 WIs SEALED.
- [ ] Feature flag: toggle em DO reflete em ≤ 5s edge globalmente.
- [ ] Dual-approval: admin op sem 2 assinaturas é bloqueada + logged.
- [ ] Secret rotation: 1 TDK rotated em staging sem downtime.
- [ ] Terraform drift: injected drift é detected em próximo daily run.
- [ ] Progressive rollout: bad deploy auto-rollback em < 10 min.

## 7. Completeness (delta)

- [ ] **10.s13.1** CTRL-AUDIT-003 (MFA attestation admin ops) enforced.
- [ ] **10.s13.2** RB-FM-205 (admin mistake) dry-run.

## 8. Invariants

- CTRL-AUTH-010 + CTRL-AUDIT-003: admin ops require MFA + audit rich event.
- INV-KEY-OVERLAP (key_management §3.2): rotation overlap period respected.

## 9. Quality Standards

- Config mudanças versionadas (git).
- Dual-approval é hard-check (não social norm).
- Rollback p99 ≤ 10 min.

## 10. Anti-scope

- ❌ UI admin panel (S-16).
- ❌ Customer-facing feature flags (out of scope; internal only).

## 11. Dependencies

- Blocker: S-03 (auth real) + S-09 (observability).

## 12. WIs antecipados

| ID | Título |
|---|---|
| WI-S13-001 | DO config-singleton + flag CRUD |
| WI-S13-002 | Admin API + dual-approval |
| WI-S13-003 | Secret rotation worker |
| WI-S13-004 | Terraform drift detection |
| WI-S13-005 | Progressive rollout controller |

## 13. Duração

2 semanas; buffer 3 dias.

## 14. Critérios de promoção

- DoD + 3 admin ops reais executados sem incident.

## 15. Riscos

| Risco | Prob | Impacto |
|---|---|---|
| Config flag race condition | M | MEDIUM |
| Dual-approval bypassed via bug | L | CRITICAL |
| Secret rotation quebra em-flight (FM-204) | M | HIGH |
| Auto-rollback falha → bad deploy permanece | L | HIGH |

---
