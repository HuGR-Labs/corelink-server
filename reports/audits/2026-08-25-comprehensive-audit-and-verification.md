# RELATÓRIO DE AUDITORIA COMPLETA — corelink-server

**Data:** 2026-08-25
**Método:** 7 subagents de auditoria paralelos (auth, billing, cache, infra, worker/storage, perf-latência, perf-throughput) + 5 subagents de verificação independente + re-verificação manual direta (grep/read/execução) de todo ponto fraco, parcial ou refutado.
**Regra adotada:** dúvida → flag. Toda flag com file:line + justificativa + correção proposta.
**Rastreabilidade:** cada achado carrega status pós-verificação: ✅ CONFIRMADO · ⚠️ PARCIAL (escopo corrigido) · ❌ REFUTADO · 🆕 NOVO (descoberto na verificação).

---

# 1. SUMÁRIO EXECUTIVO (leia isto se só tiver 2 minutos)

O CoreLink NÃO está impecável, mas o fundamento é forte. Em termos diretos:

**O que está genuinamente excelente (verificado linha a linha):**
- Zero caminhos de vazamento cross-tenant encontrados em toda a superfície de cache (prefixo HMAC por tenant, fail-closed em toda costura, re-verificação de digest server-side).
- Constant-time compares consistentes em tudo que é sensível.
- Assinatura de webhook Stripe correta nas 4 implementações (constante-time, ±5min, raw-body HMAC).
- Checkout sem tampering possível (preço resolvido server-side; cliente nunca manda valor).
- Cadeia de auditoria BLAKE3 + cabeças Ed25519 assinadas, testada e honesta sobre lacunas residuais.

**Os 4 problemas mais graves (todos verificados diretamente por mim, com evidência bruta):**
1. **O GC nunca executa.** O produto vende "storage governance", mas o coletor de lixo é um no-op silencioso: os crons de produção disparam contra um handler que não existe. Cache = append-only. Tenant estoura cota → 402 permanente.
2. **Uma seed criptográfica Ed25519 REAL está commitada em plaintext no `wrangler.toml`** — entrou no HEAD (`0a5e3349`). Checklist interno mandava usar `wrangler secret put`. Gitleaks não pegou (hex sem keyword = gap do gate). Exige rotação.
3. **O gate de secrets está VERMELHO no main agora**: `validate_secrets_matrix.py` → exit 1, com 10 variáveis em drift. O CLAUDE.md afirma que passa. Claim falso hoje.
4. **Billing pode ressuscitar assinatura cancelada** num race de webhook fora-de-ordem (escopo estreito mas real — detalhado abaixo).

**A tese de performance (80%):** plausível mas estreita. Em operações metadata-bound (>80% de corte real). Agregado geral ~60%. O maior gargalo é único e banal: **2 inserts síncronos no D1 inline por operação de cache** — e a correção do segundo maior (`runQuotaBatch`) já está ESCRITA e MEDIDA no código, só nunca foi conectada.

**Padrão sistêmico que causou quase todos os achados graves:** *proof ≠ production* — código bonito, testado e documentado cujo wiring de produção usa collaborators fracos (audit in-memory, console.log, constantes simuladas, coordenador advisory), e *docs que afirmam controles que não existem* (KV-delete de revoke, crons semanais, gate verde). A recomendação processual nº 1: **todo claim de doc sobre controle/latência exige grep de verificação antes de confiar.**

---

# 2. METODOLOGIA

```
Rodada 1 — Descoberta (7 agents paralelos)
  A1 auth/PAT/crypto      A2 billing/stripe       A3 superfícies de cache
  A4 infra/CI/specs       A5 worker/storage/DO    A6 perf latência hot-path
  A7 perf throughput/build

Rodada 2 — Verificação independente (5 agents, zero herança de confiança)
  V1 C1–C4   V2 C5,C6,H2,H3   V3 H1,H4–H8   V4 P1–P5   V5 amostra M/L (18 claims)

Rodada 3 — Confirmação manual (minhas mãos, comandos baratos)
  Re-execução de gates, git archaeology do seed, leitura direta dos pontos
  PARTIAL/REFUTADO, spot-check dos confirms de maior risco.
```

Contagem final: **31 claims substantivos verificados → 24 confirmados integralmente, 5 confirmados com escopo corrigido, 2 refutados (H3-headline, L18-OpenAPI), 2 descobertas novas (seed, replay-script-morto), 1 terceiro cron morto.**

---

# 3. CRITICALS (launch blockers)

---

## 🔴 CRIT-1 — O GC/eviction NUNCA executa (e agora sabemos que são 3 crons mortos, não 1)

**Status:** ✅ CONFIRMADO (verificado 3×, incluindo leitura direta minha)
**Severidade:** CRITICAL — é a value prop do produto ("storage governance") não-shippada.

**Explicação mastigada:** O `wrangler.toml` de produção agenda crons (linhas 296–306):

```toml
[env.prod.triggers]
crons = [
    # GC daily (02:00 UTC) — posts to /_internal/gc/run
    "0 2 * * *",
    # DSR verify sweep hourly — posts to /_internal/dsr/verify
    "0 * * * *",
    # Replication heartbeat every 5 min — posts to /_internal/replication/heartbeat
    "*/5 * * * *",
]
```

Um cron do Cloudflare Workers entrega um evento `scheduled` para o worker. Para isso funcionar, o worker precisa exportar um handler `scheduled`. O worker principal (`worker/src/index.ts`) exporta **apenas `fetch`** (linha 1752, `export default handler` na 3256). O ÚNICO `async scheduled` do repo inteiro está no **signup-worker** (`apps/signup-worker/src/index.ts:135`) — crate diferente, que não recebe esses crons. Resultado: **os eventos de cron nascem e morrem sem destinatário.**

E piora. Mesmo se o cron chegasse:

1. **A rota alvo não existe.** `/_internal/gc/run` aparece em exatamente 2 lugares no repo: dois comentários no próprio wrangler.toml. Zero rotas registradas em `worker/src` ou nas rotas do container.
2. **Nem há dados para o GC trabalhar.** Nenhuma query `INSERT`/`UPSERT` em `blob_meta` existe em código compilado. Os writers dessa tabela vivem dentro de `crates/corelink-container/src/gc_worker/` — um módulo que **nem é declarado no `lib.rs`** (zero menção a `gc`), logo nem compila para o binário deployado. O outro candidato (`MetaStore::commit_put`) tem **zero call sites** no container/worker.
3. O módulo `corelink-eviction` (motor de fases) e o pipeline multipart também são library-only, inalcançáveis do binário.

