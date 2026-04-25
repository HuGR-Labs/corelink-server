---
id: "SPRINT-CREATION-CONTRACT"
type: "framework"
doc_status: "REVIEW"
audit_status: "ACTIVE"
version: "1.0.1"
created: "2026-04-24"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["meta", "sprint", "contract", "process"]
---

# Sprint Creation Contract (META) — requisitos invariáveis de toda sprint

> **doc_status:** REVIEW (Lote 9.4 promotion pós Codex r2 + Opus independent review). FROZEN ainda depende de: (a) ≥ 2 reviewers nomeados; (b) `scripts/validate_specs.py` rodando em CI (jsonschema dep installed).
> **Versão:** 1.0.1 · **Última atualização:** 2026-04-24
> **Owner:** Gustavo Schneiter · **Aprovador Final:** Gustavo Schneiter
> **Revisores:** [Codex GPT (R1+R2 audit), Opus 4.7 (independent R2 audit)] — **Reviewers humanos ainda staffing-blocked**.

> **Propósito:** esse é o **contrato meta** que define o que TODA sprint do CoreLink **DEVE** ter ao ser criada. Cada sprint individual (S-00, S-01, …, S-20) tem seu próprio `_spec_contract.md` com requirements específicos, mas todos derivam desta base.
>
> Complementa: `00_framework.md §33.5` (lanes), `_templates/sprint_contract.md` (template de preenchimento). Este doc foca em **processo** e **invariantes de forma**; o template foca em conteúdo.

---

## Sumário

