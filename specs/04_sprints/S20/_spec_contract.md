---
id: "SPEC-CONTRACT-S20"
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
tags: ["spec-contract", "s20", "ga", "readiness", "final", "high-risk"]
---

# Spec Contract — S-20: GA Readiness (PRR Global + 72h Staging + Adversarial Pentest)

## 0. Metadata

| Sprint ID | S-20 | Lane | HIGH_RISK |
|---|---|---|---|
| Duração | 3 semanas | WIs | 7 |
| Forcing factors | FF-HR-005 (controle final); FF-HR-009 (contratos com customers vão production); FF-HR-010 (primeira entrega regulatory live) |

## 1. Objetivo

Última milha pra General Availability. PRR global (todos os 10 canonical sources verdes), pentest externo completo, 72h staging sustained, SOC 2 gap analysis preliminar, customer reference beta (3 lighthouse customers migrados com sucesso), SLA contratual standardized, marketing prep, press release ready.

## 2. Lane + forcing factors

- **Lane:** HIGH_RISK. Qualquer miss aqui = delay GA.

## 3. Inherits_from

**Todos os canonical sources** (este é o sprint de integração final):

```yaml
inherits_from:
  - "FRAMEWORK-00"
  - "SECURITY-MODEL"
  - "PRIVACY-MODEL"
  - "OBSERVABILITY-MODEL"
  - "SLO-CATALOG"
  - "FAILURE-MODES"
  - "RESILIENCE-PATTERNS"
  - "DATA-MODEL"
  - "STORAGE-SEMANTICS-MATRIX"
  - "AUTH-MODEL"
  - "KEY-MANAGEMENT"
  - "COMPLIANCE-MATRIX"
  - "INVARIANT-REGISTRY"
  - "REMOTE-CACHE-PRODUCT-PROFILE"
```

## 4. CAPs entregues

- **CAP-GA-001**: GA readiness (all boxes checked em roadmap.md).
- **CAP-GA-002**: External pentest report clean (HIGH/CRITICAL remediated).
- **CAP-GA-003**: SOC 2 gap analysis (preparation for Type I engagement pós-GA).
- **CAP-GA-004**: 3 lighthouse customers migrated + attestations.
- **CAP-GA-005**: SLAs contratuais publicados + DPA v1 ready.
- **CAP-GA-006**: Incident response team ready; PagerDuty schedule cobrindo 24/7.
- **CAP-GA-007**: Marketing launch prep (press release, case studies, blog posts).

## 5. Requirements específicos

- **R-S20-1**: Global PRR (`PRR-GA-001`) seguindo `_templates/production_readiness_review.md` com lane HIGH_RISK; 10-12 sign-offs.
- **R-S20-2**: External pentest (hire Schellman ou A-LIGN); report + retest; EVT-025.
- **R-S20-3**: SOC 2 Type I gap analysis + readiness report (Drata/Vanta integration); não é audit completo, mas rehearsal.
- **R-S20-4**: 3 lighthouse customers: 2 team tier + 1 enterprise com BYOK.
- **R-S20-5**: SLA doc published; DPA v1 signed com 3 lighthouse customers.
- **R-S20-6**: Incident response: on-call rotations live 24/7 em 3 regiões.
- **R-S20-7**: Marketing: press release, blog posts (≥ 5), case studies, Product Hunt launch prep.

## 6. DoD

- [ ] 7 WIs SEALED.
- [ ] PRR global APPROVED com zero CONDITIONALLY_APPROVED sub-items.
- [ ] Pentest: zero HIGH/CRITICAL findings pending.
- [ ] 72h staging sem SEV-1; sem SEV-2 não-resolvido.
- [ ] 3 lighthouse customers com SLA claim met em 30d.
- [ ] SOC 2 gap analysis delivered (não é audit; é roadmap).
- [ ] Oncall schedule rodando 24/7; PagerDuty responses < 5 min testadas.
- [ ] Marketing ready: press release reviewed por PR + Legal.
- [ ] All docs (S-18) complete + reviewed.
- [ ] Compliance officer sign-off.

## 7. Completeness (delta)

- [ ] **10.s20.1** All SLOs sustained 30d prod-like load.
- [ ] **10.s20.2** All runbooks dry-run executed in last 90d.
- [ ] **10.s20.3** All CAP-XXX in roadmap delivered (roadmap coverage = 100%).
- [ ] **10.s20.4** Zero active waivers em controles CRITICAL.
- [ ] **10.s20.5** SOC 2 gap analysis identifies concrete GAP-XX items + fix timeline.

## 8. Invariants

Todas as invariants CRITICAL (14 canonical sources contribute) must be active:

- INV-TENANT-ISOLATION (TLA+ verified + pentest red-team tested).
- INV-CAS-INTEGRITY (scrub + client verify).
- INV-AUDIT-APPEND-ONLY (Object Lock + chain verify daily).
- INV-GC-001 + INV-GC-004 (TLA+ verified + 30d staging clean).
- INV-BILLING-NO-LOSS + INV-BILLING-NO-DUP (reconciliation < 0.1% drift).

## 9. Quality Standards

- Full SBOM v1.0 signed published.
- All 26 runbooks dry-run tested in 90d.
- Zero SEV-1 in prod in 30d prior to GA.
- TLA+ all 4 specs verdes em CI.

## 10. Anti-scope

- ❌ Fase 2 (Remote Execution) — explicitly out of GA.
- ❌ Apache 2.0 open source release (pós-GA decision).
- ❌ SOC 2 Type I cert (6 meses pós-GA).

## 11. Dependencies

- **Blocker:** S-00 a S-19 todos SEALED.

## 12. WIs antecipados

| ID | Título |
|---|---|
| WI-S20-001 | PRR global preparation + orchestration |
| WI-S20-002 | External pentest engagement + remediation |
| WI-S20-003 | SOC 2 gap analysis |
| WI-S20-004 | 3 lighthouse customer migration |
| WI-S20-005 | SLA + DPA finalization + Legal |
| WI-S20-006 | Incident response 24/7 ready |
| WI-S20-007 | Marketing launch prep |

## 13. Duração

3 semanas; buffer 10 dias (pentest findings podem gerar cascata).

## 14. Critérios de promoção

- DoD complete + PRR global APPROVED.
- 72h clean staging sustained.
- Zero pending CRITICAL finding.
- **→ GA ANNOUNCEMENT** (public launch).

## 15. Riscos

| Risco | Prob | Impacto |
|---|---|---|
| Pentest encontra CRITICAL → delay GA | M | HIGH (mas é o objetivo do pentest) |
| Lighthouse customer desiste mid-sprint | M | MEDIUM |
| SOC 2 gap analysis revela > 50 gaps | M | MEDIUM (audit rescheduled) |
| Last-minute regression em prod staging | M | HIGH |
| Legal review DPA atrasa | M | MEDIUM |

---

**Post-GA:** Sprint S-21+ começam Fase 2 (Remote Execution — `execute-action`, executor identity, sandbox runtime), abrindo novo ciclo de 10+ sprints.
