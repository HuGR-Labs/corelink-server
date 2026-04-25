---
id: "S-00"
type: "sprint"
doc_status: "DRAFT"
work_status: "READY"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-25"
updated: "2026-04-25"
lane: "STANDARD"
# lane_forcing_factors omitted: STANDARD lane permite empty (REG-LANE-003 só obriga se lane=HIGH_RISK)
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
inherits_from:
  - "FRAMEWORK-00"
  - "REMOTE-CACHE-PRODUCT-PROFILE"
  - "COMPLIANCE-MATRIX"
  - "OBSERVABILITY-MODEL"
  - "FAILURE-MODES"
tags: ["sprint", "s00", "planning", "meta", "standard"]
---

# Sprint S-00 — Roadmap & Planning (PRFAQ + capabilities catalog + 21 sprint skeletons)

> **doc_status:** DRAFT · **lane:** STANDARD · **Versão:** 1.0.0 · **2026-04-25**
> **Owner:** Gustavo Schneiter · **Aprovador Final:** Gustavo Schneiter
> **Spec contract base:** `_spec_contract.md` v1.1.0 (Lote 9.4 SOTA)

> **🚦 Phase boundary:** meta-sprint de planejamento; output destrava 20 sprints subsequentes.

---

## 1. Objetivo

Criar a visão completa do produto CoreLink Fase 1 GA, traduzida em artefatos auditáveis e executáveis: PRFAQ (Working Backwards Amazon-style), catálogo de CAP-XXX canônico, success metrics com thresholds numéricos, roadmap por sprint S-00..S-20.

## 2. Escopo do sprint

### 2.1 In-scope

- **WI-S00-001**: PRFAQ (press release + FAQ ≥ 15 Q&A + 3 customer testimonials antecipados).
- **WI-S00-002**: Catálogo capabilities (CAP-XXX, ≥ 50 e ≤ 80 itens organizados por domain).
- **WI-S00-003**: Success metrics (GA done definition; 5 lighthouse customers, SLOs sustained 30d, zero SEV-1 cross-tenant, SOC 2 Type I em progresso).
- **WI-S00-004**: Roadmap master (S-00..S-20 timeline com dependencies + lane + CAPs entregues).
- **WI-S00-005**: 20 sprint skeletons (`_spec_contract.md` em S-02..S-20; S-01 já tem sprint.md full).

### 2.2 Anti-scope

- ❌ Código Rust (zero implementation).
- ❌ WIs full-spec (skeleton apenas).
- ❌ ADRs novas (use ADR docs separados).
- ❌ Customer commitments externos (PRFAQ é internal Working Backwards, não GTM).
- ❌ Pricing real customer-facing (deferred to S-18 com Finance + Legal review).

## 3. Customer Impact & Journey

**JTBD:** "Como Founder/CEO HuGR + Tech Lead, eu preciso de roadmap auditável de 12 meses até GA com gates verificáveis para garantir que CoreLink ship com qualidade SOTA real, não vapor."

**Journey internal:** PRFAQ → capabilities trace → roadmap → spec contracts → sprints → WIs → implementation. Bug em S-00 = meses de rework.

**CAPs entregues:**
- `CAP-META-001` (Roadmap completo até Fase 1 GA)
- `CAP-META-002` (PRFAQ)
- `CAP-META-003` (Catálogo CAP-XXX canônico)

## 4. Capability Mapping (trace)

Ver `_spec_contract.md §4` + WI-S00-002 (a criar) para tabela canonical CAPs.

## 5. Deliverables

| ID | Entregável | Onde | DoD |
|---|---|---|---|
| S00-D1 | PRFAQ doc | `specs/01_product/prfaq.md` | < 1 page; FAQ ≥ 15 Q&A; 3 testimonials; smell test passa |
| S00-D2 | Capabilities catalog | `specs/01_product/capabilities.md` | 50-80 CAPs em 12 domains (CAS/AC/Auth/GC/Privacy/Billing/Ops/SDK/UI/Onboard/Supply/Region) |
| S00-D3 | Success metrics | `specs/01_product/success_metrics.md` | Thresholds numéricos sem "best effort" |
| S00-D4 | Roadmap master | `specs/04_sprints/roadmap.md` | Tabela S-00..S-20 com lane/dates/dependencies/CAPs |
| S00-D5 | 20 sprint skeletons | `specs/04_sprints/S02..S20/_spec_contract.md` | Cada com objetivo + lane + forcing factors + inherits_from minimum + WIs antecipados count |