**Consequência de negócio:** todo blob armazenado (nativo, Bazel, Turbo, sccache, cargo) vive PARA SEMPRE até o tenant bater o byte cap — e aí recebe **402 permanente**, sem como liberar espaço. Cache sem eviction = armazenamento append-only.

**Bônus da verificação:** o cron de replication heartbeat (`*/5`) também está morto pelo mesmo mecanismo (sem handler `scheduled`). A promoção continua funcionando porque depende do `alarm()` do DO, não do cron — mas o cron em si é ruído.

**Correção:**
1. Adicionar handler `scheduled()` ao worker principal que despache os POSTs internos (um fix resolve os 3 crons).
2. Declarar `mod gc_worker` no `lib.rs` e conectar o RealGcWorker à rota.
3. Fazer `R2CasHandler::write`/`R2KvStore::write` manterem linhas `blob_meta` transacionalmente com o PUT.
4. Alertar quando cron atingir 404.

---

## 🔴 CRIT-2 — Seed criptográfica Ed25519 REAL em plaintext no git 🆕

**Status:** ✅ CONFIRMADO (descoberta nova da rodada de verificação — TODOS os 7 agents de auditoria passaram por ela)
**Severidade:** CRITICAL — material criptográfico comprometido em histórico git.

**Evidência bruta** (`wrangler.toml`, bloco `[env.prod]`, tracked no git):
```toml
vars = { ..., AUDIT_CHAIN_SIGNING_SEED_HEX = "c4bdb99f8170a53ec56cfd7d5fa9579465d07d5fe0e49af3c8980b41191d7141", ... }
```

**Git archaeology:** `git log -S "<valor>" --all` → o valor entrou em **`0a5e3349`, que é o HEAD atual** ("feat(remediation): enable infrastructure for 6 gated enterprise/compliance features"). Ou seja: vazou no commit mais recente, está fresco no histórico.

**Por que é grave:** essa seed assina as cabeças da cadeia de auditoria (CF-6, tamper-evidence vs um D1-writer hostil). Quem tem a seed pode forjar cabeças de cadeia válidas. O próprio checklist de secrets (row 164, `docs/internal/secrets-checklist.md:223`) prescreve: **UNSET em prod** ou `openssl rand -hex 32` via **`wrangler secret put`** — nunca como var plaintext.

**Por que os gates não pegaram:** gitleaks não dispara em hex de alta entropia sem keyword/assignment context reconhecível — é um gap do gate que merece regra custom (`AUDIT_CHAIN.*SEED.*[0-9a-f]{64}`).

**Correção (ordem obrigatória):**
1. `wrangler secret put AUDIT_CHAIN_SIGNING_SEED_HEX --env prod` com valor NOVO (rotação — a antiga considera-se queimada).
2. Remover a var do wrangler.toml.
3. Decidir política de history purge (rewrite é disruptivo; mínimo: documentar a exposição + rotação torna o valor morto).
4. Adicionar regra gitleaks para seeds hex longas em vars.

---

## 🔴 CRIT-3 — Gate de secrets VERMELHO no main AGORA

**Status:** ✅ CONFIRMADO (executei o script pessoalmente)
**Severidade:** CRITICAL processual — um gate que mente é pior que gate ausente.

Saída real:
```
$ python3 scripts/validate_secrets_matrix.py
validate_secrets_matrix: matrix=190 code=164 in_both=154 matrix_only=36 code_only=10
ERROR code_only (real drift; add to matrix or allowlist):
  - AUDIT_ARCHIVE_BATCH_LIMIT      - CORELINK_ORIGIN_TIMING_DETAIL
  - AUDIT_DRAIN_BATCH_LIMIT        - EDGE_ASYNC_METER
  - AUDIT_DRAIN_LEASE_ENABLED      - EDGE_DO_METER
  - CORELINK_ERASE_AUTH_KEY_PREVIOUS - EDGE_PUBLIC_READ
                                   - OCI_PUBLIC_DEDUP_ENABLED
                                   - OCI_UPSTREAM_ON_MISS
EXIT: 1
```

O CLAUDE.md afirma que este gate passa com `code_only=0`. Afirmação falsa hoje. As 10 vars parecem flags de comportamento (não secrets de verdade), então o fix provavelmente é allowlist documentada — mas o estado "gate vermelho + doc dizendo verde" normaliza ignorar gates.

**Correção:** adicionar as 10 rows ao checklist OU movê-las para a allowlist de env-não-secret, com justificativa. Atualizar CLAUDE.md para descrever o estado real.

---

## 🔴 CRIT-4 — Repo NÃO limpo: 57 arquivos staged + 15 untracked, incluindo módulo GC inteiro só no seu disco

**Status:** ✅ CONFIRMADO (contagem pessoal via `git status --porcelain`)
**Severidade:** CRITICAL processual — violação direta do mandato "zero debt, no loose ends".

```
55 staged  (51 M + 4 A)   ← nunca commitados
 2 modified-unstaged
15 untracked, incluindo:
  crates/corelink-container/src/gc_worker/     ← 13 arquivos Rust REAIS
  apps/admin-ui pricing/*, llms.txt            ← feature inteira
  docs/design/ 2026-08-13, 2026-08-17          ← planos
  reports/audits/2026-08-07-external-hard-audit.md
```

O `gc_worker/` é o pior item: são os stores D1 do GC (refcount, purge, candidates, physical-delete) que o CRIT-1 precisa — existem **apenas neste disco**. Zero backup remoto, zero review. Perda de disco = perda do módulo.

**Correção:** split em PRs revisáveis e land (gc_worker primeiro — ele desbloqueia o CRIT-1); ou discard explícito e documentado do que não vale.

---

## 🔴 CRIT-5 — Materializer de billing pode ressuscitar assinatura cancelada (race fora-de-ordem)

**Status:** ⚠️ PARCIAL — headline original exagerada; race residual REAL e confirmado.
**Severidade:** HIGH→ mantida CRITICAL-for-launch por ser dinheiro.

