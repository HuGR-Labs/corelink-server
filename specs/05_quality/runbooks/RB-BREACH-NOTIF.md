---
id: "RB-BREACH-NOTIF"
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
tags: ["runbook", "p1", "privacy", "legal", "breach"]
---

# RB-BREACH-NOTIF — Data Breach Notification

> **Trigger:** qualquer dos seguintes é breach (privacy_model §11.1):
> (a) acesso não autorizado a dado pessoal,
> (b) perda de integridade afetando dado pessoal,
> (c) indisponibilidade > 24h de dado pessoal.
>
> **SLA legal:** ≤ 72h para autoridade (GDPR Art. 33, LGPD Art. 48).

## Timeline obrigatória

| Passo | SLA | Owner |
|---|---|---|
| Detecção → declaração interna | ≤ 1h | Oncall (SEV-1) |
| Declaração → triage + containment | ≤ 4h | Security Lead + SRE |
| Triage → notificação ANPD/DPC | ≤ 48h interno / 72h legal | Privacy Officer + Legal |
| Notificação aos titulares afetados | ≤ 72h após autoridade | Privacy Officer + Comms |
| Post-mortem público | ≤ 14 dias | Privacy Officer + Security Lead |

## Step-by-step

### Fase 1: Detecção (0-1h)

1. Confirmar que é breach (não falso positivo).
2. Page Security Lead + Privacy Officer + Legal.
3. Preservar evidence (audit logs + snapshots + network traffic).
4. Escalate to CEO em ≤ 1h se confirmado.

### Fase 2: Containment (1-4h)

1. Determinar escopo:
   - Que tenants/titulares afetados?
   - Que categorias de dados? (privacy_model §2 classification)
   - Timeline do breach (início + duração)?
   - Como foi descoberto?
2. Stop the bleeding:
   - Revoke credenciais comprometidas
   - Patch bug de código
   - Isolate sistema afetado
3. Preparar draft de notificação.

### Fase 3: Notificação a autoridade (4-72h)

**Brasil (LGPD Art. 48):** ANPD em ≤ 72h via comunicacao@anpd.gov.br

- Template: `legal/breach-notification-anpd-template.md` (a criar)
- Conteúdo mínimo (Art. 48 §1):
  - Descrição da natureza dos dados
  - Titulares envolvidos
  - Medidas técnicas usadas
  - Riscos relacionados
  - Medidas para reverter / mitigar

**UE (GDPR Art. 33):** Lead Supervisory Authority em ≤ 72h
- Brasil não é UE mas temos tenants EU: Irish Data Protection Commission ou National DPA via `commissioners@dataprotection.ie`.
- Template: `legal/breach-notification-dpc-template.md` (a criar)
- Conteúdo conforme Art. 33(3).

### Fase 4: Notificação aos titulares (72h+)

- Apenas se "high risk to rights and freedoms" (GDPR Art. 34) ou "risco relevante" (LGPD Art. 48).
- Template: `legal/breach-notification-subject-template.md` (a criar)
- Conteúdo:
  - Descrição em linguagem clara
  - Dados afetados
  - Consequências prováveis
  - Medidas tomadas + recomendadas
  - Contato do DPO (Privacy Officer)

### Fase 5: Post-mortem (≤ 14d)

- Incident report público.
- Atualizar `privacy_model.md §11` se gaps descobertos.
- Revisar `failure_modes.md` se novo FM emergiu.

## Dry-run

Semestral (privacy_model §12). Simular breach + notificação, validar templates, contatos, timelines.

## Contatos chave (atualizar)

- **Privacy Officer (DPO interim):** Gustavo Schneiter (gustavo@humangr.com)
- **Legal (TBD — pré-GA backlog):** contratação de firma externa tracked em `compliance_matrix.md §9 GAP-01`; até lá Privacy Officer + CEO acumulam responsabilidade. Placeholder `legal@humangr.com` route interno.
- **ANPD (Brasil):** comunicacao@anpd.gov.br (fonte oficial ANPD)
- **Irish DPC (UE lead supervisory):** commissioners@dataprotection.ie (fonte oficial dataprotection.ie)
- **Cloudflare security contact:** cloudflare-cna@cloudflare.com (fonte: https://www.cloudflare.com/trust-hub/)

> **Nota:** todos os templates `legal/breach-notification-*.md` estão em backlog legal pré-GA (tracked em `compliance_matrix.md §9 gap analysis`). Até existirem, DPO + Legal usam templates padrão IAPP / ANPD quando necessário, com review em <1h do incident declaration.
