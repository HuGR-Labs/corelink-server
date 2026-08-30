# WP-S6 — Varredura de verificação da cauda herdada

**Data:** 2026-08-26 · **Base:** `origin/main` @ `8cd0f920`
**Objetivo:** eliminar o débito de itens "não verificados" carregados dos relatórios de
auditoria v1 e v2. Cada item recebe um veredito de três valores. Nenhum item sai desta
campanha sem destino.
**Método:** varredura de localização delegada ao `stealth/ox-alpha` (OpenRouter, free,
via o gateway Omnirouter), com **toda** citação `file:line` reaberta e conferida à mão
antes de entrar nesta tabela. O julgamento de significância é integralmente meu — ver
§3, que documenta por que essa divisão foi necessária.

---

## 1. VEREDITOS

### 1.1 Vira trabalho

| ID | Veredito | Evidência |
|---|---|---|
| **P-A7** | **CONFIRMADO** | `worker/src/index.ts:1436-1439` cria `readSession = env.CONFIG_DB.withSession("first-unconstrained")` — o handle de réplica D1. Ele é consumido **apenas** em `:1444` (`verifyPatRowCached`) e morre no escopo do bloco de auth. As leituras posteriores de tier/quota usam o handle **primário**: `runQuotaBatch(env.CONFIG_DB, …)` em `:3532` e `:4054`. A réplica está provisionada, é feature-detectada corretamente, e o caminho que mais se beneficiaria dela não a recebe. |

**Ação:** um round-trip de far-region por request autenticado, removível threadando
`readSession` para as leituras posteriores. Arquivo é `worker/src/index.ts`, que já
pertence ao **WP-F2+** — entra lá como terceiro item, sem colisão nova.

**Cuidado ao implementar:** `withSession` é feature-detectado (`typeof … === "function"`)
justamente porque runtimes de teste e builds sem replicação não o expõem. O
threading tem de preservar o fallback para `env.CONFIG_DB`, não assumir a réplica.

---

### 1.2 Confirmados, mas o comportamento é DELIBERADO — não "consertar" sem ler o entorno

Estes são os mais perigosos da lista. O fato alegado é verdadeiro; a conclusão
implícita ("logo, otimize") quebraria uma propriedade se aplicada sem ler o comentário
adjacente.

| ID | Fato | O que o entorno diz |
|---|---|---|
| **TB1** | `worker/src/index.ts:1759-1764` percorre as chaves de assinatura em laço serial, sem `Promise.all` | O comentário em `:1760-1761`: *"Do NOT early-return on a match: fold every key so the matching-key identity does not leak via timing."* O laço é **defesa de timing**. `Promise.all` continua válido — o fold vira MAX em vez de SUM e todas as chaves seguem avaliadas — mas quem implementar com early-return ou com short-circuit quebra a propriedade. |
| **TB6** | `worker/src/event_log_do.ts:277-278` faz dois `storage.put` seriais | O comentário em `:274-276`: *"Order matters: if the second put never lands, the entry is orphaned (unreachable via head) rather than the head pointing at a missing entry — read stays gap-free."* É **ordenação crash-safe**. Um `put` multi-chave é estritamente melhor (atômico, elimina o órfão que o comentário hoje tolera) — mas paralelizar os dois puts destrói a garantia. |
| **M-X1** | `crates/corelink-container/src/routes/tier_select_store.rs:273` e `:291` — dois writes D1 não-atômicos em `persist_pending_checkout` | O comentário em `:284-289` declara o trade-off e nomeia o backstop **vivo**: D1-over-HTTP não abre transação entre statements, e o cron diário `corelink_onboarding_stripe_customer_id_drift_total` cobre a janela (1)→(2). Risco aceito e monitorado, não defeito desconhecido. **Pergunta em aberto:** o relatório v2 alega que a API `/raw` do D1 aceita batch atômico — se procede, a premissa do comentário está desatualizada e o upgrade é real. Requer verificar o cliente D1 antes de qualquer coisa. |
| **TB3** | `worker/src/lib/clerk_auth.ts:298` — segunda query D1 serial no fallback do Clerk | A segunda query está dentro de um `else`: só dispara quando o lookup de owner não retorna linha. **O caminho comum já é uma query só.** `UNION ALL` trocaria um join desperdiçado no caso comum por um round-trip salvo no caso raro. Ganho muito menor do que "duas queries seriais" sugere. |
| **MI2** | `worker/src/index.ts:1327` computa SHA-256 do token antes do parse de formato e do HMAC | `:1325-1326`: *"Derive a safe log prefix… Deterministic per token value but non-reversible."* É prefixo seguro de log, e roda **depois** do check de comprimento (`:1309`, 32–256 bytes) e da varredura de charset (`:1316-1322`). SHA-256 sobre ≤256 bytes não é alavanca de DoS — o flood já está limitado antes de chegar aqui. |