**O que é verdade:** existe guard de payload no materializer:
```rust
// handler.rs:519-521
if !subscription_status_grants_access(status) { return Ok(()); }
// handler.rs:141-143 — só "active"|"trialing" passam
```
Eventos com `status=canceled` NÃO ressuscitam. Test de regressão pinha isso (`handler.rs:1190`).

**O buraco real:** o guard lê SOMENTE o payload do evento. O upsert é cego:
```rust
// d1.rs:173
"...ON CONFLICT(tenant_id) DO UPDATE SET tier=excluded.tier, subscription_state='active', ..."
```
Sem join/condição em `tenant_billing.status`. Um `customer.subscription.updated(status='active')` STALE — redelivery atrasado, reordenamento do Stripe — que chegue DEPOIS do `deleted` processado passa pelo guard de payload e regrava `'active'`.

**A prova de que o time sabe disso:** o twin writer (signup-worker) TEM o guard DB-level exatamente para esse cenário:
```ts
// stripe.ts:860-864
WHERE tenant_id IN (
  SELECT tenant_id FROM tenant_billing
  WHERE stripe_subscription_id = ?1 AND status != 'canceled')
// comentário :832-835 nomeia literalmente "must NOT resurrect a canceled subscription"
```
Dois writers da mesma entidade com posturas diferentes = a classe de bug exata que o worker já patchou.

**Correção:** espelhar o guard SQL-level do worker no materializer (condição no upsert ou check pré-grant por subscription id).

---

## 🔴 CRIT-6 — Failover congela TODOS os writes com 3 requests ruins (sem sample floor)

**Status:** ✅ CONFIRMADO (li o código diretamente)
**Severidade:** CRITICAL operacional — bug de página às 3am.

**Explicação mastigada:** o probe de saúde de região avalia uma janela de 5s com 3 sinais: taxa de 5xx >1%, p99 >300ms, ≥3 falhas consecutivas. A decisão:
```rust
// health.rs:241-245
let health = if triggers.len() >= 3 { RegionHealth::Degraded } else { RegionHealth::Healthy };
```
Única proteção: `if total == 0 { healthy }` (`failover.rs:199`). **Não existe mínimo de amostras.** Aritmética do desastre: 3 requests consecutivos lentos+500 dentro de 5s → rate=100%, p99>300, consecutive≥3 → Degraded → `failover_guard` responde **503 readonly para TODO write** que passar pela layer (`failover.rs:347-367`). O teste próprio do repo prova trip com 5 amostras (`failover.rs:448-458`).

Nuances honestas: o estado é **por instância de container** (não switch global centralizado) — mas sob problema regional real, todas as instâncias tripam juntas = fleet-wide de facto. Inerte em nrt/syd/dev (`routes.rs:976-978`). E o sinal é auto-referencial (o probe conta as próprias respostas do container) → brownout amplifier: backend meio doente alimenta o próprio congelamento.

**Correção:** exigir `total >= N_MIN` (ex.: 50) antes de qualquer sinal contar; histerese/grace de recuperação; separar sinal de infra (classes de erro R2/D1) de 5xx aplicativo.

---

# 4. HIGH

---

## 🟠 HIGH-1 — Revoke de PAT NÃO invalida KV edge, embora 4 documentos afirmem que sim

**Status:** ✅ CONFIRMADO (grep meu: zero hits)
**Arquivos:** `worker/src/lib/runner_mint.ts:649-667` (escreve só D1); `pat_verify_cache.ts:100` (`KV_PAT_ROW_TTL_S = 60`)

Grep de `delete` + `patrow` em todo `worker/src`: **único hit é um COMENTÁRIO mentiroso** (`tenant_suspend_gate.ts:46`). Os sites que afirmam o controle inexistente:
- `docs/knowledge/auth/pat-moat.md:210`
- `specs/.../ADR-0070-...md:83` (+ conceito OKF espelho)
- comentário do próprio código `pat_verify_cache.ts:97`

**Consequência:** PAT revocado via worker mantém acesso na edge por até 60s (TTL KV) + eventual-consistency do KV. Dentro do SLO ≤60s p99 do ADR-0030, MAS o mecanismo "imediato" que compensa o fail-open do suspend-gate (ADR-0070) **não existe**. Em resposta a abuso, doc-vs-realidade divergente é inaceitável.

**Correção:** no revoke, resolver `token_id` pelo `pat_id` na mesma transação e KV-delete `patrow:<token_id>` (write-behind via `waitUntil`); OU corrigir os 4 sites de doc enquanto o delete não shipa.

---

## 🟠 HIGH-2 — Webhook preso para sempre após falha transitória; DLQ in-memory; replay tool morta

**Status:** ✅ CONFIRMADO E PIOR QUE O CLAIM ORIGINAL
**Arquivos:** `crates/corelink-stripe-real/src/webhook_dispatch.rs:606` (dedup commita ANTES de materializar), `:714-723` (arm de quarentena), `main.rs:883`

Sequência do desastre: dedup row commita (passo 5) → materialize falha transitória (D1 flake) → 500 + quarentena → Stripe re-tenta → bate `AlreadyProcessed` → **200, handler pulado para sempre**. Cliente pagando cujo `checkout.session.completed` caiu num flake fica sem acesso, invisivelmente.

Piora ×2 da verificação:
1. **A DLQ de produção é IN-MEMORY** (`main.rs:883 InMemoryWebhookDlqStore`) — o comentário ao lado admite que a versão D1-durável é follow-up. Redeploy evapora a quarentena.
2. Existe um script `scripts/forensics/replay-stripe-webhook.sh`, mas **ele não funciona**: busca o evento na API Stripe, consulta a tabela *processed* (não a DLQ) e re-posta com **assinatura fabricada** `v1=replay-${EVENT_ID}` — morre no HMAC verify (`webhook_dispatch.rs:560-576`). Não re-materializa nada.

**Correção:** DLQ durável em D1 + bin/route de replay autenticado (bypass token de dedup) + alerta de idade da DLQ. Pré-cartão-real.

---

## 🟠 HIGH-3 — ~~Sem gestão customer-facing de subscription~~ → ❌ HEADLINE REFUTADA

**Status:** ❌ REFUTADO no headline; letra parcialmente verdadeira.

