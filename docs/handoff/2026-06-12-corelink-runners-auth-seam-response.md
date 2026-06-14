# Resposta → corelink-runners: seam de auth + billing CONGELADO

**De:** corelink-server techlead · **Para:** corelink-runners techlead (via owner) ·
**Data:** 2026-06-12 · **Status:** CONGELADO — podem codar o `CoreLinkTokenStore`
contra este contrato. ·
**Em resposta a:** `corelink-runners/docs/handoff/2026-06-12-corelink-auth-runners-seam.md`

---

## TL;DR

- **Ask 1 (introspecção de PAT): CONSTRUÍDO + testado** no nosso lado — PR **#261**
  (`POST /internal/v1/auth/introspect`). Contrato congelado abaixo. Podem apontar o
  `CoreLinkTokenStore` pra ele já.
- **Ask 2 (billing de slot): tier = slots, sem SKU novo, entrega por PULL.** Nenhuma
  segunda integração Stripe. Detalhes abaixo.
- **Reconciliação de ladder:** o vocabulário canônico de tier é o do **CoreLink Cache**
  (Free/Solo/Starter/Pro/Max/Enterprise); a **concorrência** (slots) é um eixo SEPARADO,
  comprado standalone ou via bundle — **não** derivada do tier de cache.

---

## Shape 1 — PAT introspection (CONGELADO)

Construído sobre o nosso `PatVerifier` de produção (HMAC + D1 + Argon2id + scope gate),
cuja semântica de erro mapeia 1:1 no seu trait `TokenStore`.

```
POST /internal/v1/auth/introspect
X-Corelink-Internal-Auth: <fabric-service-secret>
Content-Type: application/json

{ "token": "corelink_pat_<token_id>.<secret>.<sig>" }
```

Respostas:
```jsonc
// válido + ativo  (PatVerifier::verify → Ok(tenant)):
{ "valid": true, "tenant_id": "<uuid>", "plan": "pro" }
//   max_concurrency / rate_ceiling_per_min: OMITIDOS no M1 (ver §Ask-2 / cap table).
//   Campos Option + skip-if-none → adicionados no M2 sem quebrar o contrato.

// inválido / expirado / revogado / desconhecido / sem scope de cache:
{ "valid": false }                 // NUNCA um tenant_id

// falha de backend (D1 inacessível / verifier error):
//   HTTP 503 (sem body)  →  seu Err(Unreachable)  →  503 ao cliente
```

**Mapeamento fail-closed (bate exatamente com o seu trait):**
- `VerifyError::InvalidPat` → `200 {valid:false}` → seu `Ok(None)` → 401.
- `VerifyError::Backend` → `503` → seu `Err(Unreachable)` → 503. **Jamais admissão anônima.**
- Secret de serviço ausente/errado → `401`.
- `tenant_id` presente **só** quando `valid:true`.

### Respostas às perguntas da §3

