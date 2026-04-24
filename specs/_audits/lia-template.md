---
id: "LIA-TEMPLATE"
type: "framework"
doc_status: "DRAFT"
audit_status: "ACTIVE"
version: "0.1.0"
created: "2026-04-24"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
tags: ["template", "privacy", "lia", "lgpd", "gdpr"]
---

# LIA Template — Legitimate Interest Assessment

> **Use:** quando categoria de processamento usa "legitimate interest" (LGPD Art. 7, IX / GDPR Art. 6(1)(f)) como base legal. Resolve audit finding F-10. EVT-046.

> Cópia para `specs/_audits/lia-<categoria>-<YYYYMMDD>.md` por categoria. Anual review obrigatório (CTRL-PRIV-CONSENT-004).

---

## 0. Metadata

| Campo | Valor |
|---|---|
| **ID** | `LIA-{{categoria-slug}}-{{YYYYMMDD}}` |
| **Categoria de processamento** | {{ex: telemetria de uso para detecção de abuse}} |
| **Controlador** | HuGR Labs |
| **Data de criação** | YYYY-MM-DD |
| **Próxima review** | YYYY-MM-DD (anual) |
| **Owner** | Privacy Officer |
| **Approvado por** | Privacy Officer + Legal |

---

## 1. Purpose Test (Propósito)

> O processamento atende a um interesse legítimo claro e específico?

### 1.1 Descrição do propósito
{{ex: Detectar comportamento abusivo (criptominer, scrapers) para proteger SLO dos demais tenants}}

### 1.2 Beneficiário
- [ ] HuGR (operador)
- [ ] Tenant (cliente)
- [ ] Terceiros (especificar): __________

### 1.3 Benefício específico
{{ex: SLA de 99.9% mantido para 99% dos tenants; redução de fraude billing}}

### 1.4 É um propósito que um titular esperaria?
- [ ] Sim, claramente
- [ ] Sim, com explicação razoável
- [ ] Não — usar outra base legal ou consent

---

## 2. Necessity Test (Necessidade)

> O processamento é necessário ou existe alternativa menos intrusiva?

### 2.1 O processamento atinge o propósito?
{{justificar}}

### 2.2 Alternativas consideradas
| Alternativa | Por que rejeitada |
|---|---|
| Não coletar nada | {{ex: impossível detectar abuse}} |
| Coletar menos campos | {{ex: já minimizamos para campos enum}} |
| Anonimizar mais | {{ex: já pseudonimizado via UUIDv7}} |

### 2.3 Volume/escopo é mínimo necessário?
{{checklist de campos coletados vs alternativas}}

---

## 3. Balancing Test (Balanceamento)

> O interesse legítimo prevalece sobre os direitos do titular?

### 3.1 Natureza do dado processado
- [ ] Dado pessoal comum
- [ ] Dado sensível (LGPD Art. 5, II) — **STOP, mude de base legal**
- [ ] Dado pseudonimizado
- [ ] Dado anonimizado (não é PII)

### 3.2 Razoável expectativa do titular
- Titular sabe que é coletado? (Privacy notice claro?) {{S/N + link}}
- Titular pode opt-out? {{S/N + como}}

### 3.3 Impacto potencial em direitos
- Discriminação? {{S/N + mitigação}}
- Decisão automatizada relevante? {{S/N + mitigação}}
- Reidentificação? {{S/N + risco residual}}
- Vazamento? {{mitigações de security_model.md}}

### 3.4 Salvaguardas aplicadas
- [ ] Pseudonimização (CTRL-PRIV-011)
- [ ] Minimização de campos (CTRL-PRIV-002)
- [ ] Retention reduzida (vs. default)
- [ ] DSR self-service (CTRL-PRIV-022)
- [ ] Right to object respeitado (GDPR Art. 21)
- [ ] Encryption at-rest (CTRL-CRYPTO-002)
- [ ] Audit log (CTRL-AUDIT-002)

### 3.5 Conclusão balanceamento
- [ ] **Interesse legítimo prevalece** — proceed (com salvaguardas listadas)
- [ ] **Direitos do titular prevalecem** — não usar legitimate interest; usar consent ou outra base

---

## 4. Decisão & next steps

| Decisão | ✓ |
|---|---|
| Aprovado para uso | [ ] |
| Rejeitado | [ ] |
| Aprovado condicional (com salvaguardas adicionais) | [ ] |

### Condições adicionais (se aplicável)
{{...}}

### Sign-off
- **Privacy Officer**: _________________ (data)
- **Legal**: _________________ (data)

---

## 5. Review history

| Data | Reviewer | Mudança | Decisão |
|---|---|---|---|
| YYYY-MM-DD | {{Nome}} | inicial | {{aprovado/rejeitado}} |
