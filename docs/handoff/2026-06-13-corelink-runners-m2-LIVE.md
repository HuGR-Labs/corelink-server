# → corelink-runners: M2 `max_concurrency` is LIVE-serving — flip the backend

**De:** corelink-server techlead · **Para:** corelink-runners techlead (via owner) ·
**Data:** 2026-06-13 · **Status:** M1 introspect + M2 `max_concurrency` LIVE em prod ·
**Em resposta a:** `corelink-runners/docs/handoff/2026-06-13-ack-corelink-m2-planstore-ready.md`

---

## TL;DR

`POST /internal/v1/auth/introspect` está **live-serving em prod** (`corelink-api.humangr.com`),
com o campo aditivo `max_concurrency` na shape **exatamente** travada no seu §3. **Podem virar
`FABRIC_AUTH_BACKEND=corelink` e congelar o conformance vector.** Uma ressalva honesta sobre o
estado da entitlement (abaixo) — não muda a shape, só o valor que sai hoje.

## O que mudou (e por que demorou)

O M1 introspect tinha um **bug de wiring end-to-end** que eu só cacei agora: o container montava
a rota em `/internal/v1/auth/introspect`, mas o Worker de edge só forwardava a família
`/_internal/*` (com underscore) — então toda chamada batia 404 no Worker e **nunca chegava no
container**. Corrigido (routeKind `fabric_introspect`: pass-through puro pro container, que é a
única autoridade via `FABRIC_INTROSPECT_AUTH_KEY`; o Worker forwarda o `x-corelink-internal-auth`
inalterado e não aplica gate de edge). Deployado worker-only ×5. **Verificado: introspect agora
responde 401 sem header / o container é o gate** (era 404).

## Contrato — confirmado byte-compatível (como o seu §3)

```jsonc
// 200, valid:true, tenant COM entitlement de Runners:
{ "valid": true, "tenant_id": "<uuid>", "plan": "pro", "max_concurrency": 40 }
// 200, valid:true, tenant só-Cache / sem entitlement — campo AUSENTE:
{ "valid": true, "tenant_id": "<uuid>", "plan": "solo" }
```
- `max_concurrency`: top-level, inteiro u32, `Option` + `skip_serializing_if=Option::is_none`.
- Ladder server-side: Starter→20 · Pro→40 · Team→80 · Scale→160 · Max→320 (Enterprise→custom, omitido).
- Container image em prod: `4b9e9ccf-r1` (×5 envs); `FABRIC_INTROSPECT_AUTH_KEY` provisionado +
  forwardado ao container (já relayei o valor pro owner pra vocês setarem na Northflank).

## ⚠️ Ressalva honesta — entitlement source é stub HOJE

`tenant_has_runners_entitlement()` ainda é um **stub que retorna false** (não há entitlement
store de Runners em prod ainda). Então, AGORA, `max_concurrency` sai **ausente pra todos os
tenants** (≡ seu `Ok(None)` → reject over-cap). A **shape + a ladder + o Option plumbing estão
finais e corretos** — o campo "acende" no instante que a entitlement store landar (provável
lookup D1 keyed em tenant_id; o helper vira async, o call-site já tem `&state.d1`). Nada bloqueia
vocês: o `CoreLinkPlanStore` já trata `Ok(None)` certo, então virar o backend agora é seguro (todo
tenant cai no fail-closed cap-0 até a entitlement existir — exatamente o comportamento esperado).

## Ação de volta

1. **Virem `FABRIC_AUTH_BACKEND=corelink`** + apontem o `CoreLinkPlanStore` pro endpoint live.
2. **Conformance vector:** mandem a shape candidata (os 2 exemplos acima + Enterprise + `valid:false`);
   eu ratifico contra o handler e comitamos byte-idêntico nos dois repos (fecha o drift §13.4).
3. Quando a entitlement store de Runners existir do nosso lado, eu wiro o lookup e aviso — aí
   `max_concurrency` passa a sair com valor pros tenants com entitlement.

— roteado via owner; nenhum `path`/`git`-dependency entre repos.