1. [Quando criar uma nova sprint](#1-quando-criar-uma-nova-sprint)
2. [Estrutura de arquivos obrigatória](#2-estrutura-de-arquivos-obrigatória)
3. [YAML front matter obrigatório](#3-yaml-front-matter-obrigatório)
4. [Seções obrigatórias (por lane)](#4-seções-obrigatórias-por-lane)
5. [Inherits_from obrigatório](#5-inherits_from-obrigatório)
6. [Requirements invariáveis](#6-requirements-invariáveis)
7. [Definition of Done universal](#7-definition-of-done-universal)
8. [Completeness Criteria SOTA](#8-completeness-criteria-sota)
9. [Invariants que toda sprint protege](#9-invariants-que-toda-sprint-protege)
10. [Quality Standards universais](#10-quality-standards-universais)
11. [Processo: PROPOSED → READY → IN_PROGRESS → SEALED](#11-processo-proposed--ready--in_progress--sealed)
12. [Sign-off (lane-aware)](#12-sign-off-lane-aware)
13. [Failure paths](#13-failure-paths)

---

## 1. Quando criar uma nova sprint

Uma sprint **DEVE** ser criada quando:

- Há um conjunto coerente de WIs relacionados que entregam ≥ 1 `CAP-XXX` customer-visible OU ≥ 1 controle canônico (CTRL-XXX) em produção.
- O trabalho não cabe em 1 WI único sem virar "mega-WI" (anti-pattern).
- Há necessidade de sign-offs agregados (ex: PRR sprint-level) ou de release coordenado.

**Uma sprint NÃO DEVE ser criada** para:

- Trabalho que cabe em 1 WI STANDARD (<1 semana). Use WI direto.
- Trabalho de manutenção contínua sem objetivo customer-visible (vira "sprint guarda-chuva" — anti-pattern AP-007).
- Trabalho adversariais de fix bug individual em produção. Use incident + post-mortem.

## 2. Estrutura de arquivos obrigatória

```
specs/04_sprints/SXX/
├── _spec_contract.md        ← contract pré-sprint (este é template ← ref _sprint_creation_contract.md)
├── sprint.md                ← sprint contract full (template: _templates/sprint_contract.md)
├── work_items/              ← um .md por WI
│   ├── WI-SXX-001-<slug>.md
│   ├── WI-SXX-002-<slug>.md
│   └── ...
├── sub_tasks/               ← opcional; sub-tasks podem viver inline em WI
│   └── ST-SXX-001-<slug>.md
├── prr/                     ← se lane ≥ STANDARD
│   └── PRR-SXX-001-<slug>.md
└── retrospective.md         ← criado ao fim da sprint (sealed)
```

- **Naming SXX**: S00, S01, …, S99 (two-digit padded; uppercase S).
- **Naming WI-SXX-NNN**: 3-digit padded; NNN sequencial dentro da sprint.

## 3. YAML front matter obrigatório

Sprint contract (`sprint.md`) DEVE ter:

```yaml
---
id: "S-XX"                          # matching dir
type: "sprint"
doc_status: "DRAFT"                 # progride DRAFT → REVIEW → FROZEN
work_status: "READY"                # progride PROPOSED → READY → IN_PROGRESS → REVIEWING → COMPLETE → SEALED | FAILED
audit_status: "ACTIVE"
version: "X.Y.Z"                    # SemVer
created: "YYYY-MM-DD"
updated: "YYYY-MM-DD"
lane: "LOW_RISK|STANDARD|HIGH_RISK"
lane_forcing_factors: ["FF-HR-..."] # obrigatório se lane=HIGH_RISK
owner: "<nome>"
final_approver: "<nome>"
reviewers: []
supersedes: null
superseded_by: null
inherits_from: [...]                # mínimo 3 canonical sources; ver §5
tags: ["sprint", "sXX", "<dominio>", "<lane>"]
---
```

Spec contract (`_spec_contract.md`) DEVE ter:

```yaml
---
id: "SPEC-CONTRACT-SXX"
type: "spec_contract"               # enum no schema
doc_status: "DRAFT"                 # vira FROZEN quando a sprint.md é criada
audit_status: "ACTIVE"
version: "X.Y.Z"
...
```

## 4. Seções obrigatórias (por lane)

Derivado de `00_framework.md §33.5.4.1` (matriz WI), adaptado pra sprint:

| Seção sprint.md | LOW_RISK | STANDARD | HIGH_RISK |
|---|---|---|---|
| §1 Objetivo | ✅ | ✅ | ✅ |
| §2 Escopo (in + anti) | ✅ | ✅ | ✅ |
| §3 Customer Impact & Journey | ⛔ | ✅ | ✅ |
| §4 Capability Mapping | ✅ | ✅ | ✅ |
| §5 Deliverables | ✅ | ✅ | ✅ |
| §6 Escopo técnico por camada | 🟡 | ✅ | ✅ |
| §7 Definition of Done | ✅ | ✅ | ✅ |
| §8 Dependencies | ✅ | ✅ | ✅ |
| §9 Timeline | ✅ | ✅ | ✅ |
| §10 Risk Register | 🟡 | ✅ | ✅ (+ detectability/exposure/residual) |
| §11 Observability Plan (delta) | 🟡 | ✅ | ✅ |
| §12 Security & Privacy (delta) | ⛔ | 🟡 STRIDE mini | ✅ STRIDE + LINDDUN |
| §13 Post-mortem hooks | ⛔ | ✅ | ✅ |
| §14 Sign-off | ver §12 | ver §12 | ver §12 |
| §15 Change log | ✅ | ✅ | ✅ |

**Legenda:** ✅ obrigatório · 🟡 condicional · ⛔ N/A.

## 5. Inherits_from obrigatório

Toda sprint **DEVE** declarar `inherits_from` listando canonical sources aplicáveis. Mínimo por lane:

- **LOW_RISK**: mínimo 1 source (o domínio principal).
- **STANDARD**: mínimo 3 sources (domínio principal + OBSERVABILITY-MODEL + FAILURE-MODES).
- **HIGH_RISK**: mínimo 5 sources (+ SECURITY-MODEL + INVARIANT-REGISTRY obrigatórios).

Sprints que tocam:
- **Auth / crypto / tenant isolation** → DEVE incluir AUTH-MODEL, SECURITY-MODEL, KEY-MANAGEMENT.
- **PII / dados pessoais** → DEVE incluir PRIVACY-MODEL + COMPLIANCE-MATRIX.
- **Storage / state** → DEVE incluir DATA-MODEL + STORAGE-SEMANTICS-MATRIX.
- **SLOs / reliability** → DEVE incluir SLO-CATALOG + RESILIENCE-PATTERNS.

Cada `inherits_from` entry **DEVE** ser ID válido de canonical source em `00_framework.md §35.5.1`.

## 6. Requirements invariáveis

Qualquer sprint DEVE:

### 6.1 Machine-readable

- YAML front matter parseia em schema validator (`scripts/validate_specs.py`).
- Cross-references em bodies são válidos via `scripts/validate_references.py` (zero dangling).

### 6.2 Rastreável

- Cada `CAP-XXX` entregue é listado em §4 Capability Mapping.
- Cada INV-XXX protegida é listada em §12 ou §6.3 (invariantes).
- Cada CTRL-XXX implementada é listada em §12.

### 6.3 Verificável

- DoD §7 é checklist binário (`[ ]`) — nenhuma cláusula "best effort".
- Cada item DoD tem Evidence type tipado (EVT-NNN conforme `00_framework.md §35.7`).

### 6.4 Proporcional à lane

- LOW_RISK não carrega cerimônia HIGH_RISK (AP-007 "cerimônia sem rigor").
- HIGH_RISK não é rebaixado pra evitar sign-offs (má-fé).

### 6.5 Orçado

- §9 Timeline tem start/mid/end dates + buffer.
- HIGH_RISK tem buffer mínimo 5 dias úteis.
- Estimativa PERT (Best / Most Likely / Worst) em WIs STANDARD e HIGH_RISK.

## 7. Definition of Done universal

Toda sprint, independente de lane, ao atingir `work_status: SEALED` **DEVE** ter:

- [ ] Todos os WIs do sprint em status SEALED (ou formalmente CANCELED com ADR).
- [ ] Todos os testes automatizados do sprint verdes em CI (EVT-001 + EVT-002).
- [ ] Zero dangling cross-references no validator.
- [ ] Schema validation 100%.
- [ ] Change log §15 atualizado.
- [ ] Sign-off §14 completo por lane (§12 deste meta-contract).
- [ ] Retrospectiva `retrospective.md` criada com: o que funcionou, o que não funcionou, 3 ações concretas pra próxima sprint.
- [ ] Se houve SEV-1/SEV-2 durante a sprint: post_mortems linkados.

DoD específico por lane adiciona obrigatoriedades (ver `00_framework.md §33.5.4.1`).

## 8. Completeness Criteria SOTA

Herda `00_framework.md §10` do template WI. Para sprint-level, adicionalmente:

- [ ] **10.s.1 Agregação de evidence**: todos os EVTs emitidos pelos WIs do sprint estão acessíveis em `_audits/sprints/SXX/` (index ou bucket R2).
- [ ] **10.s.2 Cobertura CAP**: todo `CAP-XXX` em §4 tem evidence de implementação (integration test passa, métricas emitindo, API respondendo).
- [ ] **10.s.3 Zero regressões**: testes de sprints anteriores continuam verdes em CI.
- [ ] **10.s.4 SLO budget**: error budget de SLOs afetados não foi estourado durante staging rollout.
- [ ] **10.s.5 Observability baseline**: métricas/logs/traces emitindo em staging; dashboards criados; alertas armados.

## 9. Invariants que toda sprint protege

Toda sprint, independente de escopo, **DEVE** manter:

- **INV-TENANT-ISOLATION** (CRITICAL): sprint não pode introduzir code path que leia/escreva/enumere cross-tenant. Enforcement: TLA+ check `tenant_isolation.tla` verde.
- **INV-CAS-INTEGRITY** (CRITICAL): sprint não pode permitir write com hash mismatch. Enforcement: CTRL-CAS-001 + TLA+ `cas_integrity.tla`.
- **INV-AUDIT-APPEND-ONLY** (CRITICAL): sprint não pode introduzir UPDATE/DELETE em audit log. Enforcement: CTRL-AUDIT-001 + TLA+ `audit_immutability.tla`.
- **INV-SUPPLY-SIGNED-DEPLOY** (HIGH): sprint não pode introduzir binário não-assinado em deploy. Enforcement: CTRL-SUPPLY-002 em release pipeline.
- **INV-CONF-AT-REST** (HIGH): sprint não pode persistir PII/credentials em plaintext. Enforcement: CTRL-CRYPTO-002 + CTRL-CRED-001.
- **INV-CONF-IN-FLIGHT** (HIGH): sprint não pode aceitar TLS < 1.3. Enforcement: CTRL-CRYPTO-001 + CTRL-NET-001.

Sprint que **intencionalmente** mexe numa dessas invariantes (ex: S-06 GC toca INV-GC-*) DEVE:
- Listar forcing factor correspondente em `lane_forcing_factors`.
- Ter TLA+ check específico.
- Ter adversarial reviewer sign-off.

## 10. Quality Standards universais

Herda `_templates/sprint_contract.md §14` do template. Universal:

- **14.1 Code quality**: clippy strict + no unsafe + no unwrap em lib code (Result everywhere).
- **14.2 Test quality**: property tests > unit; 10k iter mínimo para invariantes CRITICAL.
- **14.3 Docs quality**: rustdoc + exemplos executáveis; arch diagrams atualizados.
- **14.4 Perf quality**: criterion baseline committed; regression > 10% bloqueia PR.
- **14.5 Security quality**: SAST + dependency audit + fuzz (HIGH_RISK).
- **14.6 Observability quality**: cada op emite RED metrics (rate/errors/duration).
- **14.7 Operability quality**: runbook para cada FM classe P0/P1 introduzido.
- **14.8 Evolvability quality**: breaking changes = bump major + migration doc.
- **14.9 Sustainability** (HIGH_RISK only): memory footprint bounded; zero allocations em hot path steady-state.
- **14.10 Cost regression gate (universal Lote 9.4)**: cada sprint que toca hot path (CAS/AC/billing/observability/region) DEVE incluir benchmark com per-op cost estimate em $USD por million ops; PR > 10% regression bloqueia merge sem ADR. Aplicação obrigatória em S-07/S-08/S-09/S-10/S-14; opt-in para outros. Reference: codex R1 SOTA enrichment + Opus H-09. Per-tenant tier: monthly cost projection sustained 7d staging em DoD.

## 11. Processo: PROPOSED → READY → IN_PROGRESS → SEALED

Transições `work_status` (conforme `00_framework.md §7.5`):

```
[Init]
  ↓
PROPOSED            ← spec_contract criado; sprint draft iniciado
  ↓ (owner + aprovador final assinam spec_contract)
READY               ← sprint.md completo; todos os WIs definidos; dependencies verificadas
  ↓ (start date atingida)
IN_PROGRESS         ← WIs sendo executados
  ↓ (todos os WIs SEALED)
REVIEWING           ← sprint-level review; PRR se aplicável
  ↓ (sign-offs completos)
COMPLETE            ← DoD universal atingido
  ↓ (retrospective criada)
SEALED              ← sprint fechado; não mutável
```

**FAILED path**: qualquer ponto, se sprint for abortado, `work_status: FAILED` + post-mortem + WIs orphaned adotados por sprint futuro ou cancelados.

## 12. Sign-off (lane-aware)

Alinha 1:1 com `00_framework.md §33.5.4.3`:

- **LOW_RISK**: 3 sign-offs (Sprint Owner + Code Reviewer + Aprovador Final).
- **STANDARD**: 5–8 sign-offs (+ Security + SRE + QA + Product; opcionais Privacy/Architect/Cost).
- **HIGH_RISK**: 10–12 sign-offs (todos os 11 canônicos + 1–2 extensões sprint-only: Compliance + Adversarial).

Sign-offs preenchidos em `sprint.md §14.1` conforme template. Papéis specific em `_templates/sprint_contract.md §20.1`.

## 13. Failure paths

Se sprint falha (não atinge SEALED):

1. `work_status: FAILED` marcado.
2. Post-mortem obrigatório em `specs/_audits/sprint-fail-SXX.md`.
3. WIs incompletos: decide-se explicitamente (a) cancelar via ADR, (b) mover pra sprint próxima.
4. Retrospectiva `retrospective.md` focada em: por que falhou, o que aprender, como evitar.
5. Next sprint inherits lessons em seu spec_contract.

---

## Anti-patterns proibidos

- ❌ **Sprint guarda-chuva** (AP-007): sprint sem CAP customer-visible, só trabalho interno.
- ❌ **Sprint de mais de 6 semanas**: força split; pair with HIGH_RISK ceiling 4 semanas.
- ❌ **Rebaixar lane** pra evitar sign-offs.
- ❌ **Adicionar WIs mid-sprint** sem ADR + aprovação (AP-019 "rigor superficial em scope creep").
- ❌ **Pular retrospectiva**: SEALED sem retrospective.md = work_status volta pra REVIEWING.
- ❌ **Sprint que depende de sprint futura** (dependency inversion).

---

## Referências

- `00_framework.md §33.5` — risk lanes.
- `00_framework.md §35.5` — control inheritance.
- `_templates/sprint_contract.md` — template de preenchimento.
- `specs/04_sprints/SXX/_spec_contract.md` — contratos específicos por sprint.

---

**Fim de Sprint Creation Contract.** Alterações aqui afetam **todas** as sprints futuras — requerem ADR + aprovação Arquiteto + bump major.
