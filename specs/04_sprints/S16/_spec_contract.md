---
id: "SPEC-CONTRACT-S16"
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
tags: ["spec-contract", "s16", "ui", "admin", "tenant-onboarding", "standard"]
---

# Spec Contract — S-16: Frontend Admin UI

## 0. Metadata

| Sprint ID | S-16 | Lane | STANDARD |
|---|---|---|---|
| Duração | 3 semanas | WIs | 6 |

## 1. Objetivo

Entregar Frontend web (Next.js + CF Pages) para: tenant onboarding, usage dashboard, audit log viewer, consent management, DSR request UI, PAT management, billing overview, privacy page. UI é essencial pra self-service; sem ela, onboarding/suporte ficam manuais.

## 2. Lane + forcing factors

- **Lane:** STANDARD. UI pode ser fix fast; não toca invariantes CRITICAL se segue API boundaries.

## 3. Inherits_from

```yaml
inherits_from:
  - "AUTH-MODEL"
  - "PRIVACY-MODEL"
  - "OBSERVABILITY-MODEL"
  - "COMPLIANCE-MATRIX"
```

## 4. CAPs entregues

- **CAP-UI-001**: Tenant onboarding flow (signup → DPA → billing setup).
- **CAP-UI-002**: Usage dashboard per-tenant (métricas, quota, hit ratio).
- **CAP-UI-003**: Audit log viewer (self-service, com filtros).
- **CAP-UI-004**: Consent management UI (CTRL-PRIV-CONSENT-001 render de notice versioned).
- **CAP-UI-005**: DSR request form (access, correction, erasure, portability).
- **CAP-UI-006**: PAT management (create/revoke/list scoped).
- **CAP-UI-007**: Privacy page `/privacy` + sub-processors page.

## 5. Requirements específicos

- **R-S16-1**: Next.js 15 app em `apps/web/` deployed em CF Pages.
- **R-S16-2**: Clerk SDK integration (reusa S-03 auth).
- **R-S16-3**: Usage dashboard com data de métricas Grafana (proxy endpoint).
- **R-S16-4**: Audit log viewer queryando CloudEvents em R2.
- **R-S16-5**: Consent UI captura screenshot (EVT-012) + gera EVT-049 completo com wording_id + locale.
- **R-S16-6**: DSR form com identity re-auth (CTRL-AUTH-010 MFA).
- **R-S16-7**: Privacy notice `/privacy` rendered de `legal/privacy-notice/v<M.m>.md` versioned.

## 6. DoD

- [ ] 6 WIs SEALED.
- [ ] E2E test: novo dev faz signup → cria tenant → cria PAT → faz upload via CLI.
- [ ] UI a11y WCAG 2.1 AA compliance.
- [ ] i18n: en-US + pt-BR support inicial (matching Accept-Language).
- [ ] Cross-browser: Chrome, Firefox, Safari, Edge latest.
- [ ] Lighthouse score ≥ 90 em Performance + A11y.

## 7. Completeness (delta)

- [ ] **10.s16.1** Consent form render respeita `notice_version` atual + gera wording_id único.
- [ ] **10.s16.2** DSR form: submit → confirmação em ≤ 1s + SLA clock starts visível.

## 8. Invariants

- CTRL-PRIV-CONSENT-005: locale match com Accept-Language enforcement.
- Zero PII em logs de client-side (CTRL-PRIV-001).

## 9. Quality Standards

- CSP `default-src 'none'` + explicit allowlists.
- Zero dependencies com known CVE.
- SSR/ISR where appropriate (performance).

## 10. Anti-scope

- ❌ Admin panel operacional interno (CoreLink ops) — `admin.corelink.dev` subdomain separado, não em S-16.
- ❌ Mobile app nativo.

## 11. Dependencies

- Blocker: S-03 (auth), S-10 (billing), S-11 (privacy), S-09 (observability).

## 12. WIs antecipados

| ID | Título |
|---|---|
| WI-S16-001 | Next.js skeleton + Clerk integration |
| WI-S16-002 | Tenant onboarding flow |
| WI-S16-003 | Usage dashboard + PAT management |
| WI-S16-004 | Consent UI + DSR form |
| WI-S16-005 | Audit log viewer |
| WI-S16-006 | Privacy/sub-processors pages + i18n |

## 13. Duração

3 semanas; buffer 5 dias.

## 14. Critérios de promoção

- DoD + UX testing com 5 devs externos.

## 15. Riscos

| Risco | Prob | Impacto |
|---|---|---|
| Next.js quirks com CF Pages runtime | M | MEDIUM |
| Clerk integration edge cases | M | LOW |
| a11y compliance late discovery | M | LOW |

---
