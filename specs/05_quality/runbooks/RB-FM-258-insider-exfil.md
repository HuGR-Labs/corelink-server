---
id: "RB-FM-258"
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
tags: ["runbook", "p1", "insider-threat", "privacy", "legal"]
---

# RB-FM-258 — Insider Data Exfiltration via Support Tool

> **FM:** FM-258 (S=5, RPN=25, P1 S=5→upgrade) | **CTRLs:** CTRL-PRIV-016 + CTRL-AUTH-010 + CTRL-AUDIT-003 | **SLA:** contain < 1h

## Contexto

Support tool legitimately permite SRE ler blob de tenant (ex: para debugar issue). Se SRE usa isso fora de ticket/consent = exfil. Detecção depende de audit rico + anomaly detection.

## Detecção

- Métrica `corelink_support_read_total{sre_id}` spike anômalo.
- Volume atípico: um SRE ler > 100 blobs em 1h sem ticket correspondente.
- Audit log shows accesses sem consent_token associado (CTRL-PRIV-016).
- Tip de outro SRE / manager.
- Whistleblower externo (raro mas possível).

## Comunicação

- **SEV-1.** Page Security Lead + Privacy Officer + CEO + Legal.
- **Não** envolver o SRE suspeito na comunicação.
- Se confirmado: HR + possível polícia (depending jurisdiction).

## Triage (≤ 30 min)

1. Preservar evidence ANTES de fazer nada:
   - Audit log backup imediato (R2 Object Lock já faz, mas confirmar).
   - Network logs CF.
   - SRE's endpoint (se company laptop).
2. Confirmar padrão anômalo (não falso positivo de genuíno debug).
3. Verificar: quais blobs? Quais tenants? PII envolvido?

## Mitigação imediata (≤ 1h)

1. **Revoke acesso do SRE** (SSO + PATs + session):
   - IAM role: remover admin/support.
   - Revoke PATs via CTRL-CRED-004 (propaga 60s).
   - Force session logout.
   - Collect laptop se on-site.
2. Notificar tenants afetados (dentro de 72h para conformidade).
3. Trigger RB-BREACH-NOTIF se PII/tenant data leaked.
4. Legal hold em tudo relacionado.

## Forensics (≤ 1 week)

1. Full audit trail do SRE: o que foi lido, quando, de onde.
2. Cross-check com ticket system: reads correspondem a tickets?
3. Data exfil method: download? Screen capture? Send to external?
4. Collaboration with HR + Legal + potentially law enforcement.

## Notificação

- **Tenants cujos dados foram acessados sem consent**: notificação em ≤ 72h conforme DPA.
- **ANPD/DPA**: ≤ 72h se PII envolvido (GDPR Art. 33 / LGPD Art. 48).
- **HR action** conforme policy interna.
- **Security bulletin público** (sem detalhes pessoais) se impact > 100 customers ou PR-worthy.

## Post-mortem

- Obrigatório.
- Review CTRL-PRIV-016 (consent required): funcionou? Foi bypassed?
- Review CTRL-AUDIT-003 (MFA attestation): foi loggado corretamente?
- Update policy + training.

## Prevenção

- CTRL-PRIV-016: support read requer consent token + MFA + time-boxed (15 min).
- CTRL-AUDIT-003: MFA attestation em admin ops + session recording.
- Anomaly detection em métricas de support access (dashboard dedicado).
- Separation of duties: support tool separado de admin panel.
- Annual training em privacy + insider threat awareness.
- Bug bounty de insider threat (reward tipped colleagues).
