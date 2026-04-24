---
id: "SPEC-CONTRACT-S11"
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
tags: ["spec-contract", "s11", "privacy", "dsr", "lgpd", "gdpr", "high-risk"]
---

# Spec Contract — S-11: Privacy Pipeline (DSR + Erasure Automation)

## 0. Metadata

| Sprint ID | S-11 | Lane | HIGH_RISK |
|---|---|---|---|
| Duração | 3 semanas | WIs | 7 |
| Forcing factors | FF-HR-003 (PII/GDPR), FF-HR-005 (privacy controls), FF-HR-010 (1ª regulatory full impl) |

## 1. Objetivo

Implementar pipeline completo de DSR (Data Subject Rights) conforme LGPD Art. 18 / GDPR Art. 15-22: self-service API + UI para access, correction, erasure, portability, objection, consent revoke. Erasure automation cross-backend (D1, Neon, R2, KV, DO, Grafana Loki). Consent management com proof of informed consent (CTRL-PRIV-CONSENT-001..006). Breach notification runbook dry-run.

## 2. Lane + forcing factors

- **Lane:** HIGH_RISK
- **FF-HR-003**: processa PII / dados regulados direta.
- **FF-HR-005**: implementa CTRL-PRIV-001..033 + CTRL-PRIV-CONSENT-001..006.
- **FF-HR-010**: 1ª implementação regulatory completa (LGPD + GDPR).

## 3. Inherits_from

```yaml
inherits_from:
  - "PRIVACY-MODEL"
  - "COMPLIANCE-MATRIX"
  - "SECURITY-MODEL"
  - "AUTH-MODEL"
  - "DATA-MODEL"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
```

## 4. CAPs entregues

- **CAP-PRIV-001**: DSR self-service API (`POST /v1/privacy/dsr`).
- **CAP-PRIV-002**: Erasure automation cross-backend (D1, Neon, R2, KV, DO, Loki).
- **CAP-PRIV-003**: Consent ledger com proof of informed (notice_text_hash + version + locale).
- **CAP-PRIV-004**: Privacy notice publication + versioning (`/privacy`).
- **CAP-PRIV-005**: Breach notification runbook executable (RB-BREACH-NOTIF dry-run).
- **CAP-PRIV-006**: Sub-processor register público + notification ≥ 30d antes de mudança.
- **CAP-PRIV-007**: Residency pinning E2E (tenant region opt-in).

## 5. Requirements específicos

- **R-S11-1**: DSR API endpoints (access, correction, erasure, portability, objection, consent_revoke).
- **R-S11-2**: Erasure worker (`dsr-erasure-worker`) com propagation D1/Neon/R2/KV/Loki.
- **R-S11-3**: Consent endpoint (`POST /v1/consent/<purpose>` + `DELETE` + `GET`).
- **R-S11-4**: Consent record payload completo (notice_text_hash + notice_version + locale + wording_id + ui_capture_ts + submission_ts).
- **R-S11-5**: Privacy notice em `legal/privacy-notice/v<M.m>.md` versionado.
- **R-S11-6**: Sub-processor page auto-generated de `legal/sub-processors.md`.
- **R-S11-7**: DPO role: designar DPO interim (Gustavo) + plano pra DPO permanente.

## 6. DoD

- [ ] 7 WIs SEALED.
- [ ] E2E erasure: fake user signup → use product → DSR erasure → 0 records em 30d SLA.
- [ ] Residency: tenant escolhe EU region → blob lands em `cas-weur`, não `cas-enam`.
- [ ] Consent UI screenshot evidence captured (EVT-012 + EVT-049).
- [ ] RB-BREACH-NOTIF dry-run executado com Legal + Privacy Officer.
- [ ] LGPD ANPD + Irish DPC templates drafted em `legal/breach-notification-*.md`.
- [ ] CTRL-PRIV-030 (erasure pipeline) + CTRL-PRIV-031 (residency) em prod.

## 7. Completeness (delta)

- [ ] **10.s11.1** SLO-FRESH-DSR-ERASURE: 99% ≤ 30 dias.
- [ ] **10.s11.2** DPIA para novo feature privacy-impacting.
- [ ] **10.s11.3** LIA template preenchido para telemetria legitimate interest.

## 8. Invariants

- INV-DATA-ERASURE-COMPLETE (HIGH): erasure é efetiva cross-backend.
- INV-DATA-RESIDENCY (HIGH): tenant pinned region não vaza.
- INV-AUDIT-APPEND-ONLY: DSR events e consent events são imutáveis.

## 9. Quality Standards

- Zero PII em audit logs (CTRL-PRIV-001 + CTRL-PRIV-014).
- DSR API: autenticação via PAT + MFA re-auth (CTRL-PRIV-016 for sensitive ops).
- Consent proof: notice_text_hash verifiable post-facto.

## 10. Anti-scope

- ❌ Schrems II TIA templates (pós-GA se EU tenants materializarem).
- ❌ CCPA/CPRA specific flows (aligned com GDPR; specific wording em S-18 docs).
- ❌ HIPAA BAA (opt-in enterprise S-14).

## 11. Dependencies

- Blocker: S-01+S-02+S-03 (data actually exists to be erased).
- Blocker: S-09 observability (DSR events emitted).
- Blocker: S-10 (billing data é parte do DSR scope).

## 12. WIs antecipados

| ID | Título |
|---|---|
| WI-S11-001 | DSR API endpoints (6 direitos) |
| WI-S11-002 | Erasure worker cross-backend |
| WI-S11-003 | Consent ledger + proof payload |
| WI-S11-004 | Privacy notice versioning |
| WI-S11-005 | Sub-processor register + notification |
| WI-S11-006 | Breach notification runbook + templates legais |
| WI-S11-007 | Residency pinning E2E |

## 13. Duração

3 semanas; buffer 7 dias (legal review depende de firm externo).

## 14. Critérios de promoção

- DoD + erasure E2E + DPO/Legal sign-off.
- DPIA documentada + Legal Review EVT-044.

## 15. Riscos

| Risco | Prob | Impacto |
|---|---|---|
| Erasure incompleto (algum backend ignora) | M | CRITICAL (LGPD non-compliance) |
| Consent record sem notice proof (GDPR Art. 7) | M | HIGH |
| Residency leak (tenant EU dado em US) | L | CRITICAL (Schrems II) |
| Legal templates inadequados | M | HIGH (dependência externa) |

---