**Regra que estes cinco estabelecem:** neste repositório, um laço serial ou um write
não-atômico no caminho de auth ou de dinheiro é **presumidamente deliberado** até prova
em contrário. Leia o comentário imediatamente acima antes de propor a otimização.

---

### 1.3 Confirmados, valor real mas baixo

| ID | Evidência | Nota |
|---|---|---|
| **MED-4** | `worker/src/lib/pat_verify_cache.ts:100` `KV_PAT_ROW_TTL_S = 60`; a linha cacheada carrega `scope` (`:115`), `runner_job_ac_key` (`:116`), `find_only` (`:119`); escrita em `:385` via `kv.put(…, JSON.stringify(row), { expirationTtl })` | Confirma que o L2 cacheia a **row inteira**, não a decisão. Um widening de scope concedido e revertido anda até 60s. Ratificável — precisa do trade-off escrito num ADR, não de código |
| **TB2** | `pat_verify_cache.ts:356`, `tenant_residency_cache.ts:169`, `tenant_suspend_gate.ts:178`, `tenant_tier_cache.ts:154` — todos `kv.get(key)` sem `{type:"json", cacheTtl}` | São **quatro** caches. Entries de 60s casam exatamente com o `cacheTtl` mínimo |
| **TB7** | `tsusp:` (`tenant_suspend_gate.ts:87`), `tres:` (`tenant_residency_cache.ts:62`), `ttier:` (`tenant_tier_cache.ts:72`); nenhuma chave `tmeta:` existe | Unificar economiza round-trips de KV no cold path |
| **TB8** | `worker/src/lib/runner_mint.ts:454`, `:464`, `:478` — três `await env.CONFIG_DB.prepare(...)` seriais, zero `batch` no arquivo | `db.batch` colapsa para um round-trip |
| **TB10** | `worker/src/replication_coordinator_do.ts:474` — `finally { setAlarm(now + INTERVAL) }`, incondicional e eterno; `:495-497` emite `console.warn(replication_no_eligible_replica)` a cada tick sem supressão | Ruído de log constante sobre uma feature cujo feed de heartbeat nunca shipou |
| **MI4** | `crates/corelink-hash/src/digest.rs:66-68` — `to_hex()` faz `hex::encode`, alocando `String` | Uma alocação por linha de log |
| **LA1** | `crates/corelink-pat/src/argon.rs:133` — `if let Some(params) = … { let _ = params; }`, com o próprio comentário admitindo que a checagem real é via accessor tipado | Bloco morto |
| **LA3** | `crates/corelink-pat/src/mint.rs:223` — `let _ = PatHash::from_phc_string(String::new()); // type assertion` | Statement morto |
| **LA9** | `worker/src/index.ts:1699-1703` — `base64urlDecode` re-padda e chama `atob`, aceitando trailing bits não-canônicos; o plano Rust usa `URL_SAFE_NO_PAD.decode`, que rejeita | **A mais interessante das LOW:** a validade de um token passa a depender do plano que o recebe. Não é exploração conhecida, mas é divergência de contrato entre planos |
| **LC3** | `crates/corelink-reapi/src/handler/bytestream.rs:221` — `read_blob` busca o corpo inteiro e só depois fatia via `slice_for_offset_limit` | Mesma família do P-S1; será resolvido junto com o streaming (WP-S2) |
| **LB2** | Verificação de assinatura Stripe em `corelink-billing-stripe/src/signature.rs`, `corelink-tier-selection/src/stripe.rs`, `corelink-container/src/webhook.rs` e no signup-worker TS | Quatro implementações confirmadas em **existência**. Se divergem em comportamento exige leitura comparativa — não feita aqui |

---

### 1.4 Refutados

