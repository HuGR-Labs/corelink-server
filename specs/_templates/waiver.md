---
id: "WAIVER-YYYYMMDD-NNN-REPLACE"
type: "waiver"
doc_status: "DRAFT"                      # enum: DRAFT | REVIEW | FROZEN | THAWED | SUPERSEDED | DEPRECATED
audit_status: "ACTIVE"                   # enum: ACTIVE | AUDIT_PENDING | AUDITED
version: "1.0.0"
created: "YYYY-MM-DD"
updated: "YYYY-MM-DD"
owner: "TEMPLATE_WAIVER_OWNER"
final_approver: "TEMPLATE_FINAL_APPROVER"
reviewers:
  - role: "gate_owner"                   # dono do gate sendo dispensado
    name: "TEMPLATE_GATE_OWNER"
  - role: "security"                     # obrigatório se gate de segurança
    name: "TEMPLATE_SECURITY_REVIEWER"
gates_waived:                            # OBRIGATÓRIO: lista de gates dispensados (IDs do framework)
  - "REG-XXX-001"                        # exemplo: regra 001 do framework
rationale: |                             # OBRIGATÓRIO: min 10 chars, multi-line ok
  TEMPLATE_PLACEHOLDER_RATIONALE.
  Explique por que o gate canônico não pode ser atendido neste caso
  específico; cite constraint técnico, operacional, temporal, ou contratual.
compensating_control: |                  # OBRIGATÓRIO: proteção alternativa
  TEMPLATE_PLACEHOLDER_COMPENSATING_CONTROL.
  Descreva a proteção que substitui (parcial ou totalmente) o controle
  dispensado durante o período de validade do waiver.
expires_at: "YYYY-MM-DD"                 # OBRIGATÓRIO: data de invalidação automática
revalidation_trigger: |                  # OBRIGATÓRIO: evento/condição que força re-review antes de expires_at
  TEMPLATE_PLACEHOLDER_REVALIDATION_TRIGGER.
  Ex: "vendor X anuncia support pra feature Y", "audit SOC 2 findings",
  "taxa de erro excede Z%", "aprovação de compliance nova."
supersedes: null
superseded_by: null
tags: []
---

# Waiver — WAIVER-YYYYMMDD-NNN: {{Título curto}}

> **Template Version:** 1.0.0
> **doc_status:** DRAFT | REVIEW | FROZEN | THAWED | SUPERSEDED | DEPRECATED (default inicial: `DRAFT`)
> **audit_status:** ACTIVE | AUDIT_PENDING | AUDITED (default inicial: `ACTIVE`)
> **Versão:** 1.0.0
> **Última atualização:** YYYY-MM-DD
> **Owner:** {{Nome}}
> **Aprovador Final:** {{Nome}}
> **Revisores:** {{gate owner, security, compliance conforme aplicável}}
> **Gate(s) dispensado(s):** {{lista}}
> **Expira em:** YYYY-MM-DD
> **Supersedes:** —
> **Superseded By:** —

> **CONTRATO INVIOLÁVEL:**
>
> 1. Waiver é exceção formal a um controle/gate definido no framework. **NÃO é perdão permanente.**
> 2. Todo waiver **DEVE** ter `expires_at` — data após a qual ele **invalida automaticamente** e CI bloqueia merges/deploys que dependam dele.
> 3. Todo waiver **DEVE** ter `compensating_control` — proteção alternativa vigente durante o waiver.
> 4. Violação de waiver (deixar expirar sem resolver ou sem renovar formalmente) é incident SEV-3 mínimo.
> 5. Waiver renovado ≥ 2 vezes consecutivas sem resolução da causa raiz é **red flag** — escalação obrigatória ao Aprovador Final pra decisão binária: consertar ou aceitar formalmente como constraint permanente via ADR.

---

## Sumário

