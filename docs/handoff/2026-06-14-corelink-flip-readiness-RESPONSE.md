# → corelink-runners TL: flip-readiness — status + plan per item

**De:** corelink-server techlead · **Para:** corelink-runners techlead (via owner) ·
**Data:** 2026-06-14 · **Em resposta a:** `2026-06-14-corelink-flip-readiness-checklist.md`

---

## Status por item

| # | Item | Estado |
|---|------|--------|
| 1 | Runners-entitlement lookup (§B real) | ⏳ **em construção** — shape decouplada feita; lookup D1 real = próximo (plano abaixo) |
| 2 | `max_concurrency` wire shape byte-idêntica ao vetor `bfb38e28` | ✅ **FEITO** |
| 3 | PAT de tenant real pro fabric introspectar | ⏳ eu minto no flip (coordeno c/ owner) |
| 4 | `FABRIC_INTROSPECT_AUTH_KEY` | ✅ feito (confirmo current no flip) |

## Item 2 — FEITO ✅

Espelhei o vetor congelado **byte-idêntico** em `conformance/corelink-introspect.json`
(sha256 `bfb38e28…`, os 4 casos) + um **golden test** (`introspect_shape_matches_frozen_conformance_vector`)
que serializa cada `IntrospectResponse` e diffa contra o vetor. É o tripwire cross-repo: qualquer
drift de `max_concurrency`/`tenant_id`/`plan` quebra os dois golden tests. **A shape NÃO diverge** do
`bfb38e28` — zero mudança do vosso lado.

## Item 1 — decisão + plano (§B real lookup)

A shape já está correta (§B: `max_concurrency` vem do **eixo de Runners**, não do tier de cache;
ladder reindexada; `runners_tier_for_tenant` seam; commit `c6073909`). O que falta pro lookup ser
**real** (vs stub-None):

- **Tabela D1 nova `runners_entitlement`** (`tenant_id` PK → `runners_tier` ∈
  starter/pro/team/scale/max, + audit cols). `runners_tier_for_tenant` vira um **SELECT D1 real**
  (mirror do `tier_for_tenant`: fault → 503 fail-closed, nunca cap errado).
- **Empty table = todo tenant cap-absent** (o vosso cap-0 fail-closed). Ou seja: **o flip valida a
  integração end-to-end (os 3 arms) imediatamente**, mesmo sem nenhum entitlement vendido.
- **Pra um tenant USAR runners de verdade:** uma row em `runners_entitlement` (semeada pelo
  billing quando comprarem runners/bundle — ex. "Build Stack" = `pro`; ou eu semeio o tenant de
  **dogfood** manualmente se o owner indicar qual). **Quem semeia a 1ª row = decisão owner/product.**

Construo isto batched com o §B no **próximo rebuild M2** (gated no merge-train wave-1 assentar +
o Mac liberar pra compile-verify — está saturado pelo CI agora).

## Item 3 — PAT de tenant real

No flip, eu **minto um PAT** pra um tenant real via o `/_internal/pat/mint` de prod (ex. `humangr`,
se existir no D1 prod — confirmo). É credencial viva → vai **via owner** pro vosso secret Northflank.
Coordeno no dia do flip (batched com 1).

## Handshake

Eu **pingo quando 1 + 3 estiverem live** (batched com o rebuild M2), como pedido. Item 2 está
mirror-and-confirmed; item 4 feito. Custo dos 2 round-trips/acquire: aceito, sem ação minha.
A shape continua `bfb38e28` — se algum dia eu precisar divergir, aviso ANTES do ship.

— roteado via owner; nenhum `path`/`git`-dependency entre repos.
