---
id: "RB-BYOK-REVOKE"
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
tags: ["runbook", "p2", "byok", "enterprise", "kill-switch"]
---

# RB-BYOK-REVOKE — Customer Revoke BYOK Access (Kill Switch)

> **Trigger:** Customer enterprise revogou acesso do CoreLink ao CMK deles em seu KMS (AWS/GCP/Azure/Vault).
>
> **Por design:** cache inacessível em ≤ 5 min (CTRL-KEY-011 — customer kill switch).
>
> **Natureza:** este é comportamento esperado, NÃO incidente interno. Runbook para comunicação + validação.

## Detecção

- Métrica `corelink_byok_kms_errors_total{tenant_id}` spike.
- `kms.Decrypt` calls retornam `AccessDenied` / `KMSInvalidKeyUsageException`.
- Customer notificação prévia (via email/support).
- **Alert SEV-2** (não SEV-1 — é ação intencional do customer).

## Comunicação

- Slack `#oncall` + notify customer success manager do enterprise tenant.
- **Não** é emergency — customer já decidiu fazer.
- Se revoke foi acidental (customer confirma): escalate para Security Lead para ajudar restore.

## Fluxo (≤ 5 min)

### Se revoke intencional (customer deliberately rotating OU offboarding)

1. Stop wrap/unwrap operations para o tenant.
2. Marcar tenant como `kms_access_lost` em D1.
3. Cache efetivamente inacessível (por design).
4. Notificar customer success para confirmar next steps:
   - Rotation: aguardar new CMK; coordenar re-wrap.
   - Offboarding: iniciar DSR-erasure (ver `privacy_model §6.2`).

### Se revoke acidental

1. Customer re-grants acesso via suas IAM policies.
2. CoreLink re-wrap opera normalmente após CMK accessible.
3. Validate via `kms.DescribeKey` antes de re-enable writes.
4. Se janela de revoke foi longa (> 1h), customer pode pedir "status report" de writes perdidos.

## Forensics

- Audit log do customer KMS: quem revogou? Quando?
- Nosso audit log: último acesso bem-sucedido ao CMK.
- DPA: se customer alega erro nosso, apresentar evidence.

## Comunicação

- SLA: notificação em ≤ 1h (business hours) via customer success channel.
- Se offboarding: DPA + formal ACK necessário antes de purge.

## Prevenção

- CTRL-KEY-010..012 (BYOK multi-cloud + kill switch + audit export).
- Monthly test: customer faz "probe revoke" em staging para verificar kill switch timing.
- Runbook drill semestral.
- Documentação clara no onboarding: customer sabe que revoke = cache inacessível imediato.
