---
id: "RB-FM-253"
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
tags: ["runbook", "p1", "tenant-isolation", "security", "highest-priority"]
---

# RB-FM-253 — Cross-Tenant Read (Security Bug)

> **FM:** FM-253 (S=5, O=2, D=4, RPN=40, P1) | **CTRLs:** INV-TENANT-ISOLATION (TLA+) + CTRL-ISO-001..005 | **SLA:** mitigate < 15 min
>
> **HIGHEST PRIORITY** — esse é o pior cenário do produto. Dry-run trimestral obrigatório.

## Detecção

- `corelink_isolation_assertion_total{outcome="violation"} > 0` (alert SEV-1 imediato).
- Property test em CI falha cross-tenant assertion.
- Customer report: "vi dado que não é meu" (raríssimo, mas possível).
- TLA+ re-check em model falha.

## Comunicação

- **SEV-1 IMEDIATO.** Page Security Lead + Architect + SRE Lead + CEO (blast radius comercial).
- Status page: `degraded` (sem revelar tenant isolation failure).
- Legal + Privacy Officer on-call: possível data breach notification.
- Comms framework: esperamos a análise para determinar escopo antes de customer notification.

## Mitigação imediata (≤ 15 min)

1. **Disable all reads no path afetado** via `degrade_mode=emergency` — CAS/AC retornam 503 `Retry-After: 3600`.
2. Snapshot do estado atual: D1 queries recentes, R2 access logs, audit events.
3. Identificar tenants envolvidos: vítima (dono dos dados expostos) + atacante/bug (quem leu).
4. Se incident relacionado a PAT comprometido: revoke imediato (CTRL-CRED-004, propaga 60s).

## Mitigação completa (≤ 4h)

1. Root cause analysis:
   - Bug de código? → hot-fix imediato.
   - HMAC derivação quebrada? → verify `tenant_path::derive_prefix()` lib.
   - R2 bucket policy drift? → terraform apply + audit.
   - TLA+ model não cobriu cenário observado? → atualizar spec.
2. Patch deploy via PAT-PROGRESSIVE-ROLLOUT-001 com error budget burn monitor.
3. Re-enable reads gradualmente (1% → 10% → 50% → 100%) APENAS após TLA+ green.

## Forensics

1. Audit log forense: quais blobs foram lidos? Por qual tenant? Quantos bytes?
2. Determinar exposure window: quando bug foi introduzido vs detected.
3. Honeypot tenants para detection (futuro): tenants com dado "canary" facilmente verificável.
4. Preservar evidence (R2 logs + D1 snapshot + Git blame) em R2 `evidence-incidents/` com Object Lock.

## Notificação obrigatória

- **Tenant vítima (dono dos dados)**: notificação formal em ≤ 48h conforme DPA; inclui escopo (que dados, quando, lidos por quem).
- **ANPD/DPA (LGPD) ou Irish DPC (GDPR)**: notificação em ≤ 72h conforme Art. 33 GDPR / LGPD Art. 48.
- **Tenant "atacante" (se bug, não ataque)**: informar; possivelmente enviou dados acidentalmente.
- **Bug bounty disclosure (se externo)**: 90d CERT/CC policy.

## Post-mortem obrigatório

- Incident report público em 14 dias (ou conforme DPA).
- TLA+ spec INV-TENANT-ISOLATION precisa cobrir cenário violado (regression test).
- Code review process atualizado se bug passou em PR review.
- Considerar promoção a FF-HR-002 + revisão de code review process.

## Prevenção

- Dry-run trimestral deste runbook (oncall + security).
- TLA+ INV-TENANT-ISOLATION CI check obrigatório (CTRL-FORMAL-001).
- Property test com 100k+ iter no CI.
- Canary tenants em staging (detection via invariant hash).
- 5 camadas defense-in-depth (`auth_model.md §8.1`).
