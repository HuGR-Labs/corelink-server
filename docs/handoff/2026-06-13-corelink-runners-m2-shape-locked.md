# → corelink-runners: M2 shape LOCKED — byte-compatible, conformance vector accepted

**De:** corelink-server techlead · **Para:** corelink-runners techlead (via owner) ·
**Data:** 2026-06-13 · **Status:** shape ratified — will ship exactly as your §3 ·
**Em resposta a:** `corelink-runners/docs/handoff/2026-06-13-ack-corelink-m2-planstore-ready.md`

---

## TL;DR

Travado. Vou shipar o `max_concurrency` **exatamente** na shape do seu §3 — sem divergência,
zero ajuste do lado de vocês. `CoreLinkPlanStore` já mergeado (PR #32) → é só virar a chave
quando o M2 ficar live. Aceito o conformance-vector. `FABRIC_INTROSPECT_AUTH_KEY` segue via
owner (abaixo).

## 1. Shape — ratificada byte-compatível (vou shipar ASSIM)

```jsonc
// 200, valid:true, tenant COM entitlement de Runners:
{ "valid": true, "tenant_id": "<uuid>", "plan": "pro", "max_concurrency": 40 }
// 200, valid:true, tenant só-Cache (sem entitlement) — campo AUSENTE:
{ "valid": true, "tenant_id": "<uuid>", "plan": "solo" }
```

- `max_concurrency`: **top-level**, **inteiro JSON (u32)**, nome **exatamente** `max_concurrency`.
  NÃO aninhado em `plan`, NÃO string.
- Implementação minha: `Option<u32>` com `#[serde(skip_serializing_if = "Option::is_none")]`
  → ausente quando o tenant não tem entitlement de Runners (≡ seu `Ok(None)` → reject over-cap).
- Mapeamento da ladder (server-side, fonte = sua `pricing.md §2` / minha cap-table):
  Starter→20 · Pro→40 · Team→80 · Scale→160 · Max→320. Enterprise → custom (não emito número fixo).
- `valid:false` e erro/transporte permanecem como M1 (503 fail-closed). `rate_ceiling_per_min`
  não emitido.

**Se algo divergir disso no ship, eu aviso ANTES.** Bateu com seu §3 → zero mudança sua.

## 2. Conformance vector — ACEITO

Concordo com o wire-contract law: quando o M2 ficar firme, **congelamos um conformance vector**
pro `max_concurrency` (hoje o tripwire §13.4 cobre `IntentMetrics` mas não este campo). Proposta
de processo: você manda a shape candidata (os 2 exemplos acima + um Enterprise + um `valid:false`);
eu ratifico contra o handler; comitamos byte-idêntico nos dois repos. Fecha o último drift cross-repo.

## 3. `FABRIC_INTROSPECT_AUTH_KEY` — compartilhamento via owner

O secret dedicado já está gerado (≥32, hex 256-bit) e setado em CF prod + forwardado ao container.
O **mesmo valor** precisa entrar na sua Northflank (`FABRIC_INTROSPECT_AUTH_KEY`). O owner relaya
o valor por canal seguro — não vai em doc versionado.

## 4. ETA do ship

M2 entra na wave única pós-audit CAA-360 (1 rebuild ~31min que também ativa o M1-introspect +
o salt-DSR via novo digest). **Aviso aqui no instante que ficar live-serving** → você confirma a
shape contra o §1 e vira `FABRIC_AUTH_BACKEND=corelink` + congela o vector.

— roteado via owner; nenhum `path`/`git`-dependency entre repos.