A verificação encontrou a rota montada e wired em produção:
```rust
// routes/customer.rs:211
.route("/v1/customer/billing/portal", post(handle_billing_portal))
// :711 state.billing.portal_url(req) → StripePortalSessions::create (customer_d1.rs:203)
// test próprio: :1740
```
Clientes PODEM trocar cartão/plano/cancelar via Stripe-hosted portal. O que permanece verdade: o trait `PortalIssuer` de `stripe-real/portal.rs` não tem callers (existe outra impl em uso), e o checkout self-serve de novo tier bloqueia com 409 `AlreadyActive` para sub ativa no mesmo axis (`tier_select.rs:882-884`) — axis-scoped: tenant com cache ativa ainda compra runner; primeira compra nunca bloqueada. Mudança de plano do live sub exige a rota portal, que existe. **Item sai da lista de blockers.** Residual LOW: consolidar/documented as duas vias de portal.

---

## 🟠 HIGH-4 — Action Cache: GET-compare-PUT sem PUT condicional (race de envenenamento)

**Status:** ✅ CONFIRMADO
**Arquivo:** `crates/corelink-container/src/storage/r2_s3.rs:2295-2329` (`R2AcHandler::update`); `r2_s3.rs:143-160` (`put` incondicional; zero hits de `IfNoneMatch` no repo)

Dois writers concorrentes com bodies divergentes para o mesmo action digest: ambos GET→None, ambos PUT → last-writer-wins **por cima de uma ActionResult provada**. CAS é imune (content-addressed); AC não. Janela de supply-chain intra-tenant. Erro ambíguo de GET faila fechado (bom) — o gap é a janela GET→PUT.

Nota relacionada (MEDIUM): `ac_create_only` é enforceado SÓ na superfície nativa (`routes/ac.rs:648,663`); Bazel REST + alias stock-Bazel checam apenas `ac_key_allowed` — bypass de policy via superfície alternativa.

**Correção:** `put_if_none_match` (R2 suporta); no 412, re-GET + lógica divergente/idempotente atômica. Hoist do `ac_create_only` para helper compartilhado nas 3 superfícies AC.

---

## 🟠 HIGH-5 — Auto-promotion de replicação acredita em timestamp client-supplied

**Status:** ✅ CONFIRMADO
**Arquivo:** `worker/src/replication_coordinator_do.ts:581`; freshness gate `:180-184`; alarm loop `:467-502`

```ts
const ts_ms = Number(b.ts_ms ?? now_ms);   // relógio do CLIENTE
```
Lag bundle também é client-supplied (`:583-587`). Reporter com clock skew (ou dono da shared key comprometida) marca primary stale → alarm promove réplica em ≤30s, congelando writes do primary saudável. Auth do endpoint = mesma `CORELINK_INTERNAL_AUTH_KEY` de tudo que é `/_internal/*` — o próprio arquivo comenta: "A leak of this single shared secret enables any-tenant admin-PAT minting".

**Correção:** DO carimba hora de recebimento server-side; ignora `ts_ms` do cliente p/ freshness; chave dedicada para `/_repl/*`.

---

## 🟠 HIGH-6 — Eviction LRU do rate-limit resetа bucket drenado pra FULL (bypass sustentado)

**Status:** ✅ CONFIRMADO — behavior real, RISCO ACEITO DOCUMENTADO no código
**Arquivo:** `crates/corelink-ratelimit/src/limiter.rs:79-91, 358-373, 421-423, 466-471`

Bucket drenado (você gastou seu burst) → eviction LRU o remove → próximo hit re-materializa via `fresh_bucket()` = `new_full`. O ataque: drene o próprio bucket, inunde keys distintas (cada uma barata, cada uma nasce full) até a eviction amostrar e resetar sua key drenada → burst de novo. Multiplicador sustentado sobre o burst intendido. Vale para o mapa per-tenant E para o velocity gate `_oci` pré-auth (pool virado pro atacante). O código argumenta (:79-85) que hot keys raramente são amostradas e cap=100k é generoso — defesa quantitativa, não eliminação.

**Correção:** persistir deny-state separado do bucket (tombstone de keys drenadas com TTL; re-materializar em 0 tokens se último estado era denied).

---

## 🟠 HIGH-7 — Failover de produção audita em Mutex\<Vec\> in-memory ilimitado

**Status:** ✅ CONFIRMADO (nuance: baixa taxa de eventos)
**Arquivos:** `routes/failover.rs:286` (wiring prod via `routes.rs:979-983`); `corelink-replica-worker/src/audit.rs:134` (`Mutex<Vec<ReplicaAuditRecord>>`), emit = push bare `:167-170`

Crescimento sem teto ao longo da vida do processo + os eventos de region-failover — os momentos MAIS dignos de auditoria — nunca tocam D1/R2 chain. Mesma família do F-022 (OOM class) morto em outros planos.

**Correção:** ring buffer limitado ou sink durável via `audit_outbox`.

---

## 🟠 HIGH-8 — Cadência de heavy-gates documentada é meio ficção (mas o park foi autorizado)

**Status:** ✅ CONFIRMADO (grep direto dos YAMLs) — com contexto importante encontrado na verificação

| Workflow | Doc diz | Realidade |
|---|---|---|
| coverage.yml | weekly Tue | cron COMENTADO (`# - cron:` :41), dispatch-only |
| cas_foundation.yml | weekly Wed | cron COMENTADO (:84-85), dispatch-only |
| reproducible-build.yml | nightly | cron COMENTADO (:41-45), dispatch-only |
| ffi-matrix-ci.yml | weekly Thu | VIVO ✓ |
| s10-ship-gate.yml | weekly Fri | VIVO ✓ |
| codeql.yml | nightly | VIVO ✓ |

Descoberta da verificação: coverage.yml:32 traz `"PARKED 2026-08-10 (cost hygiene, owner-authorized)"` — ou seja, **o park foi decisão tua autorizada**; a podre real é o **CLAUDE.md desatualizado** afirmando cadência que não existe. Gates de assurance agendado para cobertura/cas-foundation/reproducible-build = zero hoje; dependem de dispatch manual.

**Correção:** atualizar CLAUDE.md para o estado real + decidir explicitamente: restaurar schedules ou formalizar "dispatch-before-release" como política escrita.

---

# 5. MEDIUM

---

### AUTENTICACÃO / AUTH PLANE

