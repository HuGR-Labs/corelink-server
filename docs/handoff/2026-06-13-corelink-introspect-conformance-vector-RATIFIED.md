# → corelink-runners: introspect conformance vector — RATIFIED (byte-exact) + 1 semantic flag

**De:** corelink-server techlead · **Para:** corelink-runners techlead (via owner) ·
**Data:** 2026-06-13 · **Status:** ratified against the real handler — commit the vector + golden tests ·
**Em resposta a:** `corelink-runners/docs/handoff/2026-06-13-corelink-introspect-conformance-vector-proposal.md`

---

## TL;DR

Ratifiquei os 4 casos contra a serialização REAL do `IntrospectResponse`
(`crates/corelink-container/src/routes/auth_introspect.rs`). **As 4 shapes batem byte-exato** —
podem comitar `conformance/corelink-introspect.json` + os golden tests. Respondi as 5 perguntas
abaixo. **UMA flag semântica** sobre como o `max_concurrency` é derivado (não muda a shape do
vector, mas precisa de reconciliação antes do eixo de slots existir de verdade).

## Os 4 casos — RATIFICADOS (a serialização exata do handler)

```jsonc
// CASE 1 — valid + entitlement → cap. Ordem serde: valid, tenant_id, plan, max_concurrency.
{ "valid": true, "tenant_id": "11111111-1111-4111-8111-111111111111", "plan": "pro", "max_concurrency": 40 }
// CASE 2 — valid + cache-only / sem entitlement → max_concurrency AUSENTE.
{ "valid": true, "tenant_id": "22222222-2222-4222-8222-222222222222", "plan": "solo" }
// CASE 3 — valid + enterprise → max_concurrency AUSENTE (ladder `_`-arm → None).
{ "valid": true, "tenant_id": "33333333-3333-4333-8333-333333333333", "plan": "enterprise" }
// CASE 4 — invalid → SÓ valid:false, nada mais.
{ "valid": false }
```
Todos batem. (CASE 1 é shape-correto mas **ainda não é produzível em prod hoje** — ver flag §B.)

## Respostas às 5 perguntas

1. **Ordem + chaves no wire:** `valid, tenant_id, plan, max_concurrency` (ordem de declaração do
   serde). Os 3 últimos são `Option` com `skip_serializing_if=Option::is_none`; `valid:false` →
   só `{"valid":false}`. ⚠️ Existe um **5º campo na struct, `rate_ceiling_per_min: Option<u32>`,
   mas o construtor o seta SEMPRE como `None`** → **nunca aparece no wire** (confirma "não emito").
2. **Valores de `plan` (closed set):** os **8 tiers de CACHE**, lowercase snake_case, espelhando
   `TierKind::as_str`: **`free, solo, starter, team, pro, org, max, enterprise`**. Default = `free`.
   Um valor de D1 fora desse set é tratado como ausente (cai no default-tier). **NÃO inclui
   `scale`** (esse é um tier de Runners, não de Cache — ver §B).
3. **Casing do `tenant_id`:** UUID **lowercase RFC-4122 hifenizado** (ADR-0002; o tenant_id É o
   UUID verbatim). Um UUID representativo lowercase por caso, como vocês propuseram.
4. **Enterprise (case 3):** `max_concurrency` **genuinamente AUSENTE** — `max_concurrency_for_plan`
   tem `_ => None` e o `skip_serializing_if` omite o campo (não é `0`, não é `null`). `plan` é
   literalmente `"enterprise"`. ✓
5. **Campo omitido que eu emito?** Só o `rate_ceiling_per_min` da §1 — e ele **nunca vai pro wire**
   (sempre None). Nenhum `expires_at`, nenhum `scopes`. A shape do vector reflete os bytes reais.

## §B — FLAG semântica (não bloqueia o vector; reconciliar antes do go-live de Runners)

Hoje `max_concurrency_for_tenant = if entitlement { max_concurrency_for_plan(plan_de_CACHE) } else
{ None }`, com `entitlement` = stub `false`. Duas consequências:
- **O cap é derivado do tier de CACHE do tenant**, não de um tier de Runners independente. Ex:
  entitlement + cache-plan `pro` → 40. Mas o eixo de slots de vocês (Starter/Pro/Team/**Scale**/Max)
  é separado do tier de Cache — e meu `plan` emite o tier de **Cache** (que tem `org`/`solo`, e
  **nunca `scale`**). A ladder tem `scale→160`, mas como nenhum `plan` de cache é `"scale"`, esse
  arm é **dead code hoje**.
- **Pergunta de design pra quando a entitlement store landar:** o `max_concurrency` deve vir (a) do
  tier de Cache do tenant via ladder (meu código atual), ou (b) de uma **subscription de Runners
  separada** (o modelo de eixo-separado de vocês, onde "Scale" é uma compra independente)? Se for
  (b), eu troco a derivação pra ler o tier de Runners da entitlement store, não o `plan` de cache.
  **Isso NÃO afeta a shape do conformance vector** (o campo é o mesmo); afeta só DE ONDE eu puxo o
  número. Quando vocês definirem a fonte da entitlement, me avisem e eu wiro a derivação correta.

## Próximo passo

Comitem `conformance/corelink-introspect.json` byte-idêntico nos 2 repos + os golden tests. Eu
adiciono um golden test do meu lado que serializa cada caso e diffa contra o JSON commitado
(tamper → fail), espelhando o §13.4. A flag §B fica trackeada pra quando a entitlement de Runners
existir — até lá, todo tenant cai no CASE 2 (cap-0 fail-closed de vocês), que é o esperado.

— roteado via owner; nenhum `path`/`git`-dependency entre repos.
