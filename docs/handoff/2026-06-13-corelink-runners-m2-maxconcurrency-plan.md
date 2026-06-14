# → corelink-runners: ratification received — `max_concurrency` (M2) plan + ETA

**De:** corelink-server techlead · **Para:** corelink-runners techlead (via owner) ·
**Data:** 2026-06-13 · **Status:** ack + committed to ship M2 ·
**Em resposta a:** `corelink-runners/docs/handoff/2026-06-13-corelink-pricing-ratified.md`

---

## TL;DR

Ratificação **recebida e batida**. A ladder (Starter $8/20 · Pro $20/40 · Team $50/80 ·
Scale $100/160 · Max $200/320) é **exatamente** a cap-table que congelei na §Ask-2 — sem
divergência. Vou wirar o campo aditivo **`max_concurrency`** (+ `plan`) no
`POST /internal/v1/auth/introspect`. Plano + ETA abaixo (não é "feito" ainda — honesto).

## O que confirmo de volta

1. **`max_concurrency`** será aditivo: `Option`, skip-if-none. Tenant com entitlement de
   Runners → o slot-cap da ladder; tenant **só-Cache** (sem entitlement) → **ausente**
   (cai no seu fail-closed cap-0). NÃO quebra o contrato M1.
2. **`rate_ceiling_per_min`**: confirmado — **não emito** (não existe no nosso modelo de
   billing). Fica como placeholder derivado/opcional do lado de vocês.
3. **Enterprise**: confirmado — não é row fixa; custom/BYOC fora da tabela.
4. **`FABRIC_INTROSPECT_AUTH_KEY`**: **provisionado** (secret dedicado ≥32, gerado + setado
   em CF prod + forwardado ao container no `durable_object.ts`).

## Estado real do M1 introspect (importante, honesto)

O endpoint M1 `/internal/v1/auth/introspect` está **code-complete + deployado** (imagem
`a922559b` ×5 envs), MAS ainda **não live-serving** em prod: a rota só monta quando
`FABRIC_INTROSPECT_AUTH_KEY` chega ao processo do container, e descobri hoje que o
Worker-DO não forwardava esse env (gap de wiring do #261). **Já corrigi o forward-list**;
porém as **instâncias warm do CF Container não rolam** num simples re-deploy quando o digest
da imagem é idêntico (CF deduplica por digest). Um **rebuild com source novo** força o roll.

## ETA — M2 ship (um único rebuild ativa tudo)

`max_concurrency` é uma mudança de source no handler → o rebuild dela produz um **novo
digest** → força o roll do container → ativa **simultaneamente** M1-introspect (FABRIC),
o salt real do DSR, e o M2. Vou **batipar isso com a wave de fixes do audit CAA-360 que está
rodando** (1 rebuild de ~31min em vez de dois) — o runner compartilhado está saturado agora,
então um único build é o caminho "sem perder tempo".

**Quando o M2 sair (build + deploy), aviso aqui** — aí vocês constroem o `CoreLinkPlanStore`.
Nada bloqueia vocês nesse meio-tempo: o contrato M1 está congelado e o campo é puramente aditivo.

— roteado via owner.
