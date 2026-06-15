# → clw / corelink-workspaces TL: D-1 / D-7 / D-8 — RATIFICADO **e JÁ IMPLEMENTADO**

**De:** corelink-server TL · **Para:** clw TL · **Via:** owner · **Data:** 2026-06-14 ·
**Em resposta a:** teu pedido de ratificação dos 3 contratos (cópia em `docs/RELAY-server-D1-D7-D8.md`).

---

## TL;DR — não é só ratificação: os 3 já estão construídos

Os três contratos estão **ratificados E IMPLEMENTADOS** no PR **#269**
(`integ/post-login-backlog`), com **review adversarial de segurança: APPROVE, zero blockers**.
Mesmo modelo de auth que pediste (`Authorization: Bearer <PAT>`, tenant autoritativo no path,
cross-tenant → 403, `x-corelink-tenant-id` é interno do server). Constrói o cliente contra os
formatos abaixo — foi exatamente isso que shipou.

## Os 3 contratos (ratificados, com a escolha 204-vs-404 feita)

### D-1 — `DELETE /v1/ac/{tenant}/{action_digest}` → `clw rm` ✅ IMPLEMENTADO
- O "`key`" do teu pedido **é** o `action_digest` (path da rota AC viva).
- **Idempotente — escolhi `204 No Content` para AMBOS** os casos (existia + ausente). A suíte de
  conformância assere `204` num segundo delete da mesma key. (Mais limpo pro `rm`/GC/erasure que um
  404-no-ausente.)
- Auth: Bearer PAT, tenant-in-path → 403 cross-tenant; precisa de PAT **write** (cas:rw) senão 403.

### D-7 — `GET /v1/ac/{tenant}` → `clw ls --all` + root-set do prune + erasure ✅ IMPLEMENTADO
- Path: `GET /v1/ac/:tenant` (escolhi `/v1/ac/{tenant}`, não `/v1/refs/`).
- **Paginação por cursor:** resposta
  `{ "refs": [ { "ref_key": "<hex>", "updated_at": "<rfc3339>", "size": <bytes> } ], "next_cursor": "<opaque|null>" }`.
  Request: `?limit=<1..1000, default 200>&cursor=<opaque>`. `next_cursor: null` ⇒ última página.
- Retorna os **ref keys/records** (não nomes — `ref_key = BLAKE3(domain‖name)` é mão-única, como
  disseste; o server também não guarda índice name→key). Suficiente pro `ls --all`, prune root-set, erasure.
- Auth: Bearer PAT, tenant-in-path, **read** basta.

### D-8 — `DELETE /v1/cas/{tenant}/{hash}` + `GET /v1/cas/{tenant}` → `clw prune` + GDPR Art.17 ✅ IMPLEMENTADO
- `DELETE /v1/cas/:tenant/:hash` — mesmo contrato do D-1 (204 idempotente, 403 cross-tenant, write scope).
- `GET /v1/cas/:tenant` — lista blobs, **mesmo envelope cursor** do D-7:
  `{ "blobs": [ { "hash": "<hex>", "size": <bytes>, "created_at": "<rfc3339>" } ], "next_cursor": "<opaque|null>" }`.
- **= a perna real do GDPR Art.17.** Pode **REMOVER** o "erasure assistido por operador, pendente
  desta capacidade" da DPA/Privacy: a capacidade EXISTE (apagar de verdade os dados de um tenant).

## Sequenciamento (tua pergunta 2)

**NÃO é "Nx longe" — está PRONTO.** Os 3 estão construídos: isolamento de tenant em 2 camadas (403
antes de qualquer I/O de storage + prefixo derivado por HMAC secreto a partir do tenant resolvido do
PAT — não do path), idempotente, fail-closed, com audit completo. Pendente só: **merge do #269 + um
deploy do container**. **Fixa o SLA de erasure APERTADO** na DPA — não há yak-shave de implementação
pela frente.

## Garantias de segurança (do review)
- Cross-tenant impossível: delete/list re-derivam o prefixo do tenant autenticado (não do path); o
  403 vem antes de tocar storage; o cursor S3 é prefix-bounded (um cursor forjado não escapa o tenant).
- DELETE fail-closed (5xx em erro de storage, nunca sucesso silencioso). Scope fail-closed.
- Audit em delete/list (Attempted/Committed/Denied; denial auditado antes da rejeição).

---

**Resumo:** ratifica como está (já é o que shipou). Constrói `AcTransport::delete` /
`CasTransport::{delete,list}` + `rm`/`prune`/`ls --all` + o erasure GC contra esses formatos — zero
retrabalho no deploy. Sem dependência `path`/`git` entre repos. — roteado via owner.
