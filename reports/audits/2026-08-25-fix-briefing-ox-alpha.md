# BRIEFING DE EXECUÇÃO — correções pós-auditoria (CoreLink)

**Destinatário:** agente executor (`ox alpha` / OpenRouter) + sub-agentes paralelos
**Autor:** verificação independente contra `origin/main` @ `8cd0f920` + API de produção Cloudflare
**Data:** 2026-08-25
**Repo:** `HuGR-Labs/corelink-server` (privado)
**Status deste documento:** auto-suficiente. Você não precisa de nenhum contexto anterior.

---

## 0. LEIA ISTO PRIMEIRO — 8 armadilhas que matam esta tarefa

Estas oito coisas já aconteceram neste repositório. Não são hipóteses.

1. **O diretório principal do repo NÃO está em `main`.** Ele está em
   `feat/remediation-gated-features` @ `0a5e3349` (2026-08-07), uma branch **local,
   nunca pushada, em quarentena permanente**, 316 commits atrás de `origin/main`, com
   55 arquivos staged e 16 untracked que **não têm backup em lugar nenhum**.
   **Você nunca trabalha nesse diretório.** Todo trabalho acontece em git worktrees
   novos criados a partir de `origin/main`.

2. **`git add -A` / `git add .` / `git commit -a` são PROIBIDOS.** Worktrees
   compartilham o mesmo clone e o mesmo stash. Um `git add -A` de um agente captura
   o WIP não-commitado de outro agente e o módulo `gc_worker/` que só existe no disco
   do dono. **Sempre commite por nome de arquivo explícito.**

3. **Os runners de CI self-hosted SÃO o Mac do dono.** Compilações pesadas e rajadas
   de CI já derrubaram a máquina. Capacidade real ~7 jobs. Cada push custa ~US$ 0,15
   e dispara ~13 workflows. **Máximo de 3 PRs em voo simultaneamente.** Use
   `CARGO_BUILD_JOBS=4` em toda compilação local.

4. **Um relatório de auditoria anterior mediu a branch em quarentena e reportou como
   se fosse `main`.** Vários dos seus "achados" são falsos em `main`. A §2 deste
   documento lista o que está **REFUTADO — NÃO CONSERTE**. Se você "consertar" um
   item refutado, você está introduzindo uma regressão.

5. **Não adicione um handler `scheduled()` ao Worker.** Ver §7, item OWNER-3. Existe
   um cron de chaos-engineering herdado em produção que hoje é inerte só porque não
   há handler. Adicionar o handler o acorda em prod.

6. **Nunca faça `--admin` merge.** Merge é **um único comando**:
   `bash scripts/pre-merge-gate-check.sh --merge <PR>`. Nunca use pipe (`| tail`,
   `| grep`) nesse comando — o exit status do pipeline é o do último comando e a
   recusa do portão é descartada. Foi assim que um PR entrou com 4 checks pendentes.

7. **Toda mudança em código citado pelo wiki OKF (`docs/knowledge/`) pode deixar o
   portão C5 vermelho.** Ver §5.6. Commite o **código primeiro**, depois reconcilie a
   documentação em um **commit separado**.

8. **Todo commit `feat:`/`fix:` exige uma entrada em `CHANGELOG.md` sob
   `## [Unreleased]`, exatamente UMA, e um trailer `Signed-off-by:` (DCO).**
   Com múltiplos PRs paralelos isso gera conflito de merge garantido. Ver §5.4.

---

## 1. CONTEXTO MÍNIMO DO PRODUTO

CoreLink é uma plataforma multi-tenant de **cache content-addressable + governança de
armazenamento** rodando em Cloudflare (Workers + Durable Objects + Containers + R2 +
D1). Cerca de 73 crates Rust. Vendida self-serve para SMBs.

Superfícies de cache expostas: CAS/AC nativo, **Bazel REAPI v2**
(`crates/corelink-container/src/routes/bazel_v2.rs`), **Turborepo**
(`.../routes/turbo_v8.rs`) e **sccache** (WebDAV).

Dois planos de execução importam para este briefing:

- **Worker** (TypeScript, `worker/src/`) — plano de controle na borda. Auth, quota,
  roteamento. Deployado como `corelink-prod`.
- **Container** (Rust, `crates/corelink-container/`) — plano de dados. R2, D1, as
  superfícies de cache.

Há também `apps/signup-worker/` — um Worker separado que é a **autoridade primária de
billing** (handler Stripe vivo). O container é um segundo escritor de defesa em
profundidade. Essa dualidade é a raiz do WP-A.

Banco: **D1** (SQLite gerenciado). Migrations em `migrations/d1/`. Em produção o
binding `CONFIG_DB` aponta para `corelink-config-prod`
(`D1_DATABASE_ID = d64742ea-e102-40b2-a844-ff02e3f94562`).

---

## 2. ⛔ ITENS REFUTADOS — NÃO CONSERTE NADA DISTO

Um relatório anterior alegou os problemas abaixo. Cada um foi verificado contra
`origin/main` e/ou a API de produção da Cloudflare e **é falso**. Se você mexer
nestes pontos, está causando dano.