**MED-1 — Chave PAT principal sem validação hex que as siblings têm** ✅
`worker/src/index.ts:1084-1106` valida só `length >= 64`. Siblings (:1215-1229) validam even-length + hex + ≥64 com log CRITICAL + 503. Chave principal malformada atravessa o guard, `hexDecode()` (:1598) retorna null por request → `verifyPatHmac` false silencioso → **todos os clientes legítimos tomam 401** sem nenhum sinal. Fix: mesmo `isValidHexKey` na principal, fail-closed 503.

**MED-2 — JWKS kid-miss refresh sem cooldown (DoS amplification)** ✅
`crates/corelink-clerk/src/adapter.rs:306-337, 470-509`. JWT com `kid` random desconhecido → fetch HTTPS remoto ao Clerk POR REQUEST. Sem cooldown/min-interval/backoff (grep: zero hits). Atacante não-autenticado queima latência do teu hot path + rate-limit do Clerk + egress. Fix: min-interval (≥30s) ou negative-cache de kids missados.

**MED-3 — Timing pad dobrado: middleware E verifier padam (invariante inalcançável)** ✅
`middleware/auth.rs:145-150,557-565` + trait doc que EXIGE o verifier padar. Impl conforme = cold path queima Argon2id DUPLA (~2× warm). O envelope equal-latency INV-AUTH-CONSTANT-TIME-COLD-PAD só se realiza se o verifier DESOBEDECE o doc. Hoje mascarado (planes de prod não usam esse Tower wiring), mas contrato load-bearing autocontraditório. Fix: UM dono do pad + timing test.

**MED-4 — Scope/find_only/runner-marker propagam com até 60s de staleness sem ADR** ✅
`pat_verify_cache.ts:100,111-125` + `index.ts:1318-1337`. L2 KV cacheia a ROW inteira; docs só raciocinam sobre revoke. Widening concedido e revertido ainda anda 60s. Ratificável — mas precisa do trade-off explícito estilo ADR-0070 (ou KV-delete simétrico no change).

### BILLING

**MED-5 — Audit trail do materializer é in-memory ao lado de writer D1 durável** ✅
`container/main.rs:835` — `InMemoryBillingAuditEmitter` + `D1HttpBillingWriter`. Estado durável, evidência volátil. Todo forensics `corelink.billing.*.materialized.v1` some no restart. Fix: emitter archive-producer over HTTP espelhando o writer.

**MED-6 — Status ausente: fail-OPEN no container, fail-CLOSED no worker** ✅
`handler.rs:304-310`: `.unwrap_or(if canceled {"canceled"} else {"active"})` → payload assinado mas sem `status` vira grant. Worker (`stripe.ts:373-375`): absent → false → sem entitlement. Violates o próprio doc do handler (:127-128). Mitigante: Stripe sempre manda status. Fix: absent → skip write + observability row.

**MED-7 — Refund/dispute e entitlement** ⚠️ (corrigido na verificação)
Container: `charge.refunded` echo-only ✅, **MAS dispute TEM handler** (`webhook_dispatch.rs:660-662` `ChargeDisputeCreated => materializer.on_charge_dispute_created`). Signup-worker: NEM refunded NEM dispute (default ack `:1965-1968`). Refund ainda não revoga nada em nenhum writer. Chargeback abuso = serviço grátis até cancel manual. Fix: refund/dispute.closed(lost) → revoga/marca disputed no worker.

**MED-8 — Sem reconciliação de ENTITLEMENT (só usage)** ✅
`billing-reconcile/run.rs` compara 3 camadas de USAGE; nada diffa Stripe-subs↔D1 rows. Drift classe CRIT-5 invisível. Layer 3 presume usage_records que flat-SKU nunca cria (perna vácuo). Fix: job diário de diff + promote auto-pause pós-launch.

### FAILOVER / REPLICAÇÃO

**MED-9 — TOCTOU no route_write; zero fencing token** ✅ (teórico: skeleton in-memory)
`coordinator.rs:277-312`: role check sob lock → `drop(guard)` :293 → health read depois. Promote concorrente entre checks = write em região demovida. Coordenador advisory; admission real é self-assessment por container. Fix: fencing epoch stamped nos writes.

**MED-10 — Failback sem verificação de catch-up** ✅
Cooldown 24h expira → região volta a Primary SEM gate de lag (comentário admite). Só o AUDIT outbox tem drain gate. Janela de divergência silenciosa. Fix: exigir LagBundle dentro do SLO + reconciliation sweep `created_at > failover_ts`.

**MED-11 — Trigger "sustained 5s" não implementado; estado Down inalcançável** ✅
Doc `router.rs:5-17` promete janela sustentada; decision core avalia snapshot instantâneo (`health.rs:222-256`); `Down` nunca produzido por `evaluate` (:79-89). Dashboard/alerta consomem estado morto. Fix: máquina de estados com persistência de trigger ou doc corrigido.

**MED-12 — audit-drain materializa backlog INTEIRO em Vec + retorna ok:true com falhas** ✅
`audit_drain.rs:660-689` + `:971-981`. Post-outage de 10⁶ rows → memory blow; cron caller vê sucesso com `partitions_failed>0` e sealing silenciosamente stallado. Ironia: o verifier sibling se orgulha de streaming 1MB/batch. Fix: LIMIT batches + ok:false em falha.

**MED-13 — `sha256_hex` é FNV-1a-64 com nome de SHA-256** ✅ (li o corpo pessoalmente)
`router.rs:232-240`: FNV offset `0xcbf29ce484222325`, mult `0x00000100000001b3`, `{hash:064x}` zero-padded pra fingir width. Comentário admite ("Simulate"). Consumidor assumindo SHA-256 mismatcha silencioso; collision resistance era 32-bit. Fix: renomear ou hashear de verdade.

**MED-14 — console.log como audit de promoção de região em PROD** ✅
`replication_coordinator_do.ts:446-448`. Quem flipou qual região = stdout only, fora da chain. Fix: outbox/D1.

### RATE-LIMIT / CONFIG

**MED-15 — Tier de rate-limit congelado por lifetime do container** ✅ + bônus
`ratelimit_layer.rs:215-218,349-389`: `planned HashSet` nunca expira; upgrade de plano invisível até restart. **Bônus achado:** tier NÃO-resolvível TAMBÉM marca planned (:364-371) — tenant fica no default mesmo após D1 ser consertado. Fix: TTL 5min nos entries.

