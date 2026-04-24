---
id: "RB-KEY-COMPROMISE"
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
tags: ["runbook", "p1", "security", "crypto", "key-management"]
---

# RB-KEY-COMPROMISE — Suspected or Confirmed Key Compromise

> **Escopo:** Root HSM, KEK-GLOBAL, TDK de tenant, audit chain key, release signing.
>
> **SLA:** revoke ≤ 1h; re-wrap completo ≤ 24h. Ver `key_management.md §7.1`.

## Triggers

- **Detecção**: credential em repo público (GitHub secret scanner alert), anomaly em KMS access log, insider report, alerta de cryptanalysis, vendor advisory.
- **Confirmação**: qualquer um dos acima + Security Lead review.

## Comunicação

- **SEV-1 imediato.** Page Security Lead + CEO + Privacy Officer + Legal.
- Comms prep: stakeholders externos (customers enterprise) pode precisar notification.
- Se release signing key: **parar builds + releases** até resolução.

## Mitigação imediata (≤ 1h)

### Se Root HSM comprometida

1. Switch CoreLink para `degrade_mode=emergency` (503 para tudo exceto health).
2. Rotate root key via CF API (requires break-glass + dual-approval).
3. Re-wrap todas KEKs subordinadas.
4. Audit log marcado com `integrity_hold=true` (CTRL-AUDIT-001).

### Se KEK-GLOBAL ou TDK de um tenant

1. Rotate key (cria nova key em state `pending`).
2. Promove nova key para `active`; old vira `rotated`.
3. Iniciar re-wrap background job (`corelink_key_rewrap_progress_ratio`).
4. Revoke todos os PATs emitidos que assinam com a key velha (CTRL-CRED-004, propaga 60s).

### Se release signing key

1. Revoke via Sigstore transparency log (cosign `revoke`).
2. Re-assinar todos os releases válidos com nova key.
3. Invalidate deploy caches CF.
4. Bloquear new deploys até verify chain restabelecido.

### Se audit chain key

1. Audit log continua append (forward-only).
2. Nova key inicia chain; link explícito ao último hash com old key.
3. Documentar break-point em incident report.

## Mitigação completa (≤ 24h)

1. Re-wrap 100% concluído (validar via `corelink_key_rewrap_progress_ratio == 1.0`).
2. Old key movida para `retired` (90d hold antes de destroy).
3. Verificar que nenhum artefato em prod ainda usa old key (property test).
4. Se supply chain attack suspeitado: SBOM scan + dep review.

## Forensics

1. Quando a key foi exposta? (timestamp do leak)
2. Quantos artefatos foram acessados com a key no período?
3. Customer enterprise com BYOK: qual a janela de exposição via suas chaves?
4. Preservar evidence: HSM access logs + CF account audit + KMS audit.

## Notificação (se confirmada compromise com customer data exposure)

- Trigger RB-BREACH-NOTIF (data breach notification).
- Customers enterprise com BYOK: notification imediata via DPA contact.
- SOC 2 auditor notify em ≤ 7d (se Type II active).

## Post-mortem obrigatório

- Incident report em ≤ 14d (público se customer data envolvido).
- ADR atualizado para prevenção.
- Rotation policy re-avaliada (frequency, automation).
- HSM provider security advisory check.

## Prevenção

- CTRL-KEY-001..021 (key_management §8).
- HSM-backed root (FIPS 140-2 L3).
- Annual rotation automática.
- Dual-approval para destructive ops (destroy, export).
- Semestral dry-run deste runbook.
- GitHub secret scanner + Gitleaks em CI (CTRL-CRED-001).