| Alegação falsa | Verificação |
|---|---|
| Seed Ed25519 `c4bdb99f…` commitada em plaintext; exige rotação | `grep` do valor em `origin/main`: zero. `git log -S` em `origin/main`: zero. API CF de `corelink-prod`: `AUDIT_CHAIN_SIGNING_SEED_HEX` **ausente** como VAR e como SECRET. O valor só existe num commit local não-pushado. **Não rotacione nada. Não crie regra de gitleaks para isto.** |
| Portão de secrets vermelho (`code_only=10`) | `python3 scripts/validate_secrets_matrix.py` em `main`: `matrix=223 code=191 in_both=191 code_only=0`, **exit 0**. Verde. |
| 3 crons de produção mortos apontando para `/_internal/gc/run` | `main/wrangler.toml` **não tem** `[env.prod.triggers]`. Zero ocorrências de `/_internal/gc/run` no repo. API CF: prod tem exatamente 2 crons (`0 6 * * 1`, `0 14 * * 1`). |
| `runQuotaBatch` escrito mas nunca conectado ("fix de 1 linha") | Conectado em `worker/src/index.ts:3532` e `:4054`. Já está no hot path. |
| Conceito OKF `replication-failover` diz "designed-not-wired" | `docs/knowledge/planes/replication-failover.md:4` já diz `STATUS: SPLIT. The read-side failover-router IS live…`. Já reconciliado. |
| CLAUDE.md mente sobre a cadência dos heavy gates | CLAUDE.md em `main` descreve o estado real. O relatório é que errou: `ffi-matrix-ci.yml` tem **zero** cron e `s10-ship-gate.yml` **não existe** (deletado no PR #1091). |
| TOCTOU em `route_write`; failback sem gate de catch-up (`coordinator.rs`) | `coordinator.rs` e `route_write` **não existem** em `main`. `failback.rs` existe mas é o gate de drain do `audit_outbox`, sem cooldown de 24h. Citações apontam para código ausente. |
| Sem spec OpenAPI | `openapi/corelink-v1.json` e `.yaml` existem (OpenAPI 3.1.0, 44 paths). |
| Portal de billing do cliente não existe | Rota montada e testada em produção. |
| `findMissingBlobs` totalmente serial | O seam `exists_batch` já existe (`crates/corelink-bazel-bridge/src/find_missing.rs:179`); o loop serial é só fallback. Falta apenas concorrência de probes — **não está no escopo deste briefing**. |
| `audit-drain` materializa o backlog inteiro em `Vec` (risco de OOM) | `AUDIT_DRAIN_BATCH_LIMIT` existe, default 200 (`audit_drain.rs:405`), com flag `incomplete` na resposta. Metade refutada. (A outra metade é o WP-L.) |
| Crons redundantes em buck2-starter, mutation-nightly, sbom; `gc-ship-gate` | Já não têm cron. `gc-ship-gate` não existe. Sobraram só dois (ver WP-H). |

---

## 3. AS 15 WORK-PACKAGES

Regra fundamental: **cada WP é uma branch, um PR, um conjunto de arquivos exclusivo.**
Nenhum arquivo aparece em dois WPs. Essa disjunção é o que permite paralelizar sem
conflito. Não expanda o escopo de um WP para dentro de outro; se você achar que
precisa, **pare e escale**.

Prioridade: `P0` = blocker real, `P1` = importante, `P2` = higiene.

---

### WP-A — `P0` Billing pode ressuscitar assinatura cancelada

**Arquivos exclusivos:** `crates/corelink-billing-stripe-materializer/src/d1.rs`

**O defeito.** O materializer do container tem um guard de payload
(`handler.rs:519`, `subscription_status_grants_access`) que barra eventos com
`status=canceled`. Mas o guard lê **somente o payload do evento**. O UPSERT em si é
cego:

```
crates/corelink-billing-stripe-materializer/src/d1.rs:200
SQL_UPSERT_TIER = "INSERT INTO tier_selections (tenant_id, tier, subscription_state,
  subscription_started_at_ms, correlation_id) VALUES (?, ?, 'active', ?, ?)
  ON CONFLICT(tenant_id) DO UPDATE SET tier = excluded.tier,
  subscription_state = 'active', subscription_started_at_ms = excluded.subscription_started_at_ms,
  correlation_id = excluded.correlation_id"
```

Não há condição sobre o estado atual. Um `customer.subscription.updated(status=active)`
**stale** — redelivery atrasado, ou reordenamento do Stripe — que chegue **depois** do
`deleted` já processado passa pelo guard de payload e regrava `'active'`. Cliente
cancelado volta a ter acesso pago.

**Prova de que o time já conhece a classe do bug.** O escritor gêmeo — o
signup-worker, que é a autoridade primária — tem exatamente o guard em nível de banco:

```
apps/signup-worker/src/webhooks/stripe.ts:863
  AND status != 'canceled'
apps/signup-worker/src/webhooks/stripe.ts:833  (comentário)
  "…must NOT resurrect a canceled subscription"
```

**A correção.** Espelhar o guard no SQL do materializer usando a cláusula `WHERE` do
`DO UPDATE` (suportada por SQLite desde 3.24, portanto por D1):

```sql
INSERT INTO tier_selections
  (tenant_id, tier, subscription_state, subscription_started_at_ms, correlation_id)
VALUES (?, ?, 'active', ?, ?)
ON CONFLICT(tenant_id) DO UPDATE SET
  tier = excluded.tier,
  subscription_state = 'active',
  subscription_started_at_ms = excluded.subscription_started_at_ms,
  correlation_id = excluded.correlation_id
WHERE NOT EXISTS (
  SELECT 1 FROM tenant_billing tb
   WHERE tb.tenant_id = tier_selections.tenant_id
     AND tb.status = 'canceled')
```

**Restrição de projeto que você NÃO pode violar — motivo pelo qual a subquery é
correlacionada e não parametrizada.** Existe um teste de invariante que conta os
placeholders:

```
crates/corelink-billing-stripe-materializer/src/d1.rs:971
assert_eq!(SQL_UPSERT_TIER.matches('?').count(), 4, "{SQL_UPSERT_TIER}");
```

A subquery acima **não adiciona nenhum `?`** — ela correlaciona pelo
`tier_selections.tenant_id` da própria linha em conflito. Se você parametrizar o
tenant de novo, o assert quebra **e** os call sites passam a ter aridade errada.
Mantenha 4 binds.

Os outros asserts do mesmo bloco (`:972` `contains("'active'")`, `:974`
`contains("subscription_started_at_ms")`, `:978` `!contains("materialized_at_ms")`)
continuam válidos sem alteração.

**Semântica que você precisa preservar:**
- Se **não existe** linha em `tenant_billing`, `NOT EXISTS` é verdadeiro → o UPDATE
  ocorre. Correto: primeira compra nunca é bloqueada.
- O ramo **INSERT** (linha inexistente em `tier_selections`) não é afetado pelo
  `WHERE` do `DO UPDATE`. Correto: sem linha prévia, não há o que ressuscitar.
- `SQL_DOWNGRADE_TIER` (`:232`) **não muda**. Ele já escreve `'inactive'` e já é
  convergente com o worker.

**Pré-voo obrigatório — se falhar, ESCALE em vez de improvisar.**
Confirme que `tier_selections` e `tenant_billing` vivem **no mesmo banco D1** que este
escritor acessa. Ambas as DDLs estão em `migrations/d1/`
(`0039_tier_selection.sql:34` e `0055_tenant_billing.sql:28`), o que é forte indício
de que sim. Mas há **dois** caminhos de escrita para esta constante:

- `crates/corelink-container/src/billing_d1_http.rs:342` (container, D1 via HTTP)
- `crates/corelink-billing-stripe-materializer/src/wasm32_binders.rs:289` (wasm/Worker)

Verifique que ambos apontam para o mesmo D1. Se apontarem para bancos diferentes, a
subquery cross-database é impossível e o desenho tem de mudar — **pare e reporte**.

**Este SQL JÁ FOI PROVADO contra as DDLs reais.** Foi executado em SQLite com a DDL de
`migrations/d1/0039_tier_selection.sql` + `0055_tenant_billing.sql`, com estes
resultados medidos:

```
placeholders '?' : 4                                  (assert :971 preservado)
T1  tenant_billing.status='canceled', linha 'inactive' → ('pro','inactive')  NÃO ressuscita ✓
T2  tenant_billing.status='paid',     linha 'inactive' → ('pro','active')    concede ✓
T3  sem linha em tenant_billing                        → ('team','active')   primeira compra ✓
T4  sem linha em tier_selections + billing 'canceled'  → ('pro','active')    ramo INSERT NÃO é bloqueado
```

Você ainda deve reproduzir isso no seu ambiente antes de escrever o Rust — mas o
desenho está validado; se der diferente, o problema é a sua transcrição do SQL.

**Residual conhecido, documentado de propósito (caso T4).** O `WHERE` do `DO UPDATE`
não protege o ramo **INSERT**: um tenant com `tenant_billing.status='canceled'` mas
**sem** linha em `tier_selections` recebe uma linha nova `'active'`. Na prática isso
não ocorre, porque o caminho de cancelamento (`SQL_DOWNGRADE_TIER` e o writer do
signup-worker) grava `'inactive'` em vez de deletar a linha — então uma linha
inexistente significa "tenant nunca teve tier", não "tenant cancelado". **Não tente
fechar isso** reescrevendo o statement como `INSERT … SELECT … WHERE NOT EXISTS`: você
quebraria a interação com o `ON CONFLICT` e a aridade dos binds. Escreva o residual
como comentário no rustdoc da constante, com esta justificativa.

**Testes de aceitação (escreva-os VERMELHOS primeiro):**
1. `tenant_billing.status='canceled'` + `SQL_UPSERT_TIER` → `tier_selections.subscription_state`
   **não** vira `'active'`.
2. `tenant_billing.status='paid'` + `SQL_UPSERT_TIER` → vira `'active'` (não-regressão).
3. Sem linha em `tenant_billing` + `SQL_UPSERT_TIER` → vira `'active'` (primeira compra).
4. Bind count continua 4 (o assert existente em `:971`).
5. Caso T4 fixado como comportamento **conhecido e aceito**, não como bug.

**Tipo de commit:** `fix(billing):`

---

### WP-B — `P0` Failover congela todos os writes com 3 requests ruins

**Arquivos exclusivos:**
- `crates/corelink-failover-router/src/health.rs`
- `crates/corelink-failover-router/src/router.rs` (apenas rustdoc)
- `crates/corelink-replica-worker/src/audit.rs`
- `crates/corelink-container/src/routes/failover.rs`

Este WP agrega três achados que tocam os mesmos arquivos. Mantê-los juntos é o que
evita conflito — **não os separe**.

#### B.1 — Sem piso de amostragem (o defeito principal)

A decisão de saúde de região exige 3 sinais simultâneos numa janela de 5s:

```
crates/corelink-failover-router/src/health.rs:241
let health = if triggers.len() >= 3 { RegionHealth::Degraded } else { RegionHealth::Healthy };
```

Os três gatilhos são: taxa de 5xx > 1%, p99 > 300ms, ≥3 falhas consecutivas. A única
proteção contra amostra minúscula é:

```
crates/corelink-container/src/routes/failover.rs:199
if total == 0 { /* healthy */ }
```

**Aritmética do desastre:** 3 requests consecutivos, lentos e com 500, dentro de 5s →
taxa = 100% (>1% ✓), p99 = latência deles (>300ms ✓), consecutivas = 3 (≥3 ✓) →
`Degraded` → o `failover_guard` responde **503 `failover_readonly` para TODO write**.
Três requests ruins congelam a escrita da região.

Agrava: o sinal é **auto-referencial** — o probe conta as respostas do próprio
container. Um backend meio doente alimenta o próprio congelamento (amplificador de
brownout).

**A correção — e por que ela vai no probe, não no `evaluate`.**
`RegionHealthSnapshot::evaluate` é uma função **pura** com doctests que fixam o
comportamento (`health.rs:215-221` afirma `assert_eq!(snap.active_triggers.len(), 3)`).
Mudar a assinatura dela para receber uma contagem de amostras quebra os doctests e
todos os call sites. **Não faça isso.**

Em vez disso, aplique o piso no probe, onde a contagem já existe:

```
crates/corelink-container/src/routes/failover.rs — em `impl HealthProbe for RollingMetricsHealthProbe`, fn probe(), linha ~198
```

Troque `if total == 0` por `if total < MIN_SAMPLES_FOR_FAILOVER`, retornando o mesmo
snapshot saudável que o ramo atual já constrói. Defina:

```rust
/// Piso de amostragem: abaixo disto a janela de 5s não é estatisticamente
/// significativa e NENHUM sinal de degradação conta. Sem este piso, 3 requests
/// ruins consecutivos congelam os writes da região inteira.
const MIN_SAMPLES_FOR_FAILOVER: usize = 50;
```

Torne-o sobrescrevível por env var (`FAILOVER_MIN_SAMPLES`) usando o helper que o
crate já usa para isso — `crate::storage::env_or` / `non_empty_env`. **Se adicionar
uma env var nova, você DEVE registrá-la na matriz de secrets** (ver §5.3), senão o
portão fica vermelho.

#### B.2 — Histerese

Além do piso, exija **N probes degradados consecutivos** antes de o guard tripar, e
uma recuperação com grace. O estado vive em `FailoverLayerState`
(`crates/corelink-container/src/routes/failover.rs`, construído em `with_primary`,
~linha 285) como um `Arc<AtomicU32>` de contador. Sugestão: 3 probes consecutivos
para tripar, 5 para destripar.

#### B.3 — Sink de auditoria de failover cresce sem limite

```
crates/corelink-container/src/routes/failover.rs:286
let audit = Arc::new(InMemoryFailoverAuditSink::new());
```

`InMemoryFailoverAuditSink` é um alias de `InMemoryReplicaAuditSink`, definido em:

```
crates/corelink-replica-worker/src/audit.rs:134
records: std::sync::Mutex<Vec<ReplicaAuditRecord>>,
...
fn emit(&self, record) { self.records.lock()?.push(record); Ok(()) }
```

`push` puro, sem teto, no wiring de **produção**. Cresce por toda a vida do processo.
Pior: os eventos de failover de região — exatamente os mais dignos de auditoria —
nunca chegam a D1 nem à cadeia de auditoria.

**Correção:** adicione um construtor `with_capacity(n)` que transforma o `Vec` num ring
buffer (descarta o mais antigo ao estourar) e use-o no wiring de produção com um teto
explícito (sugestão: 10 000). **Não mude o comportamento do `new()` existente** — os
testes dependem de retenção total. Adicione, não substitua.

#### B.4 — Rustdoc mente sobre "sustained"

```
crates/corelink-failover-router/src/router.rs:10-11
//! All sustained within 5s window (prevents false-positive from transient
//! slowness misdetected as outage).
```

Não existe máquina de estados de sustentação; `evaluate` avalia um snapshot
instantâneo. Além disso, `RegionHealth::Down` **nunca é produzido** por `evaluate` —
só aparece em código de exibição (`health.rs:99`, `:115`), então dashboards e alertas
consomem um estado morto.

**Correção:** corrija o rustdoc para descrever o comportamento real **depois** das
mudanças B.1/B.2 (o piso + histerese são, de fato, uma sustentação parcial — diga
exatamente isso) e documente explicitamente que `Down` é inalcançável hoje, ou remova
a variante. **Não invente uma máquina de estados nova** — isso é escopo novo.

**Testes de aceitação:**
1. 3 requests 500 lentos dentro da janela → `Healthy` (era `Degraded`).
2. 50+ amostras com 5xx real > 1%, p99 > 300ms, ≥3 consecutivas → `Degraded`
   (não-regressão: o failover ainda funciona quando deve).
3. Histerese: um único probe degradado não tripa o guard; três consecutivos tripam.
4. Sink com teto N: emitir N+10 registros mantém `len() == N` e preserva os mais
   recentes.
5. Os doctests existentes de `evaluate` continuam passando **sem edição** — se você
   precisou editá-los, mudou a assinatura e escolheu o caminho errado.

**Tipo de commit:** `fix(failover):`

---

### WP-C — `P1` Action Cache: janela de envenenamento entre GET e PUT

**Arquivo exclusivo:** `crates/corelink-container/src/storage/r2_s3.rs`

**O defeito.** `R2AcHandler::update` (declarado em `r2_s3.rs:2856`) implementa a
imutabilidade do Action Cache com um GET-compare-PUT **não atômico**:

- `~:2968` — `let existing = ... self.client.get(&key)`
- compara: divergente → `DivergentBody` (409); igual → no-op; ausente → cai para o PUT
- `~:3003` — `self.client.put(&key, payload)` — **incondicional**

Dois escritores concorrentes com bodies divergentes para o mesmo `action_digest`:
ambos veem `Ok(None)`, ambos fazem PUT, o último ganha **por cima de um ActionResult
provado**. O CAS é imune (content-addressed, o conteúdo é o nome); o AC não. É uma
janela de supply-chain intra-tenant.

**A correção é pequena — o helper já existe.** Um relatório anterior alegou que
"não há nenhum `IfNoneMatch` no repo". **Falso.** Existe:

```
crates/corelink-container/src/storage/r2_s3.rs:181
pub async fn put_if_absent(&self, key: &str, bytes: Vec<u8>) -> Result<bool, String>
    ... .if_none_match("*") ...
    // Ok(true)  = este chamador criou o objeto
    // Ok(false) = 412 PreconditionFailed, a chave já estava tomada
```

E `R2AcHandler` já possui um `client: R2S3Client` (`r2_s3.rs:2341-2342`), o mesmo tipo
que expõe `put_if_absent`. Ou seja: o método está ao alcance direto de `self.client`.

**Faça:** troque a chamada `put` do caminho de `update` por `put_if_absent` e trate o
`Ok(false)`:
- `Ok(true)` → sucesso, `durable = true` (comportamento atual).
- `Ok(false)` → outro escritor venceu a corrida. **Re-GET** e compare com
  `stored_view`: bytes iguais → no-op idempotente (`durable = false`); bytes
  divergentes → `DivergentBody` (409), exatamente como o compare pré-PUT.
- `Err(_)` → mesmo tratamento de erro de hoje.

**Não altere** o caminho do CAS (`R2CasHandler`) — ele é content-addressed e o `put`
incondicional lá é correto e mais barato.

**Preserve, sem exceção:**
- O wrapper de telemetria `crate::origin_timing::PhaseScope::enter(Phase::Store)` que
  envolve as chamadas de I/O.
- As emissões de auditoria antes da mutação (`UpdateAttempted`) e a negação
  cross-tenant no topo da função.
- A ordem BYOK: `resolve_byok` → `encrypt_body` → compare contra `stored_view`
  (ciphertext para tenant ativo). O compare tem de continuar contra os **bytes que
  seriam armazenados**, não contra o plaintext.

**Testes de aceitação:**
1. Dois `update` concorrentes com bodies divergentes, mesmo digest → exatamente um
   sucesso, o outro recebe `DivergentBody`; os bytes armazenados são os do vencedor e
   **nunca** os do perdedor.
2. Dois `update` concorrentes com bodies idênticos → ambos 200, um `durable=true` e
   um `durable=false`, nenhum erro.
3. Não-regressão: primeiro write num digest virgem continua `durable=true`.
4. Não-regressão: re-PUT byte-idêntico continua no-op sem tocar R2.

**Tipo de commit:** `fix(ac):`

---

### WP-D1 — `P1` DLQ de webhook Stripe é in-memory (perda de dinheiro no restart)

**Arquivos exclusivos:**
- `crates/corelink-stripe-real/src/dlq.rs`
- `crates/corelink-container/src/main.rs`
- `migrations/d1/<NOVO>.sql`

**O defeito.** O dispatcher de webhook commita a linha de dedup **antes** de
materializar (passo 5 de `crates/corelink-stripe-real/src/webhook_dispatch.rs`). Se a
materialização falhar de forma transitória (flake do D1), o Stripe re-tenta, bate em
`AlreadyProcessed` e o handler é **pulado para sempre**.

Isso é **deliberado e documentado** (F-008, ver `webhook_dispatch.rs:1021` e o
comentário em `:715-723`) — **não mude essa ordem.** A mitigação projetada é a DLQ: o
evento vai para quarentena e pode ser reprocessado.

O problema é que **a mitigação é volátil**:

```
crates/corelink-container/src/main.rs:901
let webhook_dlq = Arc::new(InMemoryWebhookDlqStore::new());
```

Todo redeploy de container evapora a quarentena. Um cliente pagante cujo
`checkout.session.completed` caiu num flake fica sem acesso, de forma invisível e
irrecuperável.

**A correção.** Existe o trait:

```
crates/corelink-stripe-real/src/dlq.rs:259
pub trait WebhookDlqStore: core::fmt::Debug + Send + Sync {
    fn try_quarantine(&self, row: WebhookDlqRow) -> Result<DlqQuarantineOutcome, DlqError>;
    ...
}
```

Hoje há **uma única** implementação (`InMemoryWebhookDlqStore`, `dlq.rs:359`).
Implemente uma segunda, persistente em D1, e troque o wiring de produção.

Passos:
1. Nova migration criando `webhook_dlq` com pelo menos: `event_id` (PK / UNIQUE),
   `event_type`, `payload` (JSON bruto), `attempt_count`, `first_seen_at_ms`,
   `last_seen_at_ms`, `last_error`, `resolved_at_ms` (NULL = ainda em quarentena).
   **A idempotência sobre `event_id` é o contrato do trait** — primeira chamada
   insere e retorna `Inserted`; chamadas seguintes incrementam `attempt_count` e
   retornam `Updated`. Implemente com `INSERT … ON CONFLICT(event_id) DO UPDATE`.
2. `D1WebhookDlqStore` em `dlq.rs`, usando o mesmo cliente D1-sobre-HTTP que os outros
   stores do container já usam (siga o padrão de `billing_d1_http.rs`).
3. Trocar `main.rs:901` para o store D1, mantendo `InMemoryWebhookDlqStore` intacto
   para os testes (`crates/corelink-stripe-real/tests/prop_dlq.rs` depende dele).
4. Aproveite e corrija, **no mesmo arquivo `main.rs`**, o achado irmão MED-5:
   `main.rs:853` usa `InMemoryBillingAuditEmitter` ao lado de um writer D1 durável —
   estado durável, evidência volátil. Aponte-o para um sink durável seguindo o mesmo
   padrão. *(Este item está aqui porque toca `main.rs`; separá-lo criaria conflito.)*

**Numeração da migration — armadilha real.** Já existem **duas** migrations `0044_*`
neste repo por colisão de numeração paralela. Aloque o número **no momento do push**,
não no momento em que começar a escrever, e confira `ls migrations/d1/ | tail -5`
imediatamente antes. Se o WP-K estiver em voo, coordene.

**Testes de aceitação:**
1. Quarentena → "restart" (nova instância do store contra o mesmo D1) → a linha ainda
   está lá.
2. Duas quarentenas do mesmo `event_id` → uma linha, `attempt_count = 2`, primeira
   retorna `Inserted` e a segunda `Updated`.
3. Os property tests existentes em `prop_dlq.rs` continuam verdes sem edição.

**Tipo de commit:** `fix(billing):`

---

### WP-D2 — `P1` Ferramenta de replay de webhook não funciona

**Arquivos exclusivos:**
- `scripts/forensics/replay-stripe-webhook.sh`
- `crates/corelink-stripe-real/src/webhook_dispatch.rs`

**DEPENDE DE WP-D1.** Não comece antes do WP-D1 estar mergeado — o replay precisa ler
a DLQ durável. Esta é a **única** dependência de ordem entre WPs.

**O defeito.** O script existe e parece uma ferramenta de recuperação, mas não
recupera nada:

```
scripts/forensics/replay-stripe-webhook.sh:110
-H "Stripe-Signature: t=replay,v1=replay-${EVENT_ID}"
```

Assinatura fabricada. Morre na verificação HMAC do dispatcher. Além disso o script
consulta a tabela de eventos **processados**, não a DLQ.

**A correção.** Um caminho de replay autenticado que:
- lê da tabela `webhook_dlq` (a do WP-D1), não da de processados;
- **contorna a linha de dedup** para o `event_id` específico sendo reprocessado — esse
  é o ponto inteiro: a linha de dedup é justamente o que bloqueia o retry natural;
- é autorizado por chave interna, **não** por assinatura Stripe fabricada. Use o
  padrão de auth de consumidor `/_internal/*` que o repo já usa (veja
  `requireConsumerAuth` no worker e os handlers `/_internal/*` do container);
- marca `resolved_at_ms` no sucesso.

**Atenção de segurança:** um endpoint que pula a verificação de assinatura E a dedup é
uma primitiva perigosa. Ele **precisa** ser: escopado a um único `event_id` que já
esteja na DLQ (nunca a payload arbitrário vindo do chamador — o payload sai da linha
da DLQ, que já foi HMAC-verificada quando entrou), fail-closed no auth, e auditado.
Se você não conseguir satisfazer as três, **escale em vez de shippar**.

**Se adicionar env var nova (chave dedicada de replay), registre na matriz** — §5.3.

**Testes de aceitação:**
1. Evento em quarentena + replay autenticado → materializa, `resolved_at_ms` gravado.
2. Replay sem auth → 401/403, nada materializado.
3. Replay de `event_id` inexistente na DLQ → 404, nunca materializa payload arbitrário.
4. Replay de evento já resolvido → idempotente, não duplica efeito.

**Tipo de commit:** `fix(billing):`

---

### WP-E — `P1` Promoção de replicação confia em timestamp do cliente

**Arquivo exclusivo:** `worker/src/replication_coordinator_do.ts`

**O defeito.**

```
worker/src/replication_coordinator_do.ts:581
const ts_ms = Number(b.ts_ms ?? now_ms);   // relógio do CLIENTE
```

O bundle de lag também vem do cliente (`:583-587`). O gate de frescor
(`:181-182`) compara esse `ts_ms` com `now_ms`. Um reporter com clock skew — ou
qualquer detentor da chave compartilhada `CORELINK_INTERNAL_AUTH_KEY` — marca o
primary como stale; o loop de `alarm()` promove a réplica em ≤30s e congela os writes
de um primary saudável.

**A correção.** O DO carimba a hora de **recebimento**, server-side, e **ignora**
`b.ts_ms` para efeito de frescor. Mantenha aceitar o campo no wire por
compatibilidade, mas trate-o como telemetria informativa, nunca como entrada de
decisão. Documente isso no rustdoc/jsdoc do handler.

Considere também uma chave dedicada para `/_repl/*` em vez da chave interna
compartilhada — o próprio arquivo comenta que vazar essa chave única habilita mint de
PAT admin de qualquer tenant. **Isso é mudança de secret + deploy: NÃO faça sozinho.**
Documente a recomendação num comentário e reporte como item de owner.

**Item irmão neste mesmo arquivo (MED-14) — atenção, provavelmente bloqueado.**

```
worker/src/replication_coordinator_do.ts:447
console.log(JSON.stringify({ kind: "replication_audit", ...rec }));
```

Quem promoveu qual região vive só em stdout, fora da cadeia de auditoria. A correção
natural seria escrever no `audit_outbox` em D1 — **mas este DO não tem binding de D1**
(uma varredura por `env.` / `CONFIG_DB` no arquivo não retorna nada). Adicionar um
binding é mudança de `wrangler.toml` e afeta o deploy.

**Portanto:** neste WP, **não** adicione binding. Faça o mínimo defensável — deixe o
log estruturado e correlacionável (já é JSON; garanta `request_id` e `region`) e
adicione um comentário `// TODO(owner): sink durável requer binding D1 neste DO` — e
reporte o item. Se você achar um caminho existente de fetch interno que já leve
eventos de auditoria à cadeia sem binding novo, use-o; caso contrário, pare.

**Testes de aceitação:**
1. Heartbeat com `ts_ms` no futuro distante → não afeta o veredito de frescor.
2. Heartbeat com `ts_ms` muito antigo → não marca o primary como stale.
3. Frescor passa a ser função só do tempo de recebimento server-side.
4. Não-regressão: um heartbeat honesto e pontual continua mantendo o primary saudável.

**Tipo de commit:** `fix(replication):`

---

### WP-F1 — `P1` Revoke de PAT não invalida o cache de borda (e 4 docs afirmam que sim)

**Arquivos exclusivos:**
- `worker/src/lib/runner_mint.ts`
- `worker/src/lib/tenant_suspend_gate.ts` (apenas comentário)
- `worker/src/lib/pat_verify_cache.ts` (apenas comentário)

**O defeito.** O revoke escreve só em D1:

```
worker/src/lib/runner_mint.ts:~651
UPDATE pat SET revoked_at_ms = ?1 WHERE pat_id = ?2 AND tenant_id = ?3 AND revoked_at_ms IS NULL
  (e a variante sem tenant, ~:657)
```

Uma varredura por `.delete(` de KV em **todo** `worker/src` retorna **zero** ocorrências.
O cache L2 de borda mantém a linha do PAT por até 60s
(`worker/src/lib/pat_verify_cache.ts:89`, prefixo `patrow:`, TTL 60s), mais a
consistência eventual do KV. Um PAT revogado continua funcionando na borda.

E três sites afirmam que o mecanismo existe. O mais grave:

```
worker/src/lib/tenant_suspend_gate.ts:46
 * revoke (worker revoke also KV-deletes the `patrow:` entry) and the container
```

Isso é um comentário que descreve um controle inexistente, e ele é a justificativa
citada para o comportamento fail-open do suspend-gate.

**A correção — o dado necessário já existe no schema.** A tabela `pat` tem `pat_id`
(PK, migration `0037_signup_orchestration.sql:96`) **e** `token_id` (coluna de lookup,
adicionada em `migrations/d1/0054_pat_token_id.sql`). A chave de KV é
`patrow:<token_id>`. Então:

1. Adicione `RETURNING token_id` ao `UPDATE` (D1/SQLite ≥3.35 suporta) e troque
   `.run()` por `.first()` para colher o valor.
2. Se um `token_id` voltou (linha realmente revogada agora), delete
   `patrow:<token_id>` do KV de metadados — o mesmo binding que `pat_verify_cache.ts`
   usa para escrever.
3. **Use `await`, não `waitUntil`.** O handler `handleRunnerRevoke` hoje recebe
   `(request, env, requestId)` — **não recebe `ExecutionContext`**. Threading de `ctx`
   obrigaria a editar `worker/src/index.ts`, que é território do WP-F2, e criaria
   conflito. Um `await` num teardown de job custa poucos ms e é aceitável.
   **Não edite `index.ts` neste WP.**
4. Trate a falha do KV-delete como **não-fatal**: o D1 já está revogado (a fonte da
   verdade), o TTL de 60s continua sendo o backstop. Logue e siga; nunca faça o revoke
   retornar 500 por causa de um KV lento.
5. Aplique o mesmo tratamento a **ambos** os ramos do UPDATE (com e sem
   `owner_tenant`).
6. Depois que o código estiver correto, os comentários de `tenant_suspend_gate.ts:46`
   e `pat_verify_cache.ts` passam a ser **verdadeiros** — revise-os para descrever o
   comportamento real e não os apague.

**Sobre os documentos.** `docs/knowledge/auth/pat-moat.md` e o ADR-0070 também
afirmam o controle. Depois que ele existir, eles ficam corretos. **Não edite esses
arquivos à mão** — rode o fluxo OKF da §5.6, que reancora os conceitos.

**Testes de aceitação:**
1. Revoke de um PAT com `token_id` conhecido → a chave `patrow:<token_id>` some do KV.
2. Revoke de `pat_id` inexistente → 200 idempotente, nenhum delete de KV, nenhum erro.
3. Re-revoke (já revogado) → 200 idempotente, `RETURNING` vazio, nenhum delete.
4. KV indisponível → o revoke ainda retorna 200 e o D1 ainda está gravado.
5. Ambos os ramos (com/sem `owner_tenant`) cobertos.

**Tipo de commit:** `fix(auth):`

---

### WP-F2 — `P2` Chave PAT principal sem a validação hex que as irmãs têm

**Arquivo exclusivo:** `worker/src/index.ts`

**O defeito.** A chave de assinatura principal é validada só por comprimento:

```
worker/src/index.ts:1274
if (signingKeyRaw.length < 64) { ... }
```

As chaves irmãs, no mesmo arquivo, têm validação completa:

```
worker/src/index.ts:1392-1396
const isValidHexKey = sibling.length >= 64 && /* even length + hex */ ...;
if (!isValidHexKey) { /* log CRITICAL + 503 */ }
```

Uma chave principal malformada (64+ chars mas não-hex) atravessa o guard,
`hexDecode()` retorna `null` a cada request, `verifyPatHmac` retorna false
silenciosamente e **todos os clientes legítimos tomam 401** — sem nenhum sinal
operacional.

**A correção.** Aplique exatamente o mesmo predicado `isValidHexKey` à chave
principal, com o mesmo tratamento fail-closed: log CRITICAL + 503. Extraia o
predicado para um helper compartilhado em vez de duplicá-lo — assim as duas não podem
divergir de novo.

**Testes de aceitação:**
1. Chave principal com 64 chars não-hex → 503 + log CRITICAL (era 401 silencioso).
2. Chave principal com comprimento ímpar → 503.
3. Chave válida → comportamento idêntico ao de hoje (não-regressão).
4. As irmãs continuam com o comportamento atual.

**Tipo de commit:** `fix(auth):`

---

### WP-G — `P2` Refresh de JWKS sem cooldown (amplificação de DoS)

**Arquivo exclusivo:** `crates/corelink-clerk/src/adapter.rs`

**O defeito.** Um JWT com `kid` desconhecido dispara um fetch HTTPS remoto ao Clerk
**por request**. Um grep por `cooldown`, `min_interval`, `last_refresh` e `backoff`
nesse arquivo retorna **zero**. Um atacante não-autenticado, mandando JWTs com `kid`
aleatório, queima latência do seu hot path, o rate limit do Clerk e egress.

**A correção.** Um intervalo mínimo entre refreshes disparados por kid-miss
(sugestão: 30s) ou um negative-cache de kids já vistos como ausentes. Guarde o
`last_refresh_ms` em estado atômico/mutex no adapter.

**Trade-off que você DEVE documentar no código:** uma rotação legítima de chave do
Clerk passa a levar até o intervalo escolhido para ser observada. Com 30s isso é
aceitável; deixe isso escrito no rustdoc para que ninguém trate como bug depois.

**Testes de aceitação:**
1. N requests com kids desconhecidos distintos dentro da janela → **um** fetch remoto.
2. Após a janela expirar, um novo kid-miss → novo fetch (rotação real ainda funciona).
3. Kid conhecido → nenhum fetch (não-regressão).

**Tipo de commit:** `fix(auth):`

---

### WP-H — `P2` Higiene de CI (portão incompleto, crons redundantes, sem concurrency)

**Arquivos exclusivos:**
- `scripts/pre-merge-gate-check.sh`
- `.github/workflows/*.yml`

**Execute este WP por ÚLTIMO.** Ele toca ~20 arquivos de workflow e vai gerar a maior
rajada de CI. Rode quando os outros PRs já tiverem drenado.

#### H.1 — O portão de pré-merge tem lista de presença incompleta

```
scripts/pre-merge-gate-check.sh:308
REQUIRED_PRESENT = ["dco", "gitleaks"]
```

A lógica é: estes workflows têm `on: pull_request` puro, sem filtro de path, portanto
disparam em **todo** PR; a **ausência** deles significa que os workflows de
`pull_request` não rodaram para aquele head sha, e o portão recusa o merge.

`changelog-validate.yml` tem exatamente essa mesma característica (bare
`pull_request`) e carrega o gate `feat:`/`fix:` → entrada no CHANGELOG. Ele **não**
está na lista. Se o run dele sumir, passa silenciosamente.

**Correção:** adicione `"changelog"` à lista. Confirme antes que o nome do check
contém essa substring em minúsculas (a comparação é por `in names`, com os nomes
lowercased em `:309`). Uma linha.

#### H.2 — Dois crons que violam a regra do próprio repo

A regra escrita do repo: **um cron só se justifica quando algo pode mudar SEM um
commit** (feeds de CVE, estado de produção, backups, drift de certificado,
reconciliação de billing, drills de DR). Qualquer coisa que só muda quando o código
muda pertence a um gatilho de PR/push.

Sobreviveram dois violadores:

- `.github/workflows/bazel-starter-ci.yml:43` — `cron: '0 6 * * 1'`, descrito como
  "sustained 7d green gate", ou seja, re-verificando código byte-idêntico num relógio.
- `.github/workflows/proptest-density-gate.yml:32` — `cron: '31 4 * * 4'`, redundante
  com a lane de PR do mesmo gate.

**Correção:** remova os dois `schedule:`, mantendo `workflow_dispatch` e os gatilhos de
PR. **Não toque** nos crons que ganham o próprio sustento: `codeql.yml` (`30 5 * * *`),
scanners de CVE, canários de produção, backups, drills.

#### H.3 — 19 workflows sem grupo de concorrência

De 121 workflows, 102 têm `concurrency:` e 19 não. Sem `cancel-in-progress`, pushes
sucessivos empilham runs sobre 5 runners compartilhados — no Mac do dono.

O pior deles é `nightly.yml`: TLC sobre 30 specs, `cargo test --release` do workspace,
fuzz 9×1h, mutants 240min.

Os 19 sem grupo:

```
backlog-verify.yml            byok_kill_switch_drill_weekly.yml   byok_matrix_weekly.yml
cas_foundation.yml            cf-deploy-prod.yml                  container-build-push-prod.yml
coverage.yml                  dr-drill-monthly.yml                fabric-soak-proof.yml
ffi-matrix-ci.yml             fuzz-nightly.yml                    mutation-nightly.yml
nightly.yml                   notarize-macos.yml                  perf-nightly.yml
release-slsa3.yml             sbom.yml                            sign-linux.yml
sign-windows.yml
```

**Correção:** copie o padrão de `concurrency:` dos 99 que já o têm. **Duas exceções
que exigem julgamento:** `cf-deploy-prod.yml` e `container-build-push-prod.yml` são
caminhos de deploy — cancelar um deploy no meio é pior que enfileirá-lo. Para esses
dois use um grupo **sem** `cancel-in-progress` (serializa, não cancela), ou
deixe-os fora e diga isso no PR. Mesma cautela para `notarize-macos.yml`,
`sign-linux.yml`, `sign-windows.yml` e `release-slsa3.yml` (assinatura/release).

**Testes de aceitação:** este WP não tem teste unitário. A prova é:
`bash scripts/pre-merge-gate-check.sh <PR>` (forma somente-relatório) rodando limpo, e
uma inspeção manual dos diffs de YAML. Não invente teste.

**Tipo de commit:** `chore(ci):` para H.2/H.3, `fix(ci):` para H.1.

---

### WP-I — `P2` Três coleções/estados sem limite ou divergentes

**Arquivos exclusivos:**
- `crates/corelink-container/src/routes/ratelimit_layer.rs`
- `crates/corelink-config-do/src/store.rs`
- `crates/corelink-container/src/routes/turbo_v8.rs`

Três defeitos independentes, agrupados só porque são pequenos e não colidem com nada.

#### I.1 — Tier de rate-limit congelado pelo tempo de vida do container

```
crates/corelink-container/src/routes/ratelimit_layer.rs:262
planned: Arc<Mutex<HashSet<Uuid>>>,
```

Uma vez que um tenant entra no set (`:412`), nunca sai (`:397-430`). Consequência: um
upgrade de plano é invisível até o container reiniciar. **Pior:** um tenant cujo tier
**não pôde ser resolvido** (D1 fora do ar) também é marcado como planned, então ele
fica preso no default mesmo depois que o D1 volta.

**Correção:** TTL nas entradas (sugestão 5 min) — troque o `HashSet<Uuid>` por um mapa
`Uuid → expira_em_ms` e trate expirado como ausente. **Não marque como planned quando
a resolução falhou** — esse é o bug mais nocivo dos dois; uma falha deve ser
retentável.

#### I.2 — Mapas de história do Config-DO crescem para sempre

```
crates/corelink-config-do/src/store.rs:190-192
history: BTreeMap<u64, ConfigVersionEntry>,
payloads: BTreeMap<u64, ConfigPayload>,
```

Existe uma retenção de 90 dias que gateia elegibilidade de rollback, mas os mapas
nunca são podados.

**Correção:** ao gravar uma versão nova, remova as entradas fora da janela de
retenção. Use a **mesma** constante de retenção que já gateia o rollback — não
introduza um segundo número, ou eles divergem.

#### I.3 — Lista de sentinelas divergente no Turborepo

```
crates/corelink-container/src/routes/turbo_v8.rs:504, :678, :885
const TENANT_SENTINELS: &[&str] = &["_anonymous", "_unknown", "_system", "_pending"];
```

Três cópias locais, **sem** `_oci` e `_public`, divergindo do helper canônico. O
`bazel_v2.rs` faz certo:

```
crates/corelink-container/src/routes/bazel_v2.rs:193, :382
if crate::auth_tenant::is_reserved_sentinel(raw) { ... }
```

Hoje isso está sombreado pela ordem dos extractors, mas é latente — e o drift de
copy-paste já aconteceu uma vez.

**Correção:** apagar as três listas locais e chamar
`crate::auth_tenant::is_reserved_sentinel`. Confirme que a semântica do helper casa
com o uso em cada um dos três sites antes de trocar.

**Testes de aceitação:**
1. (I.1) Tenant resolvido, avança o relógio além do TTL → re-resolve o tier.
2. (I.1) Falha de resolução → **não** marca planned; a próxima tentativa re-resolve.
3. (I.2) Gravar versões além da janela de retenção → mapas param de crescer e as
   entradas restantes são exatamente as elegíveis a rollback.
4. (I.3) Um tenant `_oci` e um `_public` são rejeitados nos três sites do turbo.

**Tipo de commit:** `fix(container):`

---

### WP-J — `P2` Materializer trata status ausente como concessão de acesso

**Arquivos exclusivos:** `crates/corelink-billing-stripe-materializer/src/handler.rs`

**O defeito.**

```
crates/corelink-billing-stripe-materializer/src/handler.rs:309
.unwrap_or(if canceled { "canceled" } else { "active" })
```

Um payload assinado mas **sem** o campo `status` vira uma concessão de acesso: o
`unwrap_or` transforma ausente em `"active"` **antes** de o valor chegar ao gate
`subscription_status_grants_access` (`:141`), que então aprova.

O escritor gêmeo faz o oposto: no signup-worker, ausente → `false` → sem entitlement.
E o próprio doc-comment do handler, logo acima do gate, **contradiz o código**:

```
crates/corelink-billing-stripe-materializer/src/handler.rs:126-128
/// ONLY `active` and `trialing` qualify. Fail-safe: an unknown/absent status is
/// treated as NOT grantable.
```

O gate cumpre o contrato; o `unwrap_or` em `:309` o burla antes que ele rode.

Mitigante honesto: o Stripe sempre manda `status`. É defesa em profundidade, não um
buraco explorável hoje.

**Correção:** status ausente → **pular a escrita** e emitir uma linha de
observabilidade (não silenciosamente falhar, não silenciosamente conceder). Alinhe com
o comportamento do worker.

**Cuidado:** pode haver testes que dependem do default atual. Se um teste quebrar,
**leia o teste antes de editá-lo** — se ele fixa o comportamento fail-open de
propósito, ele é o achado, e você atualiza o teste **e** explica no PR. Se ele só usa
o default por conveniência de fixture, conserte a fixture.

**Fora de escopo — não faça:** revogar acesso em `charge.refunded` ou em disputa
perdida. Isso é decisão de produto (ver §7, OWNER-4).

**Testes de aceitação:**
1. Payload sem `status` → nenhuma escrita em `tier_selections`, uma linha de
   observabilidade emitida.
2. Payload com `status=active` → concede (não-regressão).
3. Payload com `status=canceled` → não concede (não-regressão).

**Tipo de commit:** `fix(billing):`

---

### WP-K — `P2` Colisões de migration, migrations órfãs e apodrecimento da raiz

**Arquivos exclusivos:** `migrations/` (exceto `migrations/d1/`, ver aviso), `TODO.md`

#### K.1 — Colisão de numeração em `migrations/d1/`

```
migrations/d1/0044_drata_evidence_sent.sql
migrations/d1/0044_stripe_webhook_events_processed.sql
```

Dois `0044`. Como a ordem de aplicação é derivada do nome, isso é ambíguo.

**⚠️ NÃO RENOMEIE NEM DELETE NADA SEM CHECAR O LEDGER.** O D1 mantém uma tabela de
migrations aplicadas. Renomear um arquivo **já aplicado** faz o runner tentar
reaplicá-lo. Este repo já teve um incidente de dessincronização de ledger por
`execute --file` ad-hoc pulando o registro.

Procedimento obrigatório:
1. Leia o ledger de migrations do D1 de produção e liste o que já foi aplicado.
2. Se **ambos** os `0044` já foram aplicados: **não renomeie**. Documente a colisão
   num comentário no topo de cada arquivo e num ADR curto, e siga.
3. Só renumere um arquivo que comprovadamente **nunca** foi aplicado em nenhum
   ambiente.

#### K.2 — Migrations órfãs na raiz de `migrations/`

Fora do `migrations_dir` que o wrangler usa (`migrations/d1/`), na raiz existem:

```
migrations/0001_init.sql
migrations/002_auth_tables.sql       ← schema alheio ao stack atual
migrations/013_admin_op_log.sql      ← colisão 013
migrations/013_rotation_state.sql    ← colisão 013
migrations/N4__sub_processor_tables.sql  ← convenção de nome alienígena
migrations/neon/                     ← stack que não é o atual
```

**Correção:** **mova** para `migrations/_archive/` com um `README.md` explicando a
proveniência. **Nunca delete.** Confirme antes que nada em `wrangler.toml`, em scripts
ou em workflows referencia esses caminhos.

#### K.3 — `TODO.md` da raiz

```
TODO.md:1
# CoreLink — Roadmap MVP (12 semanas)
```

É um plano de abril, com Postgres/sqlx, semanas 1–12, tudo desmarcado, zero
correspondência com o stack atual (Cloudflare + D1 + R2). A fonte única de verdade
para trabalho aberto é `BACKLOG.md`.

**Correção:** mover para `docs/_archive/2026-04-todo-mvp.md` com uma nota de uma linha
no topo dizendo que é histórico e apontando para `BACKLOG.md`. Não delete.

**Testes de aceitação:** nenhum teste unitário. Prove com: `python3
scripts/validate_specs.py` verde, `python3 scripts/backlog_verify.py` verde, e um grep
provando que nada referencia os caminhos movidos.

**Tipo de commit:** `chore(repo):`

---

### WP-L — `P2` `audit-drain` retorna `ok:true` mesmo com partições falhando

**Arquivo exclusivo:** `crates/corelink-container/src/routes/audit_drain.rs`

**O defeito.** A resposta de sucesso é montada assim:

```
crates/corelink-container/src/routes/audit_drain.rs:1533-1548
"ok": true,
...
"partitions_failed": partitions_failed,
"incomplete": incomplete,
```

`ok` é literalmente `true`, sempre. Um caller (o cron horário) que checa `ok` vê
sucesso enquanto `partitions_failed > 0` e o selamento da cadeia de auditoria está
silenciosamente travado.

*(Nota: a alegação irmã de que o drain materializa o backlog inteiro na memória é
**falsa** — `AUDIT_DRAIN_BATCH_LIMIT` existe, default 200, em `:405`, e a flag
`incomplete` já sinaliza continuação. Não mexa nisso.)*

**Correção:** `ok` passa a ser `partitions_failed == 0`. **Mantenha `incomplete`
independente** — o comentário em `:1546` está certo: "budget esgotado, re-drenar" é um
sinal diferente de "partição falhou".

**⚠️ Pré-voo obrigatório — risco de tempestade de alertas.** Antes de inverter, leia o
caller: `apps/signup-worker/src/webhooks/audit_drain_cron.ts`. O comentário em `:44`
diz que ele foi escrito para nunca deixar uma falha escapar do `scheduled()`. Se ele
paginar em `ok:false`, e se hoje houver partições falhando de forma crônica, sua
mudança gera uma tempestade de páginas. **Meça primeiro:** rode/inspecione o estado
atual de `partitions_failed` em produção. Se for não-zero de forma crônica, **pare e
reporte** — a correção então precisa vir junto com a causa raiz, não sozinha.

**Testes de aceitação:**
1. Todas as partições ok → `ok:true` (não-regressão).
2. Uma partição falha → `ok:false`, `partitions_failed:1`.
3. Budget esgotado sem falhas → `ok:true`, `incomplete:true` (os sinais permanecem
   independentes).

**Tipo de commit:** `fix(audit):`

---

### WP-M — `P2` Eviction LRU do rate-limit reseta bucket drenado para CHEIO

**Arquivo exclusivo:** `crates/corelink-ratelimit/src/limiter.rs`

**⚠️ Este WP começa com uma PROVA, não com uma correção. Leia a seção inteira antes de
escrever qualquer código.** O comportamento aqui é **conhecido e argumentado no
próprio código** como risco aceito. Se você não conseguir provar o bypass com um
teste, a resposta certa é **reportar, não consertar**.

**O comportamento.** Quando o mapa de buckets atinge o teto, a eviction remove um
entry e o bucket some. Na próxima batida ele re-materializa cheio:

```
crates/corelink-ratelimit/src/limiter.rs:424-430
fn fresh_bucket(&self, now_ms: u64) -> TokenBucketState {
    TokenBucketState::new_full(...)
}
```

**A defesa que o código já opõe** (`:79-91` e `:333-373`), textualmente: a eviction só
pode resetar um bucket para "full", o estado mais permissivo — "eviction only ever
resets a bucket to the most-permissive state, so this is NOT a rate-limit bypass"; e
uma chave quente, sendo a mais recentemente acessada, é estatisticamente improvável de
ser o mínimo de uma amostra de `K = 8` — "an attacker cannot steer eviction onto a key
they are actively hammering". O teto é `LIMITER_BUCKET_MAP_CAP = 100_000` (`:92`),
descrito como deliberadamente generoso.

**A brecha que essa defesa não cobre.** O argumento protege a chave que o atacante
está **martelando**. Mas o bucket que interessa ao atacante é justamente o que ele
**parou** de martelar — ele acabou de drená-lo e está bloqueado nele. Sequência:

```
1. drena o próprio bucket (gasta o burst) → agora está negado
2. PARA de tocar nessa chave → o last_access dela envelhece
3. inunda chaves distintas e baratas (cada uma nasce cheia)
4. a eviction amostrada acaba pegando a chave drenada e envelhecida
5. volta à chave original → re-materializa CHEIA → burst de novo
```

O resultado é um multiplicador sustentado sobre o burst pretendido. Vale para o mapa
per-tenant e, pior, para o velocity gate pré-auth do `_oci`, cujo keyspace é uma string
de path controlada pelo atacante — ou seja, o passo 3 é gratuito e ilimitado para ele.

**PASSO 1, OBRIGATÓRIO — prove ou desista.** Antes de qualquer correção, escreva um
teste que execute exatamente os 5 passos acima e afirme que o atacante obtém **mais
requests permitidos** do que o burst configurado. Ao escrever, você precisa determinar
uma coisa de fato:

> **Uma tentativa NEGADA atualiza o `last_access` do bucket?**

Leia `try_acquire` e o ponto onde `last_access` é escrito. Se um deny atualizar
`last_access`, o passo 2 exige que o atacante fique realmente em silêncio na chave
alvo, e o custo do ataque sobe — mas ele continua viável. Se um deny **não** atualizar,
o ataque é mais barato. **Escreva a resposta que você mediu no corpo do PR** — ela
determina a severidade real.

Se o teste **não** conseguir demonstrar o bypass, **pare**. Reporte com o que mediu, não
force uma correção. Um `STATUS: BLOQUEADO` com medição honesta aqui vale mais que um
patch especulativo.

**PASSO 2 — a correção, só se o passo 1 provou.** Persista o *estado de negação*
separado do bucket: um tombstone de chaves drenadas com TTL, consultado na
re-materialização. Se a última coisa conhecida sobre a chave foi "negada" e o TTL do
tombstone não expirou, `fresh_bucket` a re-materializa com **0 tokens** em vez de
cheia.

O mapa de tombstones precisa ele mesmo ser limitado — senão você reintroduz exatamente
o DoS de crescimento ilimitado que a eviction foi criada para fechar (ver o histórico
em `:74-78`: um OOM-kill do DO singleton `_oci`, que é um apagão cross-tenant). Um
tombstone é muito menor que um bucket, então um teto maior é aceitável, mas ele **tem
de existir**. Diga qual teto escolheu e por quê.

**Não** remova nem afrouxe a eviction amostrada. Ela fecha um DoS de complexidade
algorítmica real (o `O(n)` sob Mutex descrito em `:100-113`). Você está adicionando
uma camada, não substituindo a existente.

**Testes de aceitação:**
1. O teste do passo 1 (o bypass) fica **verde** depois da correção — o atacante não
   obtém mais que o burst.
2. Não-regressão: um tenant honesto cujo bucket foi evictado por pressão legítima de
   memória, e cujo TTL de tombstone expirou, re-materializa cheio.
3. Não-regressão: a eviction continua `O(K)`, nunca `O(n)` — o mapa de tombstones não
   introduz varredura completa.
4. O mapa de tombstones respeita o próprio teto sob flood de chaves distintas.

**Tipo de commit:** `fix(ratelimit):`

---

## 4. MAPA DE CONFLITOS E PLANO DE PARALELIZAÇÃO

### 4.1 Matriz de arquivos — a prova de disjunção

Cada arquivo aparece em **exatamente um** WP. Confira antes de escrever a primeira
linha; se você precisar de um arquivo que não está na sua coluna, **pare e escale**.

| WP | Arquivos que este WP possui |
|---|---|
| A | `crates/corelink-billing-stripe-materializer/src/d1.rs` |
| B | `crates/corelink-failover-router/src/health.rs`, `crates/corelink-failover-router/src/router.rs`, `crates/corelink-replica-worker/src/audit.rs`, `crates/corelink-container/src/routes/failover.rs` |
| C | `crates/corelink-container/src/storage/r2_s3.rs` |
| D1 | `crates/corelink-stripe-real/src/dlq.rs`, `crates/corelink-container/src/main.rs`, `migrations/d1/<novo>.sql` |
| D2 | `scripts/forensics/replay-stripe-webhook.sh`, `crates/corelink-stripe-real/src/webhook_dispatch.rs` |
| E | `worker/src/replication_coordinator_do.ts` |
| F1 | `worker/src/lib/runner_mint.ts`, `worker/src/lib/tenant_suspend_gate.ts`, `worker/src/lib/pat_verify_cache.ts` |
| F2 | `worker/src/index.ts` |
| G | `crates/corelink-clerk/src/adapter.rs` |
| H | `scripts/pre-merge-gate-check.sh`, `.github/workflows/*.yml` |
| I | `crates/corelink-container/src/routes/ratelimit_layer.rs`, `crates/corelink-config-do/src/store.rs`, `crates/corelink-container/src/routes/turbo_v8.rs` |
| J | `crates/corelink-billing-stripe-materializer/src/handler.rs` |
| K | `migrations/*.sql` (raiz apenas — NUNCA `migrations/d1/`), `migrations/neon/`, `TODO.md` |
| L | `crates/corelink-container/src/routes/audit_drain.rs` |
| M | `crates/corelink-ratelimit/src/limiter.rs` |

**Arquivos compartilhados por TODOS os WPs — fonte garantida de conflito:**
`CHANGELOG.md` (uma entrada por PR) e, se aplicável, `BACKLOG.md` e
`docs/internal/secrets-checklist.md`. Protocolo na §5.4.

**Colisões de numeração que precisam de coordenação em tempo real:** o número da
migration nova do WP-D1 versus qualquer coisa que o WP-K toque em `migrations/`. Aloque
o número **no momento do push**.

### 4.2 Ordem

Só existe **uma** dependência dura em todo o plano:

```
WP-D1  ──►  WP-D2      (o replay precisa da DLQ durável)
```

Todo o resto é independente. Isso significa que a **autoria** pode ser 100% paralela.

### 4.3 A restrição que realmente limita a paralelização

Autoria paralela, **push serializado**. Motivo: os runners de CI self-hosted são o Mac
pessoal do dono. Cada push dispara ~13 workflows e custa ~US$ 0,15. Duas compilações
`cargo` concorrentes na caixa já produziram falhas de `lost-communication`. Doze PRs
abertos ao mesmo tempo derrubam a máquina e nada mergeia.

**Regra dura: no máximo 3 PRs abertos simultaneamente.** Quando um mergeia, abra o
próximo.

### 4.4 Escalonamento sugerido

```
FASE 1 — autoria paralela (todos os agentes ao mesmo tempo, cada um no SEU worktree)
  Agente 1: WP-A   (billing resurrect)      P0
  Agente 2: WP-B   (failover)               P0
  Agente 3: WP-C   (AC conditional put)     P1
  Agente 4: WP-D1  (DLQ durável)            P1
  Agente 5: WP-E   (coordinator ts)         P1
  Agente 6: WP-F1  (revoke KV)              P1
  Agente 7: WP-F2  (hex validation)         P2
  Agente 8: WP-G   (JWKS cooldown)          P2
  Agente 9: WP-I   (coleções sem limite)    P2
  Agente 10: WP-J  (status ausente)         P2
  Agente 11: WP-K  (higiene)                P2
  Agente 12: WP-L  (ok:false)               P2
  Agente 13: WP-M  (bucket drenado resetado)  P2

  Cada agente: cria worktree, escreve teste VERMELHO, corrige, roda os gates
  LOCAIS (§5.2), commita — e PARA. Não pusha.

FASE 2 — fila de push, no máximo 3 em voo, na ordem de prioridade
  Onda 1:  A, B, C            (os P0 e o P1 de segurança)
  Onda 2:  D1, E, F1          (após cada merge da onda 1)
  Onda 3:  F2, G, I
  Onda 4:  J, L, M
  Onda 5:  K
  Onda 6:  D2                 (exige D1 mergeado)
  Onda 7:  H                  (por último — é a maior rajada de CI)

Entre ondas: rebase dos branches restantes em origin/main e resolver o CHANGELOG.
```

### 4.5 Se um WP travar

Um WP bloqueado **não bloqueia os outros**. Se você atingir um dos pontos de
escalação marcados no texto (pré-voo falhou, decisão de produto, mudança de deploy),
**entregue todos os outros WPs completos** e reporte explicitamente o que ficou de
fora e por quê. Reduzir o escopo é decisão do dono, não sua.

---

## 5. PROTOCOLO DE EXECUÇÃO — siga literalmente

### 5.1 Setup do worktree (obrigatório, primeiro comando de todo agente)

```bash
cd /Users/gustavoschneiter/Documents/HuGR/corelink-server
git fetch origin
git worktree add -b claude/fix-<WP-ID> .claude/worktrees/fix-<WP-ID> origin/main
cd .claude/worktrees/fix-<WP-ID>

# VERIFICAÇÃO OBRIGATÓRIA — se estas duas linhas não forem idênticas, PARE:
git rev-parse HEAD
git rev-parse origin/main
```

Se os dois hashes divergirem, você está numa base errada. Pare e reporte. Nunca
trabalhe no diretório raiz do repositório.

Toolchain Rust (o proxy do rustup está quebrado neste ambiente — coloque na PATH):

```bash
export PATH="$HOME/.rustup/toolchains/1.91.1-x86_64-apple-darwin/bin:$PATH"
export CARGO_BUILD_JOBS=4
```

A versão fixada é `1.91.1` (`rust-toolchain.toml`). **Não a mude** — um bump exige
emenda ao ADR-0015 e o workflow de reproducible-build passando.

### 5.2 Gates locais (rode ANTES de pushar; um vermelho = não pushe)

```bash
# 1. Specs — deve imprimir 473 com schema completo + 11 YAML-only (484), 0 falhas
python3 scripts/validate_specs.py

# 2. Matriz de secrets — deve terminar com code_only=0 e exit 0
python3 scripts/validate_secrets_matrix.py

# 3. Checklist de secrets — deve dizer OK, sem drift
bash scripts/secrets-checklist-verify.sh

# 4. Backlog — nenhum item DRIFTED nem STALE
python3 scripts/backlog_verify.py

# 5. Rust (só se você tocou em Rust) — do crate que você mexeu, não do workspace
cargo fmt --all
cargo clippy -p <crate> --all-targets -- -D warnings
cargo test -p <crate>

# 6. Worker TypeScript (só se você tocou em worker/ ou apps/)
#    Instale com --legacy-peer-deps. Existem ~14 erros de tsc PRÉ-EXISTENTES;
#    sua barra é "não adicionei nenhum", não "zero erros".
npm install --legacy-peer-deps && npx vitest run

# 7. Drift OKF — ver 5.6
python3 scripts/okf_reconcile.py --json
```

Notas sobre o Rust: use `-p <crate>`, nunca `--workspace`, para não fritar a máquina.
`cargo test` aceita **um** filtro só. Há um flake conhecido de `ct-variance` neste Mac
— se falhar, rode de novo antes de investigar.

### 5.3 Se você adicionar uma variável de ambiente nova

Toda env var nova lida pelo código **precisa** de uma linha em
`docs/internal/secrets-checklist.md`, senão `validate_secrets_matrix.py` acusa
`code_only` drift e o portão fica vermelho. Isso vale para **flags de comportamento**,
não só para segredos.

⚠️ Armadilha conhecida: o scanner da matriz procura `process.env.X`, mas o Worker lê
`env.X`. Se sua var for do Worker, confirme manualmente que ela entrou no inventário —
o scanner é cego para essa forma.

### 5.4 CHANGELOG (a maior fonte de conflito entre PRs paralelos)

Todo commit `feat:` ou `fix:` exige **exatamente uma** entrada sob `## [Unreleased]`
em `CHANGELOG.md` (linha 23). O gate verifica cardinalidade e roda
`scripts/check_changelog_duplicates.py`, que reprova entradas duplicadas por três
sinais independentes.

Protocolo para reduzir conflito:
- Adicione sua entrada **no fim** da subseção apropriada, nunca no topo.
- Uma entrada por PR. Uma frase-líder em negrito descrevendo o defeito e a correção.
- No conflito: `git rebase origin/main`, e na resolução **mantenha as duas** entradas
  (a sua e a que chegou primeiro). Nunca apague a entrada de outro PR.
- Rode `python3 scripts/check_changelog_duplicates.py` depois de resolver.

### 5.5 Formato do commit

```
fix(billing): guard SQL-level impede ressurreição de assinatura cancelada

<corpo: o defeito, a correção, e como foi provado>

Signed-off-by: Gustavo Schneiter <cachorronarigudo26@gmail.com>
Co-Authored-By: Claude Opus 5 <noreply@anthropic.com>
```

⚠️ Os trailers `Signed-off-by:` e `Co-Authored-By:` têm de ser **contíguos**, sem
linha em branco entre eles, ou o gate de DCO reprova.

⚠️ **Commite por nome de arquivo.** Nunca `git add -A`, `git add .` ou `git commit -a`.

```bash
git add crates/corelink-billing-stripe-materializer/src/d1.rs CHANGELOG.md
git commit -F /caminho/para/mensagem.txt
```

### 5.6 OKF — o wiki de arquitetura com anti-drift

`docs/knowledge/` contém 160 conceitos de arquitetura, cada um citando os arquivos e
intervalos de linha que ele explica. Um portão (C5) reprova o PR quando você edita
código citado sem reancorar o conceito.

Vários arquivos deste plano são citados por conceitos:

```
d1.rs                        → 5 conceitos
audit_drain.rs               → 4 conceitos
storage/r2_s3.rs             → 3 conceitos
routes/failover.rs           → 2 conceitos
webhook_dispatch.rs          → 2 conceitos
runner_mint.ts               → 1 conceito
replication_coordinator_do.ts→ 1 conceito
limiter.rs                   → 1 conceito
```

Procedimento **na ordem exata**:

1. Antes de editar, carregue os conceitos da área:
   `python3 scripts/okf_context.py --file <caminho>`
2. Faça a mudança de código e **commite o código primeiro**.
3. `python3 scripts/okf_reconcile.py --json` → leia `stale_count`.
4. Se `stale_count > 0`, rode `scripts/okf-reconcile-local.sh` (reconcilia no
   worktree e deixa staged para revisão) e commite a documentação **num commit
   separado, depois do commit de código**.
5. `python3 scripts/validate_okf.py` tem de sair verde.

⚠️ Duas armadilhas reais: (a) um `git commit --amend` ou rebase depois do reanchor
apaga a âncora — se você rebasear, **reancore de novo**; (b) o C5 no CI compara contra
a base-ref e é **mais estrito** que a checagem local. Local com 0 stale mas CI
vermelho significa que você editou linhas citadas e precisa atualizar `source_blobs`
à mão.

### 5.7 Abertura do PR e merge

```bash
git push -u origin claude/fix-<WP-ID>
gh pr create --title "fix(<escopo>): <resumo de uma linha>" --body-file <corpo.md>
```

O corpo do PR termina com o rodapé:
```
🤖 Generated with [Claude Code](https://claude.com/claude-code)
```

**Merge — um comando, sem pipe, sem exceção:**

```bash
bash scripts/pre-merge-gate-check.sh --merge <PR>
```

Esse script roda o portão e só então faz `gh pr merge --squash`; o merge é
**inalcançável** se qualquer check não estiver verde. Ele também deleta o branch
remoto sozinho.

Proibido:
- `bash scripts/pre-merge-gate-check.sh <PR> | tail -20 && gh pr merge …` — o exit
  status de um pipeline é o do `tail`, a recusa do portão é descartada. Foi
  literalmente assim que um PR entrou com 4 checks pendentes.
- `gh pr merge --admin` sem motivo documentado de infra/flake. Se for realmente
  necessário: `--merge --admin-reason "<por quê>"`, e o script recusa em estados de
  draft/pendente/conflitante.
- Push direto em `main`.

⚠️ Um PR **com conflito** recebe **zero** checks, e um portão ingênuo lê isso como
verde. Sempre confira `mergeable` antes de confiar.

### 5.8 Forma de retorno de cada agente

Ao terminar um WP, retorne **exatamente** isto — sem despejo de log, sem diff
completo:

```
WP: <id>
STATUS: PRONTO-PARA-PUSH | BLOQUEADO | PARCIAL
BRANCH: claude/fix-<id>
ARQUIVOS: <lista exata dos arquivos commitados>
TESTES: <nomes dos testes adicionados> — <contagem de aprovados>
GATES: specs=OK|FAIL secrets=OK|FAIL backlog=OK|FAIL clippy=OK|FAIL okf=OK|FAIL
PROVA: <a linha decisiva mais curta que demonstra o defeito corrigido>
BLOQUEIO: <se BLOQUEADO: qual pré-voo falhou e qual decisão é necessária>
FORA-DE-ESCOPO: <qualquer coisa que você notou e NÃO consertou>
```

---

## 6. REGRAS QUE NÃO SE NEGOCIAM

1. **Teste vermelho primeiro.** Escreva o teste que falha por causa do defeito, veja-o
   falhar, então corrija. Um teste escrito depois da correção prova pouco.
2. **Nenhum WP toca arquivo de outro WP.** Se precisar, pare e escale.
3. **Nenhum WP conserta um item da §2.**
4. **Nada de `git add -A`.** Nunca. Por nome.
5. **Nada de push direto em `main`.** Branch → PR → gate → merge.
6. **Nada de `--admin` merge** sem motivo de infra documentado.
7. **Nada de mudança de deploy** (`wrangler.toml`, secrets, bindings novos) sem
   aprovação do dono. Se sua correção exigir uma, entregue o resto e reporte.
8. **Nada de afrouxar um gate para ficar verde.** Se um portão pegou você, ele está
   certo. Conserte o item ou conserte o mundo — nunca apague a checagem.
9. **Não confie em nenhuma alegação deste documento sem verificar.** Cada
   `arquivo:linha` aqui foi verificado em `origin/main` @ `8cd0f920` em 2026-08-25. Se
   `main` andou, os números podem ter mudado. Confirme por **conteúdo**, não por
   número de linha.
10. **Reporte o que falhou.** Se um teste ficou vermelho, diga-o com a saída. Se você
    pulou um passo, diga. Sucesso silencioso é a classe de defeito dominante neste
    repo.

---

## 7. ITENS DE DECISÃO DO DONO — NÃO IMPLEMENTE, APENAS REPORTE

Estes cinco não estão nos WPs. São decisões de produto, de custo ou de deploy. Se
você "resolver" um deles por conta própria, causará dano.

**OWNER-1 — O GC nunca executa.** `corelink-gc` é dependência declarada
(`crates/corelink-container/Cargo.toml:223`) com **zero** uso em
`crates/corelink-container/src`. Não há rota de GC. `corelink-meta` — o crate que
contém o `INSERT OR IGNORE INTO blob_meta` (`crates/corelink-meta/src/cas_query.rs:45`)
— **não é dependência do container**, e todos os call sites de `MetaStore::commit_put`
estão em testes. Consequência: a tabela `blob_meta` não tem escritor no binário
deployado, e todo blob vive para sempre até o tenant bater o cap de bytes e receber um
402 permanente. Isso **já está documentado** pelo próprio repo — ver o commit
`8cd0f920`, "blob_meta cannot answer findMissingBlobs — it is EMPTY in prod". É um
gap de produto conhecido, com solução grande, e não uma correção de bug. **Não
comece.**

**OWNER-2 — 55 arquivos staged + 16 untracked sem backup.** O diretório principal está
na branch em quarentena com 51 modificados + 4 adicionados staged, 2 modificados não
staged e 16 untracked. Entre os untracked está
`crates/corelink-container/src/gc_worker/` — 13 arquivos Rust que implementam os
stores D1 do GC (refcount, purge, candidates, physical-delete). **Existem apenas nesse
disco.** Zero backup remoto, zero review. Uma falha de disco perde o módulo. Precisa de
uma decisão: fazer land em PRs revisáveis, ou descartar explicitamente. **Não toque
nesses arquivos.**

**OWNER-3 — Cron de chaos herdado em produção.** `wrangler.toml:276` declara
`[triggers]` no nível superior com dois crons. O comentário logo acima (`:259-260`)
afirma que "production env intentionally omits `[triggers]` so chaos NEVER fires in
prod". `[env.prod]` de fato não declara triggers — **mas a API da Cloudflare mostra os
dois crons deployados em `corelink-prod`** (`0 6 * * 1` e `0 14 * * 1`, modificados em
2026-08-25). O wrangler **herda** `[triggers]` para ambientes nomeados. O comentário
está derrotado pela herança.

Hoje isso é inerte, porque `worker/src/index.ts` exporta **somente** `fetch` —
`baseHandler` em `:1928` não tem `scheduled`, e não existe nenhum `async scheduled` em
todo o Worker. Dois bugs se anulando.

**Consequência operacional direta para você:** **não adicione um handler
`scheduled()`.** No instante em que ele existir, o cron de chaos-engineering acorda em
produção. A ordem correta é primeiro decidir o que fazer com a herança (provavelmente
um `[env.prod.triggers]` explícito contendo apenas o drill de página sintética, e não o
de chaos), depois o handler. Essa é uma decisão de deploy do dono.

**OWNER-4 — Reembolso e disputa não revogam nada.** No container, `charge.refunded` é
apenas registro (`webhook_dispatch.rs:1139`) e `charge.dispute.created` tem handler
(`:660`). No signup-worker, nenhum dos dois é tratado. Nenhum escritor revoga acesso
num reembolso. Abuso de chargeback = serviço gratuito até o cancelamento manual.
Fechar isso é decisão de política de produto (um reembolso deve cortar o acesso
imediatamente? no fim do período?), não de engenharia. Reporte, não implemente.

**OWNER-5 — Sem reconciliação de entitlement.** `crates/corelink-billing-reconcile/`
compara camadas de **uso**; nada faz o diff entre as assinaturas do Stripe e as linhas
de `tier_selections`/`tenant_billing`. Um drift da classe do WP-A é invisível.
Construir esse job é trabalho novo com custo recorrente. Reporte.

Dois itens menores para reportar, sem implementar:
- **REAPI:** `QueryWriteStatus` é `unimplemented`
  (`crates/corelink-reapi/src/handler/bytestream.rs:45`), o que quebra upload
  resumível para clientes Bazel reais; e as capabilities anunciam **BLAKE3-only**
  (`capabilities.rs:98`) enquanto o Bazel stock usa SHA256 por padrão — além disso o
  HTTP-Bazel armazena em `bazel/sha256/` (`storage/r2_s3.rs:510`) enquanto
  native/sccache usam BLAKE3, ou seja, os mesmos bytes viram duas cópias e o efeito de
  rede do cache multi-tenant é estruturalmente derrotado. Decisão de conformance +
  estratégia de produto.
- **`sha256_hex` que é FNV-1a:** `crates/corelink-replica-worker/src/replication.rs:411`
  implementa FNV-1a de 64 bits com nome de SHA-256, zero-padded para fingir a largura
  (o próprio doc-comment diz "Simulate"). **Severidade baixa:** o container **não**
  depende de `corelink-replica-worker` para esse caminho, então é código morto, não
  produção. Renomear é trivial; não vale um PR sozinho — pegue de carona quando alguém
  tocar o crate.

---

## 8. TABELA-RESUMO

| WP | P | Título | Arquivos | Depende de |
|---|---|---|---|---|
| A | P0 | Guard SQL contra ressurreição de assinatura | 1 | — |
| B | P0 | Piso de amostragem + histerese + sink limitado no failover | 4 | — |
| C | P1 | PUT condicional no Action Cache | 1 | — |
| D1 | P1 | DLQ de webhook durável em D1 | 3 | — |
| D2 | P1 | Replay de webhook funcional | 2 | **D1** |
| E | P1 | Timestamp server-side no coordinator | 1 | — |
| F1 | P1 | KV-delete no revoke de PAT | 3 | — |
| F2 | P2 | Validação hex da chave principal | 1 | — |
| G | P2 | Cooldown de refresh do JWKS | 1 | — |
| H | P2 | Higiene de CI (rode por último) | ~20 | — |
| I | P2 | Coleções sem limite + drift de sentinelas | 3 | — |
| J | P2 | Status ausente → fail-closed | 1 | — |
| K | P2 | Migrations órfãs + apodrecimento da raiz | vários | — |
| L | P2 | `ok:false` quando partições falham | 1 | — |
| M | P2 | Tombstone p/ bucket drenado (provar antes de corrigir) | 1 | — |

**Definição de pronto para toda a campanha:** os 15 PRs mergeados via
`pre-merge-gate-check.sh --merge`, os cinco portões verdes em `main`
(`validate_specs` 484/0, `validate_secrets_matrix` `code_only=0`,
`secrets-checklist-verify` sem drift, `backlog_verify` sem DRIFTED/STALE,
`validate_okf` verde), e um relatório final nomeando cada item da §7 que continua
aberto e por quê.

---

## 9. RASTREABILIDADE — todo achado tem um destino

Esta tabela existe para que nada suma no meio do caminho. Cada achado da auditoria
original aparece aqui exatamente uma vez, com um destino explícito. Se você encontrar
um achado que não está listado, ele **não** foi avaliado — reporte em vez de agir.

| Achado original | Destino |
|---|---|
| GC/eviction nunca executa | §7 OWNER-1 (confirmado; já documentado no ADR do próprio HEAD) |
| Seed Ed25519 em plaintext | §2 REFUTADO (ausente em `main`, em `origin` e em prod) |
| Gate de secrets vermelho | §2 REFUTADO (`code_only=0`, exit 0) |
| Repo com 57 staged + 15 untracked | §7 OWNER-2 |
| Materializer ressuscita assinatura | **WP-A** |
| Failover sem piso de amostra | **WP-B.1** + **B.2** |
| Revoke de PAT não invalida KV | **WP-F1** |
| Webhook preso + DLQ in-memory + replay morto | **WP-D1** + **WP-D2** |
| Sem gestão de subscription pelo cliente | §2 REFUTADO (portal montado em prod) |
| AC GET-compare-PUT sem PUT condicional | **WP-C** |
| Promoção confia em timestamp do cliente | **WP-E** |
| Eviction LRU reseta bucket drenado | **WP-M** (com prova exigida antes da correção) |
| Audit de failover em `Mutex<Vec>` sem teto | **WP-B.3** |
| Cadência dos heavy gates é ficção | §2 REFUTADO (CLAUDE.md correto; o relatório errou 2 linhas) |
| MED-1 chave principal sem validação hex | **WP-F2** |
| MED-2 JWKS sem cooldown | **WP-G** |
| MED-3 timing pad duplo | **NÃO VERIFICADO** — ver nota abaixo |
| MED-4 staleness de 60s sem ADR | **NÃO VERIFICADO** — ver nota abaixo |
| MED-5 audit de billing in-memory | **WP-D1** (mesmo arquivo `main.rs`) |
| MED-6 status ausente fail-open | **WP-J** |
| MED-7 refund/dispute não revogam | §7 OWNER-4 (decisão de produto) |
| MED-8 sem reconciliação de entitlement | §7 OWNER-5 |
| MED-9 TOCTOU em `route_write` | §2 REFUTADO (arquivo não existe em `main`) |
| MED-10 failback sem catch-up | §2 REFUTADO (arquivo não existe em `main`) |
| MED-11 "sustained" não implementado; `Down` morto | **WP-B.4** |
| MED-12 backlog inteiro em `Vec` / `ok:true` com falhas | metade §2 REFUTADA (existe `BATCH_LIMIT`), metade **WP-L** |
| MED-13 `sha256_hex` é FNV-1a | §7 nota menor (código morto, não é prod) |
| MED-14 `console.log` como audit de promoção | **WP-E** (provavelmente bloqueado — falta binding D1) |
| MED-15 tier congelado por lifetime | **WP-I.1** |
| MED-16 mapas do Config-DO sem poda | **WP-I.2** |
| MED-17 QueryWriteStatus + split sha256/blake3 | §7 nota (conformance + estratégia) |
| MED-18 sentinelas divergentes no turbo | **WP-I.3** |
| MED-19 colisões de migration + órfãs | **WP-K.1** + **K.2** |
| MED-20 crons redundantes | **WP-H.2** (só 2 sobreviveram; o resto já limpo) |
| MED-21 workflows sem concurrency | **WP-H.3** (19, não 21) |
| MED-22 lista de presença do pre-merge | **WP-H.1** |
| MED-23 root rot (`TODO.md`) | **WP-K.3** |
| MED-24 conceito OKF descreve o oposto | §2 REFUTADO (já reconciliado) |
| PERF-3 `runQuotaBatch` não conectado | §2 REFUTADO (conectado em 2 sites) |
| PERF-2 findMissingBlobs serial | §2 escopo corrigido — seam `exists_batch` existe; **fora de escopo** |
| PERF-1 audit sink com 2 inserts D1 síncronos inline | **NÃO VERIFICADO / FORA DE ESCOPO** — ver nota abaixo |
| PERF-4/5/6-9, LOW-* | **NÃO VERIFICADOS** — ver nota abaixo |
| 🆕 Herança de `[triggers]` põe cron de chaos em prod | §7 OWNER-3 (descoberto nesta verificação) |

### Sobre o que NÃO foi verificado

Três itens da auditoria original foram deixados de fora deste plano de forma
deliberada e explícita. Nenhum deles é "resolvido"; são desconhecidos declarados.

- **MED-3** (o pad de timing duplicado entre middleware e verifier, em
  `crates/corelink-worker/src/middleware/auth.rs`) e **MED-4** (propagação de
  scope/find_only com até 60s de staleness sem ADR). Não os re-verifiquei contra
  `main`. Não os implemente às cegas — se algum agente terminar cedo, o trabalho útil
  é **verificar** se ainda valem, e reportar.

- **PERF-1** — a alegação de que o sink de auditoria faz 2 inserts D1 **síncronos e
  inline** por operação de cache (`block_in_place` + `block_on` no request path),
  custando ~100–400ms por operação. Não verifiquei. Mesmo se verdadeira, o próprio
  relatório reconhece que o bloqueio é imposto por uma trait síncrona e que
  fire-and-forget **enfraquece deliberadamente** o invariante de audit-before-mutation.
  Isso é um trade-off de arquitetura, não um bug com correção óbvia — **não é trabalho
  para este briefing.**

- **Toda a lista LOW.** Dado o índice de erro do relatório original (base errada, 2
  linhas de tabela falsas, 2 achados apontando para arquivos inexistentes, 1 alegação
  de ausência derrubada por um único grep), trate cada item LOW como **não
  verificado**. Não implemente nada da lista LOW sem antes confirmar que o defeito
  existe em `main`.

---

*Todo `arquivo:linha` deste documento foi verificado contra `origin/main` @ `8cd0f920`
e contra a API de produção da Cloudflare em 2026-08-25. As alegações refutadas da §2
foram testadas explicitamente e são falsas; não as reintroduza. O SQL do WP-A foi
executado contra as DDLs reais das migrations 0039 e 0055; os resultados medidos estão
transcritos na seção do WP.*