| ID | Alegação | Realidade |
|---|---|---|
| **MI1** | `TextEncoder` construído em ~7 call sites de `worker/src/index.ts` | **4** sites: `:1315`, `:1642`, `:1853`, `:2102` |
| **MI5** | `parse_digest` faz split e depois re-split | Não faz |
| **LA4** | Doc de `scopes.rs` erra a aritmética ("13 + 51") | **Ambiguidade, não erro.** `scopes.rs:8` diz *"Bits 0..=11 cover the 13 canonical scopes (note: `cache-rw` is an …)"* — 12 bits para 13 **nomes**, porque um é alias. 12 + 52 = 64 fecha. O doc já sinaliza o alias; cabe clarificar a redação, não corrigir número |

---

### 1.5 Não resolvidos

| ID | Estado | O que falta |
|---|---|---|
| **MED-3** | **INDECIDÍVEL por grep** | O arquivo `crates/corelink-worker/src/middleware/auth.rs` existe e o doc da trait em `:148-150` de fato exige que o verifier pade (*"…forgets the pad"*). Não localizei a implementação do pad no middleware com busca por `pad`/`sleep`/`elapsed`. Exige leitura da função, não grep. **Nota de contexto:** o próprio arquivo, em `:54`, justifica a isenção do JWT com *"JWT verification cost is ~5ms regardless"* — que é o L-A11 ("justificativa tecnicamente falsa"). Os dois se resolvem na mesma leitura |
| **MI7** | **NÃO VERIFICADO** | Alegação de `reqwest::blocking` inline em vários crates veio sem `file:line` — a ferramenta respondeu `(multiple files)`. Não aceito sem citação |

---

## 2. BALANÇO

De ~25 itens herdados dos relatórios v1 e v2:

```
 1  vira trabalho        (P-A7 → WP-F2+)
 5  deliberados          (TB1, TB6, M-X1, TB3, MI2)  ← os perigosos
11  nit real, valor baixo
 3  refutados            (MI1, MI5, LA4)
 2  não resolvidos       (MED-3, MI7)
```

**A conclusão que importa: a cauda é rasa.** Os dois relatórios deixaram uma lista longa
pendurada e ela não esconde nenhum P0, nem nada que mude a prioridade das 16 WPs já
planejadas. O único item que vira trabalho já cai num arquivo com dono.

O achado de segunda ordem é mais valioso que qualquer item da lista: **cinco de nove
confirmados são comportamento deliberado com o motivo escrito no comentário adjacente.**
Uma auditoria que lê a linha e não o entorno produz recomendações que quebram
propriedades. Foi o que os relatórios v1 e v2 fizeram repetidamente, e é o motivo de
esta varredura existir.

---

## 3. NOTA DE MÉTODO — o que a delegação ensinou

A varredura de localização foi delegada ao `stealth/ox-alpha` (free, OpenRouter). O
primeiro disparo produziu **zero** saída utilizável em 6 blocos. A causa foi operacional,
minha, e vale registrar porque é reutilizável:

1. **`gw_agent.py --max-tokens` tem default 2048** e eu não sobrescrevi. Um bloco com 25
   claims, num modelo que emite raciocínio como texto visível, estoura isso antes da
   primeira linha de resposta.
2. **O harness só captura bloco `text`** (`gw_agent.py:232-234`). Um modelo que encerra
   o turno numa tool call devolve string vazia — não é falha silenciosa, é o loop
   terminando sem resposta composta. Resolve-se com um contrato de saída explícito:
   *"You MUST end your turn by writing plain text. Never stop on a tool call."*
3. **Tier free tem rate limit.** Seis chamadas em paralelo geram 429. Serializar.

Com `--max-tokens 32000`, contrato de saída explícito e fatias de 3–5 claims, o
resultado mudou completamente: **10 de 10 citações `file:line` corretas**, incluindo
linhas que eu não havia aberto, e duas auto-refutações honestas.

**A limitação que permaneceu, e não é configurável:** a ferramenta localiza o fato e
não lê o entorno. Pedir explicitamente uma coluna `WHY: <cite o comentário adjacente ou
NONE>` devolveu `NONE` mesmo onde o comentário está duas linhas acima da linha citada.
Cinco dos nove confirmados vieram sem o contexto que muda a conclusão.

**Divisão de trabalho que isso estabelece, para as próximas varreduras:** a ferramenta
varre e cita; o julgamento de significância fica com quem lê o entorno. Não é
terceirizável, e agora isso é medição em vez de suposição.

---

*Toda citação `file:line` desta tabela foi reaberta e conferida contra `origin/main` @
`8cd0f920`. Itens marcados INDECIDÍVEL ou NÃO VERIFICADO são exatamente isso — nenhum
foi arredondado para um veredito.*
