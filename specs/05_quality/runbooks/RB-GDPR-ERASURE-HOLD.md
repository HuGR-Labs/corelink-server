---
id: "RB-GDPR-ERASURE-HOLD"
type: "runbook"
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
tags: ["runbook", "p2", "privacy", "legal", "conflict"]
---

# RB-GDPR-ERASURE-HOLD — Conflito entre DSR-Erasure e Legal Hold

> **Trigger:** Titular solicitou erasure (LGPD Art. 18 IV / GDPR Art. 17), mas dados estão sob legal hold (audit 7y / litigation / fiscal 5y).
>
> **FM:** FM-061 (Audit log R2 Object Lock impede emergency redaction).
>
> **SLA:** decisão + comunicação ao titular ≤ 15 dias úteis.

## Contexto legal

Conflict entre:
- **Direito à eliminação** (GDPR Art. 17 / LGPD Art. 18 IV) — titular solicita
- **Exceções** (GDPR Art. 17(3) / LGPD Art. 16):
  - Legal obligation (fiscal, audit regulatório)
  - Estabelecimento / exercício / defesa de direito em processo
  - Interesse público em saúde pública
  - Arquivamento, pesquisa, estatístico
  - Legal hold em litigation

Nota: a exceção **não é automática** — deve ser documentada por caso.

## Detecção

- DSR-erasure ticket em `dsr_tickets` com `legal_hold=true`.
- Privacy Officer flagga conflict ao Legal.

## Comunicação

- Privacy Officer + Legal review; decisão documentada.
- **NÃO** é SEV; é processo normal de DSR com legal.

## Fluxo (≤ 15 dias úteis)

### Step 1: Triage (≤ 3 dias)

1. Privacy Officer consulta Legal sobre hold específico.
2. Determinar:
   - Natureza do hold (audit 7y genérico? litigation com case ID? fiscal?)
   - Dados cobertos pelo hold vs dados passíveis de erasure parcial
   - Data de expiração do hold (quando erasure vira possível)

### Step 2: Decisão (≤ 5 dias)

Três opções:

**A. Erasure completo (hold não aplica)**
- Executar `privacy_model §6.2` pipeline normal.
- Notificar titular: "Seus dados foram eliminados em YYYY-MM-DD."

**B. Erasure parcial + hold ativo**
- Eliminar dados passíveis (ex: conta, preferências).
- Manter dados sob hold (ex: audit logs, fiscal records).
- **Notificar titular com detalhamento** (conforme GDPR Art. 12):
  - "Seus dados de [categoria A] foram eliminados."
  - "Seus dados de [categoria B] estão retidos por [base legal], até [data]."
  - "Após [data], iniciaremos erasure automático de [categoria B]."
- Agendar follow-up para re-executar erasure após hold expire.

**C. Erasure negado (hold total)**
- Comunicar formalmente ao titular.
- Incluir base legal específica (referência a artigo + documento).
- Informar right to complain (ANPD/DPC).

### Step 3: Agendamento pós-hold (se B ou C)

1. Ticket programado para data de expiração do hold.
2. Automated reminder ao Privacy Officer + Legal.
3. Re-validar hold ainda aplica (hold pode ter sido removido antes).
4. Execute erasure + notificar titular (closure).

## Forensics

- Preservar todo o communication com titular em `dsr_tickets.correspondence`.
- DPIA atualizado se categoria nova de conflict emergiu.

## Comunicação ao titular (templates)

Ver `legal/dsr-templates/`:
- `partial-erasure-with-hold.md`
- `erasure-denied-with-basis.md`
- `erasure-scheduled-post-hold.md`

## Prevenção

- CTRL-PRIV-033 (legal hold override) + CTRL-AUDIT-005 (retention policy).
- Privacy notice clara sobre retention periods legais.
- Training anual do Privacy Officer + Legal sobre conflict resolution.