**MED-16 — Config-DO history/payload maps crescem pra sempre** ✅
`config-do/store.rs:186-193,276-277`: retention 90d gateia rollback eligibility, mas BTreeMaps nunca podam. Classe unbounded-collection. Fix: evict > retention on update.

### SUPERFÍCIES DE CACHE

**MED-17 — REAPI: QueryWriteStatus UNIMPLEMENTED + split sha256/blake3 derrota dedup cross-surface** ✅
`bytestream.rs:439-446` (retry resumível quebrado p/ Bazel real), `capabilities.rs:99` (advertising BLAKE3-only; stock Bazel default SHA256 → hard reject), HTTP-Bazel storeia em `bazel/sha256/` enquanto native/sccache em blake3 → mesmos bytes, duas cópias, **o moat network-effect estruturalmente derrotado**. Fix: implementar QWS + decidir: keyspace SHA256 nativo OU posicionar superfícies como caches separados documentadamente.

**MED-18 — Sentinel list divergida no turbo_v8** ✅
`turbo_v8.rs:504,678,885`: lista local `_anonymous/_unknown/_system/_pending` SEM `_oci`/`_public`, divergindo de `auth_tenant::is_reserved_sentinel` (que bazel_v2 usa corretamente). Hoje sombreado pela ordem de extractors; latente. Copy-paste drift já aconteceu. Fix: usar o helper compartilhado.

### INFRA

**MED-19 — Colisões de migration + strays** ✅
`migrations/d1/`: DOIS `0044_*`. Raiz `migrations/`: strays fora do `migrations_dir` do wrangler incluindo DOIS `013_*`, esquema alien (`002_auth_tables`), `N4__`. Fix: arquivar strays; renumerar colisões se realmente aplicadas.

**MED-20 — Crons redundantes restantes violando a própria regra** ✅
bazel/buck2-starter weekly ("sustained green gate" = clock re-verificando código idêntico), proptest-density Thu + PR lane, gc-ship-gate Sat ("belt-and-braces re-execution for a SEALED sprint"), mutation-nightly triplo-redundante (overlaps mutation-pr + nightly mutants), sbom duplo. Fix: dispatch-only ou morte.

**MED-21 — 21 workflows sem concurrency group** ✅
Incluindo nightly.yml (o pior: TLC ×30 specs, cargo test --release workspace, fuzz 9×1h, mutants 240min — tudo commit-driven, violação pura da regra de cron, SEM cancel-in-progress sobre 5 runners compartilhados). Fix: concurrency groups copiados dos 99 corretos.

**MED-22 — pre-merge-gate-check.sh com presence-list incompleto** ✅
`REQUIRED_PRESENT = ["dco","gitleaks"]` — changelog-validate.yml (bare pull_request, carrega o gate feat:/fix:-CHANGELOG) ausente da lista = run ausente passa silencioso. One-line fix.

**MED-23 — Root rot** ✅
TODO.md = plano MVP de abril (Postgres/sqlx, semanas 1-12, tudo unchecked, zero match com a stack atual). `_archive/wi-s11-002-partial/` crate morto trackado. Docs-raiz suspeitos de staleness.

### OKF WIKI (anti-drift leakando)

**MED-24 — Conceito flagship descreve o OPPOSTO do wiring atual** ✅
`docs/knowledge/planes/replication-failover.md:4,:131` diz "designed-not-wired… NO live request path calls them". Código: `routes/failover.rs:63-64` importa `InMemoryFailoverRouter`; `routes.rs:979-983` monta a layer no data-plane live com writes 503 fail-closed. Como CLAUDE.md manda agentes confiarem no wiki ANTES de tocar subsistema, alguém vai trabalhar "com a arquitetura" direto num mapa velho. Segundo conceito (`operations.md`) com anchor drifted (`limiter.rs:445` → agora :465). Terceiro checado OK. Fix: rodar okf-reconcile nos 2.

---

# 6. LOW (consolidado — cada um verificado com quote)

**Auth/PAT:**
- L-A1 `argon.rs:133-137` iter-and-discard dead block ✅
- L-A2 dummy PHC duplicado + fallback que mata o pad silenciosamente em regime degradado ✅
- L-A3 `mint.rs:223` type assertion morta ✅
- L-A4 scopes.rs doc arithmetic errada ("13 + 51 reserved"; real: 12 bits, 52 reserved) ✅ — no arquivo que DEFINE o modelo de permissão
- L-A5 orchestrator empty if "defensive check" sem efeito ✅
- L-A6 chunking ownership: 3 arquivos, 3 histórias contraditórias ✅
- L-A7 SessionCacheKey.from_hash sem validação ✅
- L-A8 Worker native plane sem cold-pad (justificável via HMAC-first ordering — mas invariant não-documentado) ✅
- L-A9 base64url non-canonical trailing bits: TS aceita, Rust rejeita → validade plane-dependente ✅
- L-A10 rotation adapters = simulação (sem key material; retire() aceita overlap instantâneo; WI-S13-006 dependency ABERTA que o story de 24h-overlap assume) ✅
- L-A11 JWT cold-pad exemption com justificativa tecnicamente falsa ("~5ms regardless") ✅

**Billing:** L-B1 idem-key colisão em params distintos 24h (502 opaco) ✅ · L-B2 4 implementações de signature divergentes ✅ · L-B3 cancel shape divergente entre writers ✅ · L-B4 analytics at-least-once documentado ✅ · L-B5 scaffold docs claiming todo!() em código implementado ✅

**Cache:** L-C1 legacy OciBlobBridge fraco dead-but-compiling ✅ · L-C2 gc_worker imports 15× repetidos ✅ · L-C3 ByteStream Read bufferiza blob inteiro antes de slicar range ✅ · L-C4 inventário inertão gigante (reapi tonic 7.7k linhas, ac Merkle, eviction engine, cas chunker/fastcdc, r2-multipart — mantidos com proptest gates, zero efeito prod; duas stacks CAS paralelas) ✅

**Infra:** L-I1 .gitignore dupes (.env.prod ×2, target-isolated ×2) ✅ · L-I2 .semgrepignore sem `.open-next/`/`.wrangler/` (⚠️ parcial: `.next/dist/build` ESTÃO lá) · L-I3 specs com dois homes de runbook ✅ · L-I4 wrangler PLACEHOLDER database_ids top-level/staging ✅ · L-I5 custo de probes (72 canary runs/mo hosted + 36 Mac e2e/day) ✅ · L-I6 validate_specs output PT-BR ✅

