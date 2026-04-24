---
id: "RB-FM-254"
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
tags: ["runbook", "p1", "security", "cas", "integrity"]
---

# RB-FM-254 — Cache Poisoning (Hash Forjado Inserido no CAS)

> **FM:** FM-254 (S=5, O=2, D=5, RPN=50, P1) | **CTRLs:** CTRL-CAS-001 + CTRL-CAS-002 + client verify | **SLA:** mitigate < 15 min

## Detecção

- Customer report: "binário baixado ≠ hash que requisitei".
- `corelink_cas_client_verify_total{outcome="mismatch"}` > 0 (SEV-1 alert).
- Scrub periódico emite `corelink_scrub_mismatch_total` > 0.
- Supply chain audit de binário do cliente mostra divergência.

## Comunicação

- **SEV-1.** Page Security Lead + CEO (potencial supply chain attack impactando clientes).
- Legal on-call: notificação a clientes afetados.
- Assumir worst-case até prova em contrário: atacante pode ter inserido payload em pipeline de cliente.

## Mitigação imediata (≤ 15 min)

1. **Quarantine blob afetado** em D1: `blob_meta.quarantined_at = now`; reads retornam 503.
2. `degrade_mode=read-only` globalmente (previne mais envenenamento durante análise).
3. Notificar clientes que podem ter baixado blob contaminado (via webhook + email).
4. Rollback deploy recente se mudança em CAS write path (ver PAT-PROGRESSIVE-ROLLOUT-001).

## Mitigação completa (≤ 4h)

1. Identificar todos os blobs suspeitos:
   - Scrub completo: recomputa hash de todo blob vs `expected_digest`.
   - Cross-check com backup (R2 versioning).
2. Quarantine + purge dos contaminados.
3. Verificar CTRL-CAS-001 está ativo: write path rejeita `hash(body) ≠ digest`.
4. Se CTRL-CAS-001 funcional mas poisoning ocorreu: hash collision? Bug de normalization? Supply chain atacker tem write access?

## Forensics

1. Audit log do write: qual PAT? Qual IP? Qual User-Agent?
2. Cross-tenant check: atacker pôde escrever em outros tenants?
3. Cliente afetado: que binários foram served? Pipeline CI deles foi comprometido?
4. Preservar evidence em `evidence-incidents/` Object Lock.

## Notificação obrigatória

- **Clientes afetados**: notificação formal em ≤ 24h; incluir hashes dos blobs + timestamps + ações recomendadas (revalidar binários produzidos).
- **Security bulletin público**: publicar incident report em ≤ 14d.
- Se supply chain attack confirmado: coordenar com CISA / CERT.

## Post-mortem obrigatório

- Incident report em ≤ 14 dias.
- CTRL-CAS-001 test: write com body diferente do digest rejeitado? Regression test adicionado.
- Client verify forced-on em todos os SDKs (se opt-out era default).

## Prevenção

- CTRL-CAS-001 (content-addressable naming) + CTRL-CAS-002 (BLAKE3 verify on read) — defense in depth.
- Client-side verify sempre enabled por default (opt-out via header apenas).
- Scrub diário de 1% dos blobs (sample); mensalmente 100% dos blobs hot.
- Dry-run trimestral deste runbook.
