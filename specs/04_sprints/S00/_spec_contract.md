---
id: "SPEC-CONTRACT-S00"
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
tags: ["spec-contract", "s00", "planning", "meta"]
---

# Spec Contract — S-00: Roadmap & Planning

> **Propósito:** meta-sprint de planejamento. Entrega o roadmap completo, PRFAQ, catálogo de capabilities, success metrics, e ~20 sprint skeletons.

## 0. Metadata

| Campo | Valor |
|---|---|
| Sprint ID | S-00 |
| Nome | Roadmap & Planning |
| Lane | STANDARD |
| Lane forcing factors | — (STANDARD default) |
| Duração estimada | 5 dias úteis |
| WIs antecipados | 5 |

## 1. Objetivo

Criar a visão completa do produto CoreLink Fase 1 GA, traduzida em artefatos que tornem o plano auditável e executável sem ambiguidade: PRFAQ (Working Backwards), catálogo de CAP-XXX, success metrics, roadmap por sprint. O output dessa sprint destrava todas as 20 sprints seguintes — sem ele, qualquer WI futuro é ambíguo quanto a "onde encaixa".

## 2. Lane + forcing factors

- **Lane:** STANDARD
- **Rationale:** trabalho de planejamento não toca invariantes CRITICAL, mas tem blast radius alto (bad plan = meses de rework). Merece rigor STANDARD (5–8 sign-offs), não LOW_RISK.
- **Forcing factors:** nenhum (sprint não toca tenant isolation, crypto, retention, etc).

## 3. Inherits_from

```yaml
inherits_from:
  - "FRAMEWORK-00"
  - "REMOTE-CACHE-PRODUCT-PROFILE"
  - "COMPLIANCE-MATRIX"
```

## 4. CAPs entregues

- **CAP-META-001**: Roadmap completo até Fase 1 GA com marcos verificáveis.
- **CAP-META-002**: PRFAQ público-ready (press release + FAQ + customer testimonial antecipado).
- **CAP-META-003**: Catálogo CAP-XXX canônico que qualquer WI futuro cita.

## 5. Requirements específicos desta sprint

- **R-S00-1**: `specs/01_product/prfaq.md` — press release 1-pager + FAQ ≥ 15 Q&A + 3 customer testimonials antecipados (Working Backwards Amazon-style).
- **R-S00-2**: `specs/01_product/capabilities.md` — catálogo ≥ 50 CAP-XXX organizados por domínio (CAS, AC, Auth, GC, Privacy, Billing, Ops, SDK, UI, Onboard, Supply, Region).
- **R-S00-3**: `specs/01_product/success_metrics.md` — definição de GA done medível (ex: 5 tenants pagantes em produção, SLOs sustained 30 dias, zero SEV-1 cross-tenant, SOC 2 Type I em progresso).
- **R-S00-4**: `specs/04_sprints/roadmap.md` — tabela mestre S-00..S-20 com: objetivo, lane, start/end, dependencies, blocking/blocked-by, CAPs entregues.
- **R-S00-5**: Skeleton `sprint.md` + `_spec_contract.md` em cada `S-02..S-20/` (S-01 já tem sprint.md full).

## 6. Definition of Done específico

- [ ] PRFAQ aprovado por Aprovador Final + Product Reviewer (EVT-016).
- [ ] Capabilities catalog validado: cross-ref com todos os canonical sources existentes mostra zero CAPs órfãos ou redundantes (EVT-001).
- [ ] Success metrics têm thresholds numéricos (sem "best effort").
- [ ] Roadmap revisado por: Tech Lead, SRE Lead, Security Lead, Finance/Cost Owner. Dates realistas dada capacity.
- [ ] 21 spec contracts (S-00 a S-20) com YAML front matter parsing no schema (EVT-026).
- [ ] Todo sprint skeleton tem: objetivo, lane, forcing factors (se HIGH), inherits_from ≥ mínimo, WIs antecipados count.

## 7. Completeness Criteria (delta local)