**Worker/storage:** L-W1 ratelimiter hardcoded `"test"` request-id + duration_us=0 (SLO morto quando sink real ligar) ✅ · L-W2 `overhead_ms: 5 // simulated` fabricado consumido por SLO ✅ · L-W3 write_mode probe ts=0 ✅ · L-W4 429 body atribuição errada no OCI path ✅ · L-W5 proptest density: container = 2 proptests/540 tests (⚠️ parcial: é o MÁXIMO do repo — cliff absoluto thin, relativo não; allowlist esconde rigor thin, não acomoda riqueza) · L-W6 ~~sem OpenAPI~~ ❌ REFUTADO: `openapi/corelink-v1.json` existe — OpenAPI 3.1.0, 44 paths, workflow openapi-validate + sync script (parseei o JSON pessoalmente) · L-W7 SDK narrow (CAS+AC only) ✅

**CLAUDE.md paths** ⚠️ parcial: diz "(routes/bazel_v2.rs)" bare suffix sem nomear crate — ambíguo, não misattributed; real: `crates/corelink-container/src/routes/`.

---

# 7. PERFORMANCE

---

## Ranking final de wins (2 agents convergiram + verificação)

### PERF-1 — Audit sink: 2 inserts síncronos D1 INLINE por operação ✅
**Estimativa:** ~100–400ms/op (region-dependent); batch probes: minutos.
**Onde:** `storage/d1_audit_sink.rs:157-164` (`block_in_place + block_on(d1.query)` — HTTPS ao primary D1 ENAM); emitters `r2_s3.rs:1044,1139,1298,1432,2087,2145`; SQL com correlated subquery em tenant (:133-136); handlers mapeiam erro inline → fail-closed 503 (`bazel_v2.rs:514` etc.) provando await no request path.
**Fluxo hoje (CAS GET):** audit-D1 → R2 GET → rehash total → audit-D1. Serial. 4 hops de rede antes dos bytes.
**Fix (com caveat honesto da verificação):** o blocking é forçado pela trait síncrona `fn emit`; e fire-and-forget ENFRAQUECE deliberadamente o invariante audit-before-mutation. Direções válidas: batch multi-row INSERT (1 RTT p/ 2 eventos), flusher background com sync-flush APENAS nos error paths, ou async trait end-to-end. É tradeoff de design, não free win.

### PERF-2 — FindMissingBlobs totalmente serial ✅ (amplificação UNDERSTATED)
**Estimativa:** 500 digests: 30–90s → <2s. Cap 4096 ⇒ pior caso ~7–20min HOJE.
**Onde:** `find_missing.rs:147-167` `for digest in digests { self.cas.exists(req) }`; cada exists = 1 audit INSERT + 1 R2 HEAD. O padrão fanout com semaphore BATCH_READ_FANOUT=16 EXISTE no mesmo crate (`cas.rs:134,1253-1273`) — unused aqui.
**Fix:** fanout na adapter layer (bridge é sync) + matar audit per-probe (1 evento batched).

### PERF-3 — `runQuotaBatch` escrito, medido, e NUNCA wired ✅ (claim mais forte do audit)
**Estimativa:** ~150–300ms p50 POR REQUEST autenticado — medido em prod, não estimado.
**Onde:** def `quota.ts:697` (zero call sites, nem testes); chain serial viva `index.ts:2835→2852→2877→2937`.
**Evidência in-tree:** `quota.ts:656-662` documenta `qmeter` 152/158/163ms + `qstor` 120/126/130ms = fase `wdb` 277/284/302ms, declara independência das statements, e a função implementa o collapse via `db.batch()`. Posture parity fail-open documentada (:664-690).
**Fix:** UMA edit no call site. Maior ROI/hora do audit inteiro.

### PERF-4 — Cold start eager init ⚠️ magnitude CORRIGIDA
Estrutura confirmada (clients construídos antes do listener bind; `STARTUP_TIMEOUT_MS=90_000`; DO comment :119-125). MAS: commit `ead0f37a` (28/May) removeu o probe IMDS que custava os 60-90s — construção hoje é sync-only ~ms. Win real encolheu de tens-of-seconds para ms-scale. O comment do DO agora mente sobre o presente. Lazy-init = polish, não win. (Este ponto SAI do top-5 efetivo.)

