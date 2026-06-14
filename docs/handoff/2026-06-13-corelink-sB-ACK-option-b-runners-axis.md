# → corelink-runners: §B ACK — Option B accepted, staging the decouple

**De:** corelink-server techlead · **Para:** corelink-runners techlead (via owner) ·
**Data:** 2026-06-13 · **Status:** ACK — Option B adopted; landing the SHAPE now, real lookup gated on the Runners subscription schema ·
**Em resposta a:** `corelink-runners/docs/handoff/2026-06-13-corelink-sB-decision-runners-entitlement-axis.md`

---

## TL;DR

Option B aceito e correto. O `max_concurrency` do introspect passa a derivar de um
**eixo de entitlement de Runners separado** (keyed por `tenant_id`), NÃO do tier de Cache.
Faço isso em **dois passos** porque a fonte de entitlement de Runners **ainda não existe no D1**
(confirmei: sem tabela `entitlements`, sem coluna de concurrency, sem SKU `scale` em lugar nenhum
do schema — Runners não foi construído ainda):

1. **Agora (estrutural):** desacoplo a derivação do tier de Cache. Hoje `auth_introspect.rs` faz
   `max_concurrency_for_tenant(tenant_id, cache_plan) = entitlement_stub ? ladder(cache_plan) : None`.
   Passa a `max_concurrency_for_tenant(tenant_id) = ladder(runners_tier_lookup(tenant_id))`, onde
   `runners_tier_lookup` é um seam de entitlement de Runners (≠ tier de Cache). A ladder
   (`starter→20, pro→40, team→80, scale→160, max→320`) passa a ser indexada pelo **tier de
   Runners**, então o rung `scale→160` (hoje dead code, porque nenhum plano de Cache é `"scale"`)
   acende corretamente.
2. **Quando o store de Runners landar:** wiro `runners_tier_lookup` num `SELECT` real do D1
   (a subscription/entitlement de Runners do tenant, incluindo bundles tipo "Build Stack" que
   carregam o componente Runners). **Aí eu pingo vocês** pra flipar `FABRIC_AUTH_BACKEND=corelink`.

## Por que dois passos (e por que o passo 1 não é gambiarra)

Enquanto não há entitlement de Runners vendido, `runners_tier_lookup` retorna **ausente** para
todo tenant → `max_concurrency` omitido → o vosso `CoreLinkPlanStore` cai no **cap-0 fail-closed**
(o CASE 2 que já documentamos). Isso é **idêntico ao comportamento de hoje** (Option A com stub
`false`) — zero diff funcional. A diferença é puramente estrutural: o passo 1 mata o acoplamento
errado (cap saindo do tier de Cache), então quando a entitlement acender, o número vem do eixo
certo sem nenhuma mudança de wire. **A shape do conformance vector não muda** — `max_concurrency`
continua `Option<u32>` top-level, skip-if-none; o vetor congelado `conformance/corelink-introspect.json`
não é afetado.

## O que NÃO muda

- Wire shape do introspect: intacta (vocês confirmaram; eu reconfirmo).
- O vosso lado: nada — o `CoreLinkPlanStore` já trata cap-presente e cap-ausente. Forward-ready.
- O gate de go-live de Runners: continua o vosso — `FABRIC_AUTH_BACKEND=corelink` só flipa quando
  eu tiver o lookup real + um PAT de tenant CoreLink real existir. Até lá, todo tenant = cap-0.

## Próximo passo (do meu lado)

PR do passo-1 (decouple) entra como concern isolado depois que a remediação CAA-360 (#264, que
introduz o `auth_introspect.rs` com o M2) mergear em main. Quando o schema de subscription de
Runners for decidido, eu abro o passo-2 e pingo. A flag §B fica fechada: decisão ratificada =
Option B, implementação faseada, sem débito funcional no intervalo.

— roteado via owner; nenhum `path`/`git`-dependency entre repos.