## 6. Escopo técnico por camada (inherits_from)

Sprint não toca camadas técnicas (planning meta-sprint). Inherits_from existe para forçar PRFAQ + roadmap consistentes com canonical sources existentes (framework + remote-cache profile + compliance matrix + observability + failure modes).

## 7. Definition of Done (lane STANDARD)

- [ ] Todos 5 WIs do sprint em status SEALED (EVT-031).
- [ ] PRFAQ aprovado por Aprovador Final + Product Reviewer (EVT-016).
- [ ] Capabilities catalog validado: cross-ref com canonical sources zero CAPs órfãos/redundantes (EVT-001 + EVT-002 cross-ref validator).
- [ ] Success metrics com thresholds numéricos (EVT-016).
- [ ] Roadmap revisado por Tech Lead + SRE Lead + Security Lead + Finance/Cost Owner (EVT-016).
- [ ] 21 spec contracts (S-00..S-20) com YAML front matter parsing schema (EVT-026).
- [ ] PRR STANDARD 5–8 sign-offs: Tech Lead + SRE Lead + Security Lead + Finance/Cost Owner + Product + Compliance + 2 peers (EVT-031).
- [ ] **Mid-check D+3** review com 2 reviewers (Lote 9.4 codex finding fix).

## 8. Dependencies

### Hard blockers

- **Framework v1.0.0-rc1** ✅ done.
- **Canonical sources completos** ✅ done.

### Outbound

- S-00 desbloqueia **todas** as 20 sprints subsequentes (sem S-00 SEALED, sprint contracts são órfãos).

## 9. Timeline

- **Sprint kick-off:** D+0 (começou em 2026-04-24 retroativamente; 21 spec contracts já existem em v1.1).
- **Mid-check:** D+3.
- **Sprint close target:** D+5 (5 dias úteis — STANDARD timing).
- **Buffer:** 0 (STANDARD permit em meta-sprint de 1 semana; bare-minimum path OK).

## 10. Risk Register

| ID | Risco | Prob | Det | Impacto | Exposure | Residual após mitigação | Mitigação | Owner |
|---|---|---|---|---|---|---|---|---|
| R-S00-001 | PRFAQ vira waffle (bold claims sem backing) | M | M | HIGH | M | LOW | Smell test + Product Reviewer + Aprovador Final mandatory | Product |
| R-S00-002 | Roadmap dates otimistas | H | M | HIGH | H | MEDIUM | PERT estimation + 20% buffer global + gap analysis + mid-check D+3 | Tech Lead |
| R-S00-003 | Capability catalog vira lista de todo mundo | M | L | MEDIUM | L | LOW | ≤ 80 CAPs máximo; força consolidar; per-domain limit | Architect |
| R-S00-004 | Cycle em dependencies graph | L | L | HIGH | L | LOW | Topological sort em CI; refusa se cycle detected | Tech Lead |
| R-S00-005 | Data classification leak em PRFAQ | L | L | MEDIUM | L | LOW | Quality 14.s00.4 + Privacy Officer review pre-publish | Privacy Officer |

## 11. Observability Plan (delta do sprint)

Sprint não emite métricas runtime. Tracking via:
- GitHub commits/PRs em `specs/01_product/*` e `specs/04_sprints/S*/`.
- Validator scripts: `validate_references.py` + `validate_specs.py` (CI gate post Lote 9.5).

## 12. Security & Privacy (delta)

- **STRIDE delta**: nenhum (planning sprint).
- **Privacy delta**: Quality 14.s00.4 — PRFAQ não vaza dados sensíveis de tenants reais (hypothetical/anonymized).

## 13. Post-mortem hooks

- Roadmap dates miss > 20% → post-mortem (PERT calibration review).
- PRFAQ external publication com pricing claim sem Finance/Legal review → CRITICAL post-mortem.

## 14. Sign-off (STANDARD — 5–8 roles)

- Owner (Gustavo Schneiter)
- Final Approver (Gustavo Schneiter)
- Tech Lead
- SRE Lead
- Security Lead
- Finance/Cost Owner
- Product
- Compliance Officer
- 2 peer reviewers

## 15. Change log

| Versão | Data | Autor | Mudança |
|---|---|---|---|
| 1.0.0 | 2026-04-25 | Gustavo (via Claude Opus 4.7) | Criação sprint.md S-00 (Lote 9.5b Phase 2 — sprint.md gap fix per Opus R3 C-N3-03 + Codex R3-02). |

---

**Fim de S-00 sprint contract.**
