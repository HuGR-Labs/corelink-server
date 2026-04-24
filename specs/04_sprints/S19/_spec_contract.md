---
id: "SPEC-CONTRACT-S19"
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
tags: ["spec-contract", "s19", "onboarding", "dpa", "standard"]
---

# Spec Contract — S-19: Customer Onboarding (signup → DPA → billing)

## 0. Metadata

| Sprint ID | S-19 | Lane | STANDARD |
|---|---|---|---|
| Duração | 2 semanas | WIs | 5 |

## 1. Objetivo

Operacionalizar o fluxo completo de customer onboarding: self-service signup → DPA click-through → tier selection → billing setup → first usage. Reduz friction pra conversão. Enterprise onboarding via white-glove process separado.

## 2. Lane + forcing factors

- **Lane:** STANDARD. Toca contratos (DPA, Terms) via flow automated.

## 3. Inherits_from

```yaml
inherits_from:
  - "COMPLIANCE-MATRIX"
  - "PRIVACY-MODEL"
  - "AUTH-MODEL"
```

## 4. CAPs entregues

- **CAP-ONBOARD-001**: Self-service signup flow (Clerk + email verify).
- **CAP-ONBOARD-002**: DPA click-through (version-aware + evidence EVT-049 consent).
- **CAP-ONBOARD-003**: Tier selection + Stripe subscription setup.
- **CAP-ONBOARD-004**: First-run experience (sample PAT + quickstart link).
- **CAP-ONBOARD-005**: Enterprise inquiry form + white-glove handoff.

## 5. Requirements específicos

- **R-S19-1**: Signup → email verify → tenant provisioning → landing dashboard.
- **R-S19-2**: DPA template v1 em `legal/dpa/v1.md` + click-through com signature EVT-049.
- **R-S19-3**: Tier selection UI integrado Stripe Checkout (S-10).
- **R-S19-4**: First-run: cria default PAT, mostra CLI install command, link pra docs.
- **R-S19-5**: Enterprise inquiry form → triggers Slack notification + CRM entry.

## 6. DoD

- [ ] 5 WIs SEALED.
- [ ] E2E signup flow < 3 min start-to-finish.
- [ ] DPA click-through gera evidence imutável (EVT-049).
- [ ] Stripe subscription activa em team tier.
- [ ] Enterprise inquiry flow testado com 1 fake lead.

## 7. Completeness (delta)

- [ ] **10.s19.1** Conversion funnel métrica (signup start → first usage) instrumentada.
- [ ] **10.s19.2** Abandon rate per-step visível em dashboard.

## 8. Invariants

- DPA click = consent record full (notice_text_hash + version + locale + wording_id).
- Billing não pode ativar sem DPA signed.

## 9. Quality Standards

- Signup form a11y AA.
- Zero secrets em URL (nem token).

## 10. Anti-scope

- ❌ Enterprise MSA negotiation automation (manual com Legal).
- ❌ Trial period logic (tier `free` já serve).

## 11. Dependencies

- Blocker: S-03 (auth), S-10 (billing), S-11 (DPA/DSR), S-16 (UI), S-18 (docs).

## 12. WIs antecipados

| ID | Título |
|---|---|
| WI-S19-001 | Signup flow (Clerk email verify + tenant provisioning) |
| WI-S19-002 | DPA click-through + consent capture |
| WI-S19-003 | Tier selection + Stripe Checkout |
| WI-S19-004 | First-run experience |
| WI-S19-005 | Enterprise inquiry + CRM handoff |

## 13. Duração

2 semanas; buffer 3 dias.

## 14. Critérios de promoção

- DoD + 5 real signups em closed beta.

## 15. Riscos

| Risco | Prob | Impacto |
|---|---|---|
| DPA legal review atrasa | M | MEDIUM |
| Stripe Checkout quirks em certas regiões | M | LOW |
| Email deliverability (Clerk) | L | LOW |

---
