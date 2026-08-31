# PROVISIONAMENTO — os dois tenants de teste (ACME e RIVAL)

**Data:** 2026-08-31 · **Sessão:** `estacoes-dois-tenants` · **Autorização:** o owner
autorizou **explicitamente** o uso de `CORELINK_PAT_MINT_AUTH_KEY` do `.env.local`
para provisionar tenants e PATs de teste.

Este documento existe para que **outra pessoa reproduza** os tenants. Sem ele, todas
as medições das estações A-1..A-8 viram relato, não evidência (Q-1).

---

## 0. A correção de rumo que tornou este caminho legítimo

Duas frentes anteriores pararam em *"não existe caminho self-service para obter PAT"*
e **recusaram-se** a usar a chave de mint, citando a regra anti-trapaça
(`GOAL-go-live-validation.md` §5).

**Essa leitura era mais larga do que a regra.** A §5 diz que uma validação que usa a
porta dos fundos **certifica a porta dos fundos**. Isso é decisivo para a estação do
**funil de cadastro** — não se pode declarar o signup validado com uma chave que o
cliente não tem. Mas **não** vale para o que vem *depois* do cadastro: uma vez que o
cliente tenha um PAT (por qualquer via), o cache, o isolamento, a trilha e a medição
de latência são **exatamente os mesmos objetos**. Um PAT cunhado pela chave e um PAT
vindo do signup são **o mesmo artefato** — mesma forma canônica, mesma linha em D1,
mesmo caminho de verificação no edge.

**Consequência registrada honestamente:** uma estação bloqueada (B-1, funil) virou 23.

### O que este caminho NÃO prova — a ressalva que precisa viajar junto

| não prova | por quê |
|---|---|
| **Que o funil de cadastro funciona** | nenhuma conta Clerk foi criada, nenhum cartão passou, nenhum webhook do Stripe rodou |
| **Que o cliente consegue obter um PAT sozinho** | `POST /v1/pats` não está montada; `concepts/tenancy.md:38` diz que o PAT do signup é a única via self-service. **Isso segue sendo achado aberto** |
| **Que o tier gravado é o tier escolhido** | os tenants nascem sem assinatura; `tier` não foi exercido |
| **Que o e-mail / identidade do tenant está correto** | `email_hash` é derivado do UUID, não de um e-mail real |
| **Que a permissão chega ao D1 pelo caminho de produção** | aqui **eu** escrevi a linha; em produção quem escreve é o signup-worker |

**A estação B-1 (funil de cadastro) continua NÃO VALIDADA por este caminho.** Tudo o
que depende exclusivamente de possuir um PAT válido passa a ser mensurável.

---

## 1. O fato que faz o provisionamento falhar se você não souber

`POST /_internal/pat/mint` é uma **função pura**. Ela calcula o HMAC do token e o
hash Argon2id e **devolve** — ela **não persiste nada**. Quem escreve a linha em D1 é
o chamador (em produção, o signup-worker / o `session_exchange`).

> **Se o PAT não autenticar depois de cunhado, é isto.** O token existe, a forma é
> válida, e o edge não acha a linha: **401**, indistinguível de token inventado.

E há um segundo pré-requisito: D1 impõe a FK `pat.tenant_id → tenant(tenant_id)`.
Cunhar um PAT para um UUID sem linha em `tenant` faz o INSERT falhar com
`FOREIGN KEY constraint failed` (7500) — token cunhado, **não** persistido, inerte.

**Portanto a ordem é obrigatória: `tenant` primeiro, `pat` depois.**

---

## 2. Os comandos exatos

Pré-requisito comum (as duas etapas leem `.env.local`; ele é gitignored):

```bash
cd /Users/gustavoschneiter/Documents/HuGR/corelink-server
set -a && source .env.local && set +a
export WRANGLER=<caminho>/worker/node_modules/.bin/wrangler   # 4.111.0
```

Os UUIDs foram gerados na hora (`python3 -c 'import uuid;print(uuid.uuid4())'`) —
**nada de UUID conhecido, nada de tenant que já existia**:

| persona | tenant_id | região | escopos cunhados |
|---|---|---|---|
| **ACME** | `97f9827a-e185-4f23-99d4-5989dadeee73` | `enam` | `cas:rw` |
| **RIVAL** | `7080e06d-e948-4864-a7cb-19b30e7fab3f` | `enam` | `cas:rw` + `read-only` |

### Passo 1 — a linha `tenant` (FK)

```bash
scripts/admin/seed-dogfood-tenant.sh --tenant <UUID> --region enam --yes
```

O que faz: `INSERT OR IGNORE INTO tenant (tenant_id, primary_region, tenant_state,
email_hash, created_at_ms, updated_at_ms, created_ms, updated_ms)` no
`corelink-config-prod` (binding `CONFIG_DB`, env `prod`, `--remote`).
`email_hash` é SHA-256 de `dogfood:<uuid>` — satisfaz o `NOT NULL` **sem inventar um
endereço plausível**. Idempotente.

Saída observada (ACME), com o contador que decide:

```
"changes": 1, "rows_written": 7, "last_row_id": 428
[seed-dogfood-tenant] ✅ tenant row seeded (idempotent).
```

RIVAL: `"changes": 1` idem. **`changes` é a prova; `success: true` não é** — a mesma
resposta com `changes: 0` significa que o `OR IGNORE` engoliu a escrita.