1. **Compatível / rota com outro nome?** Não existia rota; agora existe (#261), à sua
   shape. **Dois ajustes que pedimos:** (i) auth pelo header **`X-Corelink-Internal-Auth`**
   (reusa nosso gate constant-time auditado) em vez de `Authorization: Bearer` — se
   preferirem Bearer, trocamos, mas o header reusa o gate sem mudança; (ii) `plan` é
   opcional no M1 e os dois caps numéricos saem só no M2 (ver §Ask-2).
2. **`tenant_id` = mesmo espaço do Cache (org = tenant)?** **SIM, confirmado.** Uma
   ressalva: é um **UUID string** (`pat.tenant_id`, `TenantId(Uuid)`), não um slug tipo
   `"acme-corp"`. Keyem no UUID — é byte-idêntico ao que o Cache usa.
3. **Secret de serviço ou mTLS?** **Secret fixo**, constant-time, header — o mecanismo dos
   nossos `/_internal/*`. **Sem mTLS** na nossa stack. Provisionamos um secret
   **DEDICADO** pro fabric (`FABRIC_INTROSPECT_AUTH_KEY`, distinto da chave
   Worker↔container) pra manter o blast-radius apertado. Coordenamos a entrega do valor
   via owner no go-live.
4. **SLO de latência / degradação?** Caminho quente = HMAC-reject (sub-ms) + 1 lookup D1
   + 1 verify Argon2id (Argon2id é CPU-pesado de propósito, roda em thread bloqueante).
   Sugerimos timeout do cliente em **1–2s p99**; em falha de D1 devolvemos 5xx (seu
   `Unreachable`), nunca um `valid:false` falso. SLO exato a fixar após um load check —
   mantenham o timeout configurável (vocês já fazem).

---

## Shape 2 — Billing de slot (CONGELADO)

### Respostas às perguntas da §4

1. **SKU flat licensed-quantity (N slots) + price_id?** **Recomendamos NÃO criar um SKU de
   slot separado.** O modelo limpo: **tier = slots** — comprar um tier de Runners (ou um
   bundle que o inclua) já concede a concorrência; o cap sai do `max_concurrency` da
   introspecção. Reusa nossa subscription flat existente (`mode=subscription`), sem segunda
   integração Stripe, sem usage meter. Se vocês REALMENTE precisarem de N desacoplado do
   tier, aí sim criamos um `STRIPE_PRICE_ID_SLOTS` (licensed-quantity) — mas o default é
   tier=slots.
2. **Entrega do plano: push ou pull?** **PULL.** Vocês chamam o `/introspect` no acquire e
   recebem o cap inline — 1 round-trip, sem transporte novo, sem webhook fan-out pro
   fabric. Nosso webhook Stripe é inbound-only; um canal push pra terceiros seria um build
   bem maior — deferimos. **Pull no M1/M2.**
3. **Honra "slot = única unidade faturável"?** **SIM, por construção.** Nosso billing vivo
   não tem meter por request, por minuto, nem por cache-hit — o caminho de tier é uma
   subscription flat. O princípio se mantém do nosso lado hoje.

### A cap table (tier → concorrência) — o eixo de slots

A peça net-new. O `max_concurrency` **NÃO** deriva do tier de Cache — deriva da
**entitlement de Runners** do tenant (comprada standalone ou via bundle). A ladder
canônica de Runners (preços em revisão final do owner, concorrência estrutural):

| Entitlement de Runners | max_concurrency |
|---|---|
| (nenhum — tenant só-cache) | **0 / ausente** |
| Runners Starter | 20 |
| Runners Pro | 40 |
| Runners Team | 80 |
| Runners Scale | 160 |
| Runners Max | 320 |

→ Um tenant Cache-only (sem Runners) recebe `max_concurrency` ausente (vocês caem no
`StaticPlans` / 0). Um tenant com bundle (ex.: Build Stack) recebe a concorrência do
componente Runners do bundle (Build Stack = Pro = 40). **O `rate_ceiling_per_min` não
existe no nosso modelo** — não é dimensão de pricing nossa; recomendamos vocês dropparem
esse campo OU mantê-lo opcional/derivado do seu lado.

⚠️ **Mismatch de ladder a resolver no SEU lado:** o enum `PlanTier` em
`crates/corelink-fabric/src/plans.rs` (Free/Solo/Team/Scale @ 1/1/4/12) está 2 gerações
atrás da ladder canônica acima (Starter/Pro/Team/Scale/Max @ 20/40/80/160/320). Alinhem o
enum à ladder canônica antes do M2 GA. Os NOMES Starter/Pro/Max também colidem com os
tiers de Cache (significados/preços diferentes) — sugerimos prefixar `Runners:` nos seus.

---

## O que está pronto vs pendente

| Item | Estado |
|---|---|
| `POST /internal/v1/auth/introspect` (valid/tenant_id/plan) | ✅ **construído + testado** (#261, M1) |
| Gate de auth + secret dedicado `FABRIC_INTROSPECT_AUTH_KEY` | ✅ construído (fail-closed) |
| Resolver de `plan` (tier ativo → tenant.tier → free) | ✅ construído |
| `max_concurrency` na resposta | ⏳ **M2** — sai quando o owner ratificar o pricing (a cap table acima) + wirarmos a entitlement de Runners |
| Provisionar o valor do secret do fabric | ⏳ go-live (via owner) |

**Vocês podem começar AGORA** a codar o `CoreLinkTokenStore` contra o contrato congelado
da §Shape-1 — `valid`/`tenant_id`/`plan` já são servidos. O `max_concurrency` chega via
o mesmo endpoint no M2 (campo aditivo, não quebra nada). Nenhuma mudança no
`corelink-server` é exigida de vocês; nenhum `path`/`git`-dependency entre repos.

---

## Ação de volta pro owner

Falta **uma** ratificação sua pra fechar 100%: os **números de preço** da ladder de
Runners (a concorrência 20/40/80/160/320 é estrutural; os $ estão na proposta de pricing
`marketing/sales/pricing-proposal-v2.html` em revisão). Ratificado isso, adicionamos
`max_concurrency` ao endpoint (M2) e o seam está completo.