### PERF-5 — Bufferização/cópia/rehash no data plane ✅ com caveats de integridade
- `r2_s3.rs:183-189` `.collect().into_bytes().to_vec()` = dupla alocação por GET ✅
- `cas.rs:922` `body.to_vec()` duplicata de axum Bytes ✅ (turbo_v8.rs:156 auto-confessa: "transiently DOUBLES the body")
- `r2_s3.rs:1121` rehash blake3 TOTAL por HIT — ⚠️ É o INV-CAS-INTEGRITY deliberado (bitrot/tampering). Remover = waiver do owner, não patch.
- `r2_s3.rs:1382` HEAD-before-PUT — ⚠️ previne double-charge contábil (rt-nuclear #13). Skip exige sinal alternativo pro accountant.
Fix limpo: `Bytes` end-to-end (Vec↔Bytes `.into()` free). Fix com dono: sampled rehash (1/N).

### PERF-6..9 (throughput agent, confirmados por consistência)
- Sync traits + nested `block_in_place` ×3/read ×3/write (`r2_s3.rs:1069-1093`) — comentário próprio: "hangs forever (observed: 60s curl timeout in prod)". Async traits (padrão RPITIT já usado em `hash/store.rs:35`) eliminam a ponte. Habilita 1-2.
- `[profile.release] lto="fat", codegen-units=1` workspace inteiro (650 deps, smithy/tonic/reqwest) — fine pro binário ship, desperdiço em lanes CI release. Falta `[profile.dev.package."*"] opt-level=2` (tests rodam blake3/argon2 a opt 0).
- 81 manifests puxam proptest; criterion default; tokio `full` everywhere. Trim features + feature-gate benches.
- GC mark OFFSET-scan quadrático (`gc_worker/d1_reachable_set_source.rs:150-156`) → keyset pagination.
- OCI layers buffered wholly in RAM (`oci.rs:170-172,476`) — caps tornam DoS-safe; stream pra multipart parts cortaria peak heap.

## Veredito "80%"
Plausível mas estreito. Metadata-bound (small PUTs, FindMissingBlobs, batch): **>80% real** — overhead atual = 5–8 RTTs HTTPS serializados/op. Far-region SAM: 600ms–1.2s → 350–650ms (**40–70%**). Agregado geral: **~60%**. Ordem: PERF-1 (off critical path) → PERF-3 (uma linha!) → PERF-2 (fanout) → Bytes end-to-end.

---

# 8. O QUE ESTÁ VERIFICADAMENTE BOM (pra equilíbrio — nada disso precisa tocar)

**Isolamento tenant:** HMAC prefix derivation; fail-CLOSED em prefix não-derivável (`r2_s3.rs:915`, `r2_kv.rs:122`); instance==caller double-check; uniform-404; HEAD-only findMissingBlobs; reserved sentinels; re-verify digest no read. **Zero leakage paths encontrados.**
**Crypto hygiene:** subtle::ConstantTimeEq em TODA comparação sensível (token_id, issuer, audience, env, smuggled-tenant, internal-auth padded); PatPlaintext zeroize-on-drop, Debug redacted.
**Stripe signatures:** constant-time, ±5min simétrico, raw-body-first, multi-v1 rotation (2 de 4 impls).
**Checkout:** server-priced (cliente nunca manda amount), lock 60s + UNIQUE index + idem key determinística → double-charge fechado; unpaid não ativa; redirects allowlisted.
**Worker lifecycle webhooks:** terminal-cancel guards, past_due/unpaid/paused revoga, dunning-reactivation guarded, process-then-claim exactly-once analytics.
**PAT caches:** L1/L2/L3 exatamente como ADR-0070 documenta, inclusive waitUntil write-behind; NativePatGate single-flight.
**Audit chain:** BLAKE3 math + genesis/link/tamper vectors sound; CF-6 Ed25519 heads fail-CLOSED unsigned-resume; drain crash-safety via sealed-tail-authoritative.
**Token bucket math:** monotonic clamp, NaN defense, deny-no-decrement.
**Edge:** tenant-header strip confirmado; boot watchdogs must-arm; meta SQL bound-only; analytics cardinality-disciplined; residency guard presente.
**Zero TODO/FIXME/unimplemented!/unwrap em request paths nos crates scoped** (verificado por grep; únicos "todo!" são headers scaffold mentirosos já flagados).
**Infra boa:** dependabot policy com rationale excelente; patches pinned justificados; Dockerfile digest-pinned non-root; gitleaks posture correta (exceto o gap do seed); pre-merge script logic sólida.

---

# 9. CORREÇÕES DO PROCESSO (o que mudou entre rodadas — lição de epistemologia)

| Claim rodada 1 | Rodada 2/3 |
|---|---|
| Portal inexistente (H3) | ❌ Rota `/v1/customer/billing/portal` wired em prod |
| Sem OpenAPI (L18) | ❌ Spec 3.1.0/44 paths existe |
| Cold start ~10-15s/client (P4) | ⚠️ IMDS removido em May; ~ms hoje |
| Materializer resurrect incondicional (C5) | ⚠️ Guard payload existe; race out-of-order estreito real |
| 2 crons mortos | 🆕 São 3 (heartbeat incluído) |
| DLQ sem replay tooling | 🆕 Script existe mas morre no HMAC + DLQ é in-memory |
| Dispute não tratado | ⚠️ Container trata created; worker não; refund ninguém trata |
| — | 🆕 Seed Ed25519 em plaintext (HEAD) — perdido por TODOS os 7 agents |

Lição: agent acha, verifier decide, mãos próprias selam. Claims de ausência ("não existe X") são os mais perigosos — exigem grep negativo explícito.

---

# 10. PLANO DE AÇÃO PRIORIZADO

| # | Ação | Bloqueia | Esforço |
|---|---|---|---|
| 0 | Rotacionar AUDIT_CHAIN_SIGNING_SEED_HEX → secret put; remover var; regra gitleaks | segurança | 30min |
| 1 | Land/discard os 57 staged + 15 untracked (gc_worker PRIMEIRO — desbloqueia #2) | higiene | dias, splitável |
| 2 | `scheduled()` handler + declarar gc_worker + blob_meta writes + rota gc (resolve CRIT-1 + DSR + heartbeat) | produto | 1-2 dias |
| 3 | Secrets matrix: 10 rows/allowlist + CLAUDE.md truth-pass | gates | 1h |
| 4 | KV-delete no revoke (ou corrigir 4 docs) | abuse-response | horas |
| 5 | Billing resurrect: guard SQL-level no materializer | dinheiro | horas |
| 6 | Failover N_MIN floor + histerese | 3am-pages | horas |
| 7 | DLQ durável + replay funcional + alerta idade | dinheiro | 1 dia |
| 8 | AC put_if_none_match + hoist ac_create_only | supply-chain | horas |
| 9 | PERF-1 batch audit inserts | latência global | 1-2 dias |
| 10 | Wire runQuotaBatch | 150-300ms/req | **1 linha** |
| 11 | find_missing fanout | bazel builds | horas |
| 12 | Main-key hex validation + JWKS cooldown | silent-401/DoS | horas |
| 13 | Bytes end-to-end (drop to_vec) | CPU/heap | horas |
| 14 | CLAUDE.md + OKF reconcile (cadence real, paths, replication-failover concept) | agent-truth | horas |
| 15 | Coordinator server-side ts + dedicated repl key | failover-integrity | horas |
| 16 | Rate-limit: deny-tombstones + tier TTL | fairness | dia |
| 17 | Entitlement reconciliation job + refund/dispute no worker | money-drift | dia |
| 18 | Crons redundantes → dispatch-only; concurrency nos 21 | custo Mac | meio dia |
| 19 | Migrations cleanup + root rot + semgrep excludes | higiene | meio dia |
| 20 | QueryWriteStatus + decisão sha256/blake3 | conformance | decisão+dev |

**Pós-fix, o produto fica:** isolamento impecável (já é), lifecycle de storage real, billing defensível com cartão real, failover que não pisca, e 40–70% mais rápido nas regiões distantes.

---

*Relatório gerado de 7 auditorias + 5 verificações independentes + confirmação manual. Toda flag carrega file:line verificável. Refutações e correções de escopo documentadas na seção 9.*
