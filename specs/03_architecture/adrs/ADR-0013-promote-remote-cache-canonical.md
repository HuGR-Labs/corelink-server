---
id: "ADR-0013"
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
related_adrs: ["ADR-0012", "ADR-0014"]
tags: ["adr", "canonical-source", "inheritance", "framework"]
---

# ADR-0013: Promover `REMOTE-CACHE-PRODUCT-PROFILE` como Canonical Source de Nível 3

> **doc_status:** FROZEN
> **Versão:** 1.0.0
> **Última atualização:** 2026-04-24
> **Owner:** Gustavo Schneiter
> **Aprovador Final:** Gustavo Schneiter
> **Supersedes:** —
> **Superseded By:** —

## 0. Contexto

Audit Lote 3+4 (GPT F-02) identificou que `remote_cache_product_profile.md`:

- Usa linguagem normativa de inheritance: `inherits_from: ["REMOTE-CACHE-PRODUCT-PROFILE"]` (linha 30).
- **Não está listado** como canonical source no `00_framework.md §35.5.1`.
- No schema, é classificado como `type: "protocol"` em vez de um tipo dedicado.
- Não tem `doc_status: FROZEN` para que `validate_specs.py` aceite o `inherits_from` (REG-INHERIT-001).

Resultado: o doc atua como canonical source *de facto* mas não *de jure*. WIs que herdam ficam ambíguos (foi promovido? está em DRAFT?). Adicionalmente, `invariant_registry.md` (criado neste mesmo lote) e `key_management.md` (BYOK, criado neste mesmo lote) também precisam ser canonical sources.

## 1. Decisão

Promover formalmente 3 docs ao §35.5.1 como canonical sources de Nível 3:

1. `specs/03_architecture/remote_cache_product_profile.md` — fonte canônica de CAS, AC, GC, dedup, eviction, REAPI conformance.
2. `specs/03_architecture/invariant_registry.md` — fonte canônica de TODOS os IDs INV-XXX (resolve S-07: naming inconsistente).
3. `specs/03_architecture/key_management.md` — fonte canônica de KMS, BYOK, BYOE, key rotation (resolve F-13: BYOK gaps SOTA).

Os 3 entram em §35.5.1 com status `Nível 3`.

## 2. Alternativas consideradas

| Opção | Pró | Contra |
|---|---|---|
| **Manter remote_cache como `protocol`** | Sem mudança | Continua ambíguo; auditoria fica sem trilha |
| **Criar canonical sources individuais (escolhida)** | Trilha clara; inheritance funciona; CI pode validar | Bump minor no framework + 2 novos docs |
| **Mesclar tudo em `data_model.md`** | Menos arquivos | Viola PRINC-006 (responsabilidades separadas); doc fica 2000 linhas |

## 3. Consequências

- **Positivas:**
  - `inherits_from: ["REMOTE-CACHE-PRODUCT-PROFILE"]` agora é semanticamente válido.
  - Schema mantém `type: "protocol"` (não precisa novo enum) mas o status canônico vem de §35.5.1, não do schema.
  - Invariant registry centraliza nomenclatura (resolve S-07).
- **Negativas:**
  - Mais 2 docs canônicos pra manter (`invariant_registry.md`, `key_management.md`).
  - Pessoas precisam ler `remote_cache_product_profile.md` antes de tocar CAS/AC.

## 4. Implementação

- [x] Atualizar `specs/00_framework.md §35.5.1` com 3 novas linhas.
- [x] Bump framework v0.5.0.
- [x] Mudar `remote_cache_product_profile.md` para `doc_status: REVIEW` (caminho para FROZEN após este ADR).
- [x] Criar `specs/03_architecture/invariant_registry.md` (feito Lote 5.4).
- [x] Criar `specs/03_architecture/key_management.md` (feito Lote 5.4).

## 5. Evidence

- Audit findings: `specs/_audits/sealed/2026-04-24-gpt-audit-lote3-4.md` (F-02), `2026-04-24-sonnet-audit-lote3-4.md` (S-07, F-13).

---

**Status final:** FROZEN. Mudanças requerem novo ADR (supersede).