### Passo 2 — cunhar + persistir o PAT

```bash
scripts/admin/mint-dogfood-pat.sh --tenant <UUID> --scope cas:rw \
  --ttl-seconds 86400 --yes > tok.txt
```

O que faz, nesta ordem:
1. `POST https://corelink-api.humangr.com/_internal/pat/mint` com
   `x-corelink-internal-auth: $CORELINK_PAT_MINT_AUTH_KEY` **e** o par de headers de
   Cloudflare Access (`CF-Access-Client-Id` / `-Secret`, também do `.env.local`).
   **Sem o par de Access a chamada volta 403 com HTML do Access, não erro do
   CoreLink** — Access está na frente de `/_internal/*` no hostname de produção.
2. `INSERT INTO pat (...)` no `CONFIG_DB` prod com o `pat_hash` (Argon2id) que a
   resposta devolveu. `scope` é gravado **canonicalizado** (`cas:rw` → `read-write`),
   porque o CHECK da coluna só aceita `('read-write','read-only','admin')`.
3. Imprime o plaintext **uma vez** em stdout.

Resultados (o token **nunca** aparece aqui — só prefixo/tenant, por invariante):

| persona | escopo persistido | `token_id` | `pat_id` |
|---|---|---|---|
| ACME | `read-write` | `TCDY2W1TC21Q569T` | `01a058cb-47ff-7ef1-bc5e-5f579d53a28f` |
| RIVAL | `read-write` | `H5CPPMNTKR8M5QZ4` | `01a058cb-753f-7412-8485-4a8bc9c6902f` |
| RIVAL (leitura) | `read-only` | `67BE6D1QXPRG5RF5` | `01a058cb-82e9-76c0-b0d7-2a81c0551e2c` |

Todos os três: `"changes": 1` no INSERT, `expires_ms` a 24 h.

### Forma do token — confirmada, contra os exemplos publicados

```
len=96  dots=2  prefixo=corelink_pat_<token_id de 16 chars Crockford b32>
```

Os três tokens: **96 chars, exatamente 2 pontos**. Confirma a forma canônica e
**confirma o achado já registrado** de que os exemplos publicados (26/37/53 chars)
não parseiam. `worker/src/index.ts:1350-1351` documenta o mesmo: total 95 (env de 2
chars) ou 96 (env `pat`, 3 chars).

---

## 3. CALIBRAÇÃO — a prova de que o instrumento enxerga

Nenhum número das estações vale se os três desfechos abaixo não forem **distintos**.
Rodado **antes** de qualquer medição:

```
GET /v1/users/me  ·  PAT do ACME
  http=200  {"tenant_id":"97f9827a-e185-4f23-99d4-5989dadeee73","token_prefix":"H5TgaU","route_kind":"reapi_v1"}

GET /v1/users/me  ·  PAT do RIVAL
  http=200  {"tenant_id":"7080e06d-e948-4864-a7cb-19b30e7fab3f","token_prefix":"iq9gHE","route_kind":"reapi_v1"}

GET /v1/users/me  ·  SEM auth                       → http=401  UNAUTHORIZED
GET /v1/users/me  ·  PAT forjado (96 chars, 2 pontos, assinatura trocada por 'z')
                                                     → http=401  UNAUTHORIZED
```

**Quatro entradas, três desfechos distintos, e cada PAT resolve para o SEU tenant.**
O instrumento discrimina. Um 401 nas estações significa "recusado", não "meu comando
quebrou".

### Refutação que falhou (Q-6) — o `token_prefix` não é vazamento

`token_prefix: "H5TgaU"` **não** é o começo do token (que é `corelink_pat_TCDY2W…`).
Levantei a hipótese de disclosure parcial do segredo e ela **caiu**:
`worker/src/index.ts:1344-1348` deriva-o como
`base64url(SHA-256(token)).slice(0,6)` — "deterministic per token value but
non-reversible". Handle de correlação, não segredo. Registro a refutação porque ela
calibra o quanto confiar no resto.

---

## 4. Teardown

Os três PATs expiram sozinhos em 24 h (`ttl-seconds 86400`). O PAT `read-only` do
RIVAL foi **revogado durante a rodada** (teste de revogação):

```sql
UPDATE pat SET revoked_at_ms=1788196418507 WHERE token_id='67BE6D1QXPRG5RF5';  -- changes=1
```

As linhas `tenant` e os blobs escritos em R2 **permanecem** — deliberadamente, para
que a estação de apagamento (GDPR Art. 17) tenha um alvo real com dados de duas
personas distintas. Existe um espelho de teardown (`teardown-dogfood-tenant.sh`)
quando for hora de limpar.

---

## 5. Higiene de segredo

- Os plaintexts foram escritos **apenas** no scratchpad da sessão, nunca no repo,
  nunca em log, nunca neste documento.
- `.env.local` **tem chaves LIVE** sob nomes `*_LIVE_*`. Nada deste documento cita
  valor de segredo — só **nomes** de variáveis.
- O que se reporta de um PAT é **`token_id` + tenant**, nunca o token.
- Verificado depois, contra a trilha: **0 ocorrências** de `corelink_pat` / `Bearer` /
  `argon2` nos 48 eventos de auditoria dos dois tenants — com **controle positivo** na
  mesma consulta (48 ocorrências de `digest`), para que o zero não seja cegueira do
  instrumento.