Herda universal §8 meta-contract. Delta específico:

- [ ] **10.s00.1 Roadmap dates totais ≤ 12 meses**: 20 sprints × 3 semanas + buffers = ~14 meses; comprimir para ≤ 12 meses via paralelismo STANDARD/LOW_RISK (só HIGH_RISK são serial).
- [ ] **10.s00.2 Dependencies acyclic**: validated via topological sort em roadmap.md.
- [ ] **10.s00.3 CAP-XXX cobre 100% dos objetivos PRFAQ**: todo feature claim em PRFAQ mapeia para ≥ 1 CAP, que mapeia para ≥ 1 WI em ≥ 1 sprint.

## 8. Invariants específicas

Sprint não toca invariantes técnicas (é meta-trabalho). Mas protege:

- **INV-DATA-CLASSIFICATION**: PRFAQ não vaza dados sensíveis de tenants reais (é hypothetical).
- **INV-SCOPE-DISCIPLINE** (sprint-level): o output do S-00 é a lei; sprints subsequentes não podem alterar roadmap sem ADR.

## 9. Quality Standards (delta local)

- **14.s00.1 PRFAQ quality**: < 1 página; testa-se "smell test" (se parece produto vapor, reescreva).
- **14.s00.2 Dep graph**: topological sort roda limpo; sem ciclos.
- **14.s00.3 Spec contract consistency**: todos os 21 spec contracts passam schema + zero dangling refs.

## 10. Anti-scope

- ❌ Nenhum código Rust.
- ❌ Nenhum detalhamento de WI full (spec contracts são skeletons; WIs só ficam full-spec quando a sprint começa).
- ❌ Decisões arquitetônicas novas (use ADRs separados).
- ❌ Customer commitments (PRFAQ é internal working backwards, não GTM).

## 11. Dependencies

- **Blocker:** framework v1.0.0-rc1 ✅ (done).
- **Blocker:** canonical sources completos ✅ (done).
- **Nenhuma** sprint bloqueia S-00. S-00 bloqueia todas as demais.

## 12. WIs antecipados

| ID | Título | Estimativa |
|---|---|---|
| WI-S00-001 | PRFAQ (press release + FAQ + testimonials) | 1 dia |
| WI-S00-002 | Catálogo de capabilities (CAP-XXX) | 1 dia |
| WI-S00-003 | Success metrics (GA done definition) | 0.5 dia |
| WI-S00-004 | Roadmap master (S-00..S-20 timeline) | 1.5 dias |
| WI-S00-005 | 20 sprint skeletons (sprint.md draft em S-02..S-20) | 1 dia |

Total: 5 dias.

## 13. Estimativa de duração

- Start: **2026-04-28** (Mon)
- End: **2026-05-02** (Fri)
- Buffer: 0 (STANDARD permit não-buffer em meta-sprint de 1 semana; bare-minimum path ok).

## 14. Critérios de promoção ao próximo sprint

S-00 promove para S-01 (em curso) quando:

- Todos os 5 WIs SEALED.
- Roadmap aprovado (sign-off dos 5 reviewers STANDARD).
- S-01 `sprint.md` já existente (done Lote 8) citado em roadmap.md §S-01 como "em execução".
- Zero dangling cross-refs no validator.

## 15. Riscos antecipados

| Risco | Prob | Impacto | Mitigação |
|---|---|---|---|
| PRFAQ vira waffle ("bold claims sem backing") | M | H | Review com "smell test"; rejeitar se não acreditável |
| Roadmap dates são otimistas | H | H | PERT estimation + 20% buffer global + gap analysis |
| Capability catalog vira lista de todo mundo | M | M | ≤ 80 CAPs máximo; força consolidar |
| Scope creep em spec contracts (virar WIs full) | M | M | Time-box 5 dias; spec contracts são skeletons not full specs |

---

**Fim de spec contract S-00.** Quando sprint.md for criado (no ato de começar a sprint), este doc vira `doc_status: FROZEN` e sprint.md o referencia em §4 Capability Mapping.