0. [Identificação](#0-identificação)
1. [Executive Summary](#1-executive-summary)
2. [Gate(s) dispensado(s)](#2-gates-dispensados)
3. [Rationale — por que não podemos atender ao gate](#3-rationale--por-que-não-podemos-atender-ao-gate)
4. [Compensating Control — proteção alternativa](#4-compensating-control--proteção-alternativa)
5. [Impact Assessment](#5-impact-assessment)
6. [Escopo e Aplicabilidade](#6-escopo-e-aplicabilidade)
7. [Conditions for Revocation](#7-conditions-for-revocation)
8. [Review Checkpoints durante vigência](#8-review-checkpoints-durante-vigência)
9. [Plano de resolução — como eliminar o waiver](#9-plano-de-resolução--como-eliminar-o-waiver)
10. [Sign-off](#10-sign-off)
11. [Change Log](#11-change-log)

---

## 0. Identificação

| Field | Value |
|---|---|
| **Waiver ID** | WAIVER-YYYYMMDD-NNN |
| **Título** | {{título curto descritivo}} |
| **doc_status** | `DRAFT` (inicial) |
| **audit_status** | `ACTIVE` |
| **Owner** | {{Nome}} — responsável por manter o waiver válido e rastreado |
| **Aprovador Final** | {{Nome}} |
| **Reviewers** | {{gate owner}}, {{security se aplicável}}, {{compliance se aplicável}}, {{legal se aplicável}} |
| **Gate(s) dispensado(s)** | {{REG-XXX-001, REG-YYY-002, ...}} — IDs do framework |
| **Escopo (lista de artefatos afetados)** | {{WI-SXX-001, Sprint S-XX, PRR-SXX-NNN, ou "global"}} |
| **Data criação** | YYYY-MM-DD |
| **Data aprovação** | YYYY-MM-DD |
| **Expira em** | YYYY-MM-DD |
| **Prazo de vigência** | {{X semanas / Y meses}} |
| **Revalidation trigger** | {{evento/condição}} |

---

## 1. Executive Summary

> *(3–5 sentenças. Responder em prosa acessível: qual gate, por que estamos dispensando, qual proteção alternativa, quando esperamos resolver.)*

---

## 2. Gate(s) dispensado(s)

Lista **exaustiva** dos gates/regras do framework que este waiver dispensa:

| Gate ID | Referência | Descrição resumida do gate | Por que não podemos atender |
|---|---|---|---|
| REG-XXX-001 | `00_framework.md §X.Y.Z` | {{o que o gate exige}} | {{link pra §3 rationale}} |

> **REGRA:** cada gate listado **DEVE** apontar para seção específica do framework (ou de spec referenciada). "Waive o gate de segurança" é **insuficiente** — precisa ser qual regra exata.

---

## 3. Rationale — por que não podemos atender ao gate

> *(Prosa detalhada. Cite constraints técnicos, temporais, operacionais, contratuais, regulatórios. Evidence preferível: link para issue, spec, ADR, bug report.)*

### 3.1 Constraint principal

{{explicação}}

### 3.2 Alternativas consideradas antes do waiver

- {{alternativa A}}: descartada porque {{razão}}
- {{alternativa B}}: descartada porque {{razão}}

### 3.3 Evidência

- {{link}}
- {{link}}

---

## 4. Compensating Control — proteção alternativa

> **REGRA INVIOLÁVEL:** todo waiver **DEVE** ter compensating control **em vigor durante a vigência**. "Sem proteção por 6 meses" não é waiver; é decisão de aceitar risco — caminho diferente (ADR de risk acceptance).

### 4.1 Descrição do controle substituto

{{prosa detalhada}}

### 4.2 Diferenças vs controle canônico

| Aspecto | Controle canônico (dispensado) | Controle substituto (vigente) |
|---|---|---|
| Cobertura | {{escopo}} | {{escopo}} |
| Latência de detecção | {{tempo}} | {{tempo}} |
| Automação | {{auto/manual}} | {{auto/manual}} |
| Escalação | {{flow}} | {{flow}} |

### 4.3 Owner do controle substituto

{{Nome}} — responsável por garantir que o controle alternativo está operacional durante o waiver.

### 4.4 Verificação periódica

- [ ] Controle substituto testado em {{data}} — evidence: {{link}}
- [ ] Próximo teste programado: {{data}}

---

## 5. Impact Assessment

### 5.1 Risco residual

Mesmo com compensating control, que risco permanece?

- **Probabilidade:** L / M / H
- **Impacto se materializar:** L / M / H
- **Descrição:** {{prosa}}

### 5.2 Usuários/sistemas afetados

- {{persona/sistema 1}}: {{impacto}}
- {{persona/sistema 2}}: {{impacto}}

### 5.3 Compliance/Regulatory impact

- [ ] Nenhum
- [ ] GDPR/LGPD: {{consequência}}
- [ ] SOC 2: {{consequência}}
- [ ] HIPAA: {{consequência}}
- [ ] Outros: {{detalhe}}

---

## 6. Escopo e Aplicabilidade

### 6.1 Escopo temporal

- **Início:** {{data}}
- **Fim (expires_at):** {{data}}
- **Duração total:** {{X semanas/meses}}

### 6.2 Escopo por artefato

Este waiver aplica-se **apenas** a:

- {{artefato 1}}
- {{artefato 2}}

### 6.3 Escopo geográfico / regional (se aplicável)

- [ ] Global
- [ ] Apenas região {{X}}
- [ ] Apenas tenant {{Y}}

### 6.4 Escopo de tier

- [ ] Todos os tiers
- [ ] Apenas {{Free/Solo/Team/Business/Enterprise}}

---

## 7. Conditions for Revocation

> Condições que, se satisfeitas, **revogam este waiver** antes de `expires_at`.

### 7.1 Auto-revocation triggers (CI enforcement)

- [ ] **`expires_at` atingida** — waiver invalida automaticamente em {{YYYY-MM-DD}}; CI bloqueia merges/deploys que dependam dele.
- [ ] **Revalidation trigger disparado:** {{evento}} — força re-review antes de `expires_at`.

### 7.2 Manual revocation triggers

- [ ] Compensating control falha (não está operacional)
- [ ] Risco residual se materializa
- [ ] Mudança de compliance / regulatório
- [ ] Vendor/dep externa muda (se waiver dependia dela)
- [ ] Outros: {{descrição}}

### 7.3 Process de revocation manual

1. Owner notifica Aprovador Final via {{canal}}.
2. `doc_status` do waiver transiciona para `THAWED` → `REVIEW`.
3. Decisão: renovar (nova versão + novo `expires_at`), ou invalidar (`doc_status: DEPRECATED`).
4. Downstream artefatos recebem `audit_status: AUDIT_PENDING`.

---

## 8. Review Checkpoints durante vigência

Durante o período de vigência do waiver, revisões periódicas **obrigatórias**:

| Checkpoint | Data | Owner | Status |
|---|---|---|---|
| T+25% (25% da duração total) | {{data}} | {{Nome}} | ⬜ |
| T+50% | {{data}} | {{Nome}} | ⬜ |
| T+75% — início de plano de resolução (§9) | {{data}} | {{Nome}} | ⬜ |
| T+90% — go/no-go sobre renovação | {{data}} | Aprovador Final | ⬜ |

Cada checkpoint:

- [ ] Compensating control verificado operacional
- [ ] Risco residual não aumentou
- [ ] Plano de resolução §9 em progresso
- [ ] Nenhuma trigger de revocation disparada

---

## 9. Plano de resolução — como eliminar o waiver

> **REGRA:** todo waiver **DEVE** ter plano explícito de como eliminar a necessidade dele. Waiver permanente sem plano = dívida institucional.

### 9.1 Causa raiz

{{prosa — por que o gate não pode ser atendido hoje}}

### 9.2 Ações pra eliminar causa raiz

| # | Ação | Owner | Prazo | Status |
|---|---|---|---|---|
| 1 | {{ação concreta}} | {{Nome}} | {{data}} | ⬜ |
| 2 | {{ação}} | {{Nome}} | {{data}} | ⬜ |

### 9.3 Critério de sucesso (waiver pode ser encerrado)

- [ ] Gate original pode ser atendido integralmente
- [ ] Compensating control pode ser desligado sem perda de proteção
- [ ] Downstream artefatos re-auditados sem impacto

### 9.4 O que acontece se o plano falhar

- [ ] Renovar waiver com novo `expires_at` e plano atualizado
- [ ] Escalar pra ADR de risk acceptance (decisão estratégica)
- [ ] Escalar pra mudança no framework (se gate canônico é irrealista)

---

## 10. Sign-off

| Papel | Nome | Critério de aprovação | Assinatura | Data |
|---|---|---|---|---|
| **Waiver Owner** | {{Nome}} | Waiver justificado, compensating control ativo | _____________ | YYYY-MM-DD |
| **Gate Owner** (dono do gate dispensado) | {{Nome}} | Concorda com dispensa temporária | _____________ | YYYY-MM-DD |
| **Security Reviewer** (se gate de segurança) | {{Nome}} | Risco residual aceitável + compensating control suficiente | _____________ | YYYY-MM-DD |
| **Privacy Reviewer** (se toca PII) | {{Nome}} | Impact assessment aceitável | _____________ | YYYY-MM-DD |
| **Compliance Reviewer** (se toca regulação) | {{Nome}} | Compliance preservada ou risco documentado | _____________ | YYYY-MM-DD |
| **Legal** (se afeta contrato) | {{Nome}} | OK contratual | _____________ | YYYY-MM-DD |
| **Aprovador Final** | {{Nome}} | Waiver aprovado formalmente | _____________ | YYYY-MM-DD |

### 10.1 Dissents (se houver)

> Discordâncias minoritárias registradas.

*(vazio se nenhuma)*

---

## 11. Change Log

| Versão | Data | Autor | Estado | Mudança |
|---|---|---|---|---|
| 1.0.0 | YYYY-MM-DD | {{Nome}} | DRAFT | Waiver proposto |
| 1.0.1 | YYYY-MM-DD | {{Nome}} | REVIEW | Compensating control refinado após feedback |
| 1.1.0 | YYYY-MM-DD | {{Nome}} | FROZEN | Waiver aprovado e em vigor |

---

**Fim de WAIVER-YYYYMMDD-NNN.**

---

### Notas para autores

- **Localização:** waivers vivem em `specs/_waivers/WAIVER-YYYYMMDD-NNN-<slug>.md`.
- **Numeração:** `WAIVER-<data criação>-<sequencial do dia>`. Ex: `WAIVER-20260424-001`.
- **Gate owner:** SEMPRE listar como reviewer. Waiver sem aprovação do gate owner é inválido.
- **Expires_at:** máximo recomendado 6 meses. Waivers mais longos exigem escalação + justificativa adicional.
- **Renovação:** nova versão do waiver (`version` bump) com novo `expires_at`. Não aceita renovação indefinida silenciosa.
