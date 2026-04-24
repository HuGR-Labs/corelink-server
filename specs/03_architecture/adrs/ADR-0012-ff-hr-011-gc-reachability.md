---
id: "ADR-0012"
type: "adr"
doc_status: "FROZEN"
audit_status: "ACTIVE"
version: "1.0.0"
created: "2026-04-24"
updated: "2026-04-24"
owner: "Gustavo Schneiter"
final_approver: "Gustavo Schneiter"
reviewers: []
supersedes: null
superseded_by: null
related_adrs: ["ADR-0013", "ADR-0014"]
tags: ["adr", "risk-lanes", "garbage-collection", "framework"]
---

# ADR-0012: Adicionar `FF-HR-011` — Garbage Collection / Invariant de Reachability

> **doc_status:** FROZEN
> **Versão:** 1.0.0
> **Última atualização:** 2026-04-24
> **Owner:** Gustavo Schneiter
> **Aprovador Final:** Gustavo Schneiter
> **Supersedes:** —
> **Superseded By:** —

## 0. Contexto

Audit Lote 3+4 (Sonnet S-15, GPT F-02) identificou que `remote_cache_product_profile.md §15` propôs `FF-HR-011` como forcing factor para mudanças em GC e refcount, mas o framework `00_framework.md §33.5.3` só define `FF-HR-001..010`. Sem este forcing factor:

- WI/Sprint que mudam algoritmo de GC podem ser tratados como `STANDARD` lane sem que o autor perceba o risco de blast radius cross-tenant.
- `INV-GC-001` (reachable never deleted) é CRITICAL mas seu único forcing factor mais próximo (`FF-HR-005` = controle de segurança) não captura corretamente a semântica de integridade de dados.
- Failure modes `FM-300`, `FM-303`, `FM-404` (todos relativos a refcount/race em GC) ficam descobertos pelo sistema de risk lanes.

## 1. Decisão

Adicionar `FF-HR-011` ao framework `§33.5.3`:

> **FF-HR-011**: Altera algoritmo de garbage collection, refcount, ou invariante de reachability (INV-GC-*) com blast radius em integridade de dados.

## 2. Alternativas consideradas

| Opção | Pró | Contra |
|---|---|---|
| **Reusar FF-HR-005** | Sem mudança no framework | FF-HR-005 = segurança; GC não é primariamente segurança; classificação imprecisa esconde risco |
| **Reusar FF-HR-006** | Já cobre "retention/GC de customer data" | FF-HR-006 = política de retenção; algoritmo é diferente; ambíguo |
| **Adicionar FF-HR-011 (escolhida)** | Captura semântica exata; alinhado com FM-300/303/404 | Bump minor no framework |

## 3. Consequências

- **Positivas:**
  - Qualquer WI tocando GC algorithm é automaticamente `HIGH_RISK` → exige TLA+ + 10-12 sign-offs + chaos testing.
  - Cobre gap identificado pelo audit Lote 3+4.
- **Negativas:**
  - Bump minor no framework (v0.4.2 → v0.5.0).
  - WIs futuros pra GC têm cerimônia maior (mas é a cerimônia certa pro risco).

## 4. Implementação

- [x] Adicionar `FF-HR-011` em `specs/00_framework.md §33.5.3`.
- [x] Bump versão framework v0.5.0.
- [x] Atualizar `remote_cache_product_profile.md §15` para citar FF-HR-011 sem disclaimer "(novo, proposto)".
- [x] Atualizar `failure_modes.md §3.7` (FM-300, FM-404) com referência ao novo forcing factor (Lote 7.3).

## 5. Evidence

- Audit findings: `specs/_audits/2026-04-24-sonnet-audit-lote3-4.md` (S-15) e `2026-04-24-gpt-audit-lote3-4.md` (F-02).
- Framework atualizado: `specs/00_framework.md §33.5.3` linha ~2055.

---

**Status final:** FROZEN. Mudanças requerem novo ADR (supersede).
